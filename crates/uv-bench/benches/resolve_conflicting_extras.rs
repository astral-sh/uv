//! Universal lock resolution with mutually exclusive extras and a shared dependency that
//! requires different versions across Python environments, as in astral-sh/uv#21954.

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::cell::LazyCell;
use std::fmt::Write;
use std::path::Path;
use std::process::ExitCode;

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use clap::Parser;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main, measurement::WallTime};
use futures::executor::block_on;
use uv::GlobalInitialization;
use uv_cli::Cli;

/// Create an offline project with mutually exclusive extras and a shared dependency whose
/// releases force resolution to fork across Python versions.
fn create_project(root: &Path, count: usize) {
    let mut pyproject = String::from(
        r#"[project]
name = "conflicting-extras"
version = "0.1.0"
requires-python = ">=3.10,<3.15"
dependencies = ["shared>=1,<2"]

[project.optional-dependencies]
"#,
    );
    for index in 0..count {
        writeln!(pyproject, "e{index:03} = []").expect("Writing to a string cannot fail");
    }
    pyproject.push_str("\n[tool.uv]\npackage = false\nconflicts = [[\n");
    for index in 0..count {
        writeln!(pyproject, "    {{ extra = \"e{index:03}\" }},")
            .expect("Writing to a string cannot fail");
    }
    pyproject.push_str("]]\n");
    fs_err::write(root.join("pyproject.toml"), pyproject)
        .expect("Failed to write benchmark project");

    let wheels = root.join("wheels");
    fs_err::create_dir(&wheels).expect("Failed to create wheel directory");
    // The newer release drops older Python versions, forcing a universal resolution fork.
    for (version, requires_python) in [("1.0.0", ">=3.10"), ("1.0.1", ">=3.12")] {
        let metadata = format!(
            "Metadata-Version: 2.3\nName: shared\nVersion: {version}\nRequires-Python: {requires_python}\n"
        );
        let mut writer = ZipFileWriter::new(Vec::new());
        for (filename, contents) in [
            ("METADATA", metadata.as_str()),
            (
                "WHEEL",
                "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
            ),
            ("RECORD", ""),
        ] {
            let entry = ZipEntryBuilder::new(
                format!("shared-{version}.dist-info/{filename}").into(),
                Compression::Stored,
            );
            block_on(writer.write_entry_whole(entry, contents.as_bytes()))
                .expect("Failed to write wheel entry");
        }
        fs_err::write(
            wheels.join(format!("shared-{version}-py3-none-any.whl")),
            block_on(writer.close()).expect("Failed to finish wheel"),
        )
        .expect("Failed to write wheel");
    }
}

/// Build an offline lock invocation that resolves even if inherited settings request locked or
/// frozen mode. The caller removes the lockfile before each resolution.
fn lock_cli(root: &Path, python: &Path) -> Cli {
    Cli::try_parse_from([
        "uv".as_ref(),
        "lock".as_ref(),
        "--project".as_ref(),
        root.as_os_str(),
        "--python".as_ref(),
        python.as_os_str(),
        "--cache-dir".as_ref(),
        root.join("cache").as_os_str(),
        "--find-links".as_ref(),
        root.join("wheels").as_os_str(),
        "--no-index".as_ref(),
        "--offline".as_ref(),
        "--no-python-downloads".as_ref(),
        "--no-build".as_ref(),
        "--no-config".as_ref(),
        "--no-locked".as_ref(),
        "--no-frozen".as_ref(),
        "--quiet".as_ref(),
    ])
    .expect("Failed to parse lock benchmark arguments")
}

fn run_lock(runtime: &tokio::runtime::Runtime, cli: Cli, initialization: GlobalInitialization) {
    let status = runtime
        .block_on(uv::run(cli, initialization))
        .expect("Failed to lock benchmark project");
    assert_eq!(ExitCode::from(status), ExitCode::SUCCESS, "Lock failed");
}

/// Measure fresh resolutions with warm metadata caches, excluding fixture creation, runtime
/// initialization, argument parsing, and lockfile removal from each sample.
fn resolve_conflicting_extras(criterion: &mut Criterion<WallTime>) {
    let mut initialization = GlobalInitialization::Initialize;
    let mut group = criterion.benchmark_group("lock_conflicting_extras");
    for count in [4, 16, 32] {
        // Initialize each scenario once, and only when Criterion selects it.
        let scenario = LazyCell::new(|| {
            let project = tempfile::tempdir().expect("Failed to create benchmark directory");
            create_project(project.path(), count);
            let python = fs_err::canonicalize("../../.venv")
                .expect("Benchmark virtual environment should exist");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .max_blocking_threads(256)
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");
            let lockfile = project.path().join("uv.lock");

            run_lock(&runtime, lock_cli(project.path(), &python), initialization);
            initialization = GlobalInitialization::Reuse;

            let lock: toml::Value = toml::from_str(
                &fs_err::read_to_string(&lockfile).expect("Failed to read benchmark lockfile"),
            )
            .expect("Failed to parse benchmark lockfile");
            assert_eq!(
                lock["conflicts"][0]
                    .as_array()
                    .expect("Lockfile should contain the conflict group")
                    .len(),
                count
            );
            let versions: Vec<_> = lock["package"]
                .as_array()
                .expect("Lockfile should contain packages")
                .iter()
                .filter(|package| package["name"].as_str() == Some("shared"))
                .map(|package| {
                    package["version"]
                        .as_str()
                        .expect("Package should have a version")
                })
                .collect();
            assert_eq!(versions, ["1.0.0", "1.0.1"]);

            // Warm metadata cache reads while resolving from scratch.
            fs_err::remove_file(&lockfile).expect("Failed to remove benchmark lockfile");
            run_lock(&runtime, lock_cli(project.path(), &python), initialization);
            (project, python, runtime)
        });

        let name = format!("uv_lock_no_lockfile_warm_metadata_cache_{count}_conflicting_extras");
        group.bench_function(name, |benchmark| {
            let (project, python, runtime) = &*scenario;
            let lockfile = project.path().join("uv.lock");
            benchmark.iter_batched(
                || {
                    // A retained lockfile would measure the up-to-date check instead of a solve.
                    // CodSpeed can invoke setup twice before running a measured iteration.
                    if lockfile
                        .try_exists()
                        .expect("Failed to check benchmark lockfile")
                    {
                        fs_err::remove_file(&lockfile)
                            .expect("Failed to remove benchmark lockfile");
                    }
                    lock_cli(project.path(), python)
                },
                |cli| run_lock(runtime, cli, GlobalInitialization::Reuse),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, resolve_conflicting_extras);
criterion_main!(benches);
