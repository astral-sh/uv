//! Warm, offline conflict benchmarks. These are reduced dependency graphs, not timings of the
//! motivating projects. See the fixture constructors for their sizes and historical regressions.
//!
//! Like the resolver benchmarks, these use the repository's `.venv`. Fixture generation, interpreter
//! discovery, cache warming, and output checks are outside measurement. Each measured invocation
//! starts with a fresh workspace cache, but reuses the disk cache and process-global marker interner.
//! Lock benchmarks remove the previous lockfile outside measurement, so they include resolution,
//! conflict simplification, and serialization rather than measuring the up-to-date-lock fast path.

// Don't optimize the alloc crate away due to it being otherwise unused.
extern crate uv_performance_memory_allocator;

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use clap::Parser;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use uv::GlobalInitialization;
use uv::commands::ExitStatus;
use uv_cache::Cache;
use uv_cli::Cli;
use uv_python::PythonEnvironment;
use uv_resolver::{Lock, PylockToml};

const SHARED_PACKAGES: usize = 8;
const WORKSPACE_MEMBERS: usize = 24;
const BACKENDS: usize = 24;

struct Fixture {
    directory: TempDir,
    pyproject: toml::Table,
    packages: BTreeSet<(String, String)>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("Failed to create benchmark directory");
        fs_err::create_dir(directory.path().join("wheels"))
            .expect("Failed to create wheel directory");
        let mut fixture = Self {
            directory,
            pyproject: toml::toml! {
                [project]
                name = "conflict-benchmark"
                version = "1.0.0"
                requires-python = ">=3.11"
                dependencies = []
                optional-dependencies = {}

                [tool.uv]
                package = false
                conflicts = []
                sources = {}
                workspace = { members = [] }
            },
            packages: BTreeSet::from([("conflict-benchmark".to_string(), "1.0.0".to_string())]),
        };
        // A short shared chain, with a platform-dependent version at its leaf. This keeps platform
        // conditions mixed with conflict conditions rather than benchmarking only Boolean extras.
        for index in 0..SHARED_PACKAGES {
            let dependencies = if index + 1 < SHARED_PACKAGES {
                vec![format!("shared-{}==1.0.0", index + 1)]
            } else {
                vec![
                    "platform-dep==1.0.0; sys_platform == 'linux'".to_string(),
                    "platform-dep==2.0.0; sys_platform != 'linux'".to_string(),
                ]
            };
            fixture.wheel(
                &format!("shared-{index}"),
                "1.0.0",
                &dependencies,
                (index == 0).then_some("feature"),
            );
        }
        fixture.wheel("platform-dep", "1.0.0", &[], None);
        fixture.wheel("platform-dep", "2.0.0", &[], None);
        fixture
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self) {
        fs_err::write(
            self.root().join("pyproject.toml"),
            toml::to_string(&self.pyproject).expect("Failed to serialize benchmark project"),
        )
        .expect("Failed to write benchmark project");
        // An explicit config file prevents repository/user index and build policies from leaking
        // into these fixtures, while leaving their project metadata available to discovery.
        let wheels = self.root().join("wheels");
        let config = toml::toml! {
            no-index = true
            no-build = true
            find-links = [(wheels.to_str().expect("Wheel path must be UTF-8"))]
        };
        fs_err::write(
            self.root().join("uv.toml"),
            toml::to_string(&config).expect("Failed to serialize benchmark configuration"),
        )
        .expect("Failed to write benchmark configuration");
    }

    /// Write a tiny, installable wheel with static metadata; no index access or builds are needed.
    fn wheel(&mut self, name: &str, version: &str, dependencies: &[String], extra: Option<&str>) {
        let stem = format!("{}-{version}", name.replace('-', "_"));
        let mut metadata = format!(
            "Metadata-Version: 2.1\nName: {name}\nVersion: {version}\nRequires-Python: >=3.11\n"
        );
        for dependency in dependencies {
            writeln!(metadata, "Requires-Dist: {dependency}").expect("Failed to write metadata");
        }
        if let Some(extra) = extra {
            writeln!(metadata, "Provides-Extra: {extra}").expect("Failed to write metadata");
        }
        let entries = [
            (format!("{stem}.dist-info/METADATA"), metadata),
            (
                format!("{stem}.dist-info/WHEEL"),
                "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n".to_string(),
            ),
        ];
        let mut writer = ZipFileWriter::new(Vec::new());
        let mut record = String::new();
        for (path, contents) in entries {
            writeln!(record, "{path},,").expect("Failed to write wheel record");
            let entry = ZipEntryBuilder::new(path.into(), Compression::Stored);
            block_on(writer.write_entry_whole(entry, contents.as_bytes()))
                .expect("Failed to write wheel entry");
        }
        let path = format!("{stem}.dist-info/RECORD");
        writeln!(record, "{path},,").expect("Failed to write wheel record");
        let entry = ZipEntryBuilder::new(path.into(), Compression::Stored);
        block_on(writer.write_entry_whole(entry, record.as_bytes()))
            .expect("Failed to write wheel record");
        fs_err::write(
            self.root()
                .join("wheels")
                .join(format!("{stem}-py3-none-any.whl")),
            block_on(writer.close()).expect("Failed to finish wheel"),
        )
        .expect("Failed to write wheel");
        self.packages
            .insert((name.to_string(), version.to_string()));
    }

    fn remove_lock(&self) {
        let path = self.root().join("uv.lock");
        // CodSpeed may call the batch setup more than once before invoking the measured routine.
        if path.exists() {
            fs_err::remove_file(path).expect("Failed to remove benchmark lockfile");
        }
    }

    fn check_lock(&self) {
        let contents = fs_err::read_to_string(self.root().join("uv.lock"))
            .expect("Failed to read benchmark lockfile");
        let lock: Lock = toml::from_str(&contents).expect("Failed to parse benchmark lockfile");
        let packages = lock
            .packages()
            .iter()
            .map(|package| {
                (
                    package.name().to_string(),
                    package
                        .version()
                        .expect("Fixture packages have versions")
                        .to_string(),
                )
            })
            .collect();
        assert_eq!(
            self.packages, packages,
            "Lock must retain every alternative"
        );
    }
}

/// Preserve sktime's 15 overlapping pairs from the report behind #18094 (issue #18026), including
/// its multiple overlapping CI pins, rather than using hundreds of independent conflict sets.
/// <https://github.com/sktime/sktime/blob/4a5ab785285b7615cedd8a617c84d09dee47f6b0/pyproject.toml>
fn overlapping() -> Fixture {
    let mut fixture = Fixture::new();
    let pairs = [
        ("dependencies_lowest", "dependencies_lower"),
        ("dependencies_lowest", "all_extras"),
        ("dependencies_lowest", "all_extras_pandas2"),
        ("dl", "dependencies_lowest"),
        ("forecasting", "dependencies_lowest"),
        ("notebooks", "dependencies_lowest"),
        ("pandas1", "dependencies_lower"),
        ("dependencies_2024", "dependencies_lowest"),
        ("classification", "dependencies_lowest"),
        ("networks", "dependencies_lowest"),
        ("regression", "dependencies_lowest"),
        ("dependencies_2024", "dependencies_lower"),
        ("notebooks", "dependencies_2024"),
        ("numpy1", "dependencies_2024"),
        ("pandas1", "dependencies_2024"),
    ];
    let mut extras = toml::Table::new();
    let mut conflicts = Vec::new();
    for (index, (left, right)) in pairs.into_iter().enumerate() {
        // A separate version disagreement per pair preserves exactly the declared compatibility
        // graph: extras that do not conflict can still be selected together.
        let name = format!("choice-{index}");
        for (extra, version) in [(left, "1.0.0"), (right, "2.0.0")] {
            fixture.wheel(&name, version, &["shared-0".to_string()], None);
            extras
                .entry(extra)
                .or_insert_with(|| toml::Value::Array(Vec::new()))
                .as_array_mut()
                .expect("Extra requirements are arrays")
                .push(format!("{name}=={version}").into());
        }
        conflicts.push(toml::Value::Array(vec![
            toml::toml! { extra = (left) }.into(),
            toml::toml! { extra = (right) }.into(),
        ]));
    }
    fixture.pyproject["project"]["optional-dependencies"] = extras.into();
    fixture.pyproject["tool"]["uv"]["conflicts"] = conflicts.into();
    fixture.write();
    fixture
}

/// The report behind #19538 (#16779) had 24 defined extras. Model that scale with one
/// mutually exclusive set and distinct backend versions. This is not a copy of its dependency
/// graph or its conflict declarations, which also contained duplicate and undefined extra names.
/// <https://github.com/alex-shapiro/PufferLib/blob/21807b34145822e307314dd0e3b503139c6aaa97/pyproject.toml>
fn mutually_exclusive() -> Fixture {
    let mut fixture = Fixture::new();
    let mut extras = toml::Table::new();
    let mut conflicts = Vec::new();
    for index in 1..=BACKENDS {
        let extra = format!("backend-{index}");
        let version = format!("{index}.0.0");
        // Requesting a transitive extra is essential to the `without_extras` regression in #19538.
        fixture.wheel(
            "backend",
            &version,
            &["shared-0[feature]".to_string()],
            None,
        );
        extras.insert(extra.clone(), vec![format!("backend=={version}")].into());
        conflicts.push(toml::Value::from(toml::toml! { extra = (extra) }));
    }
    fixture.pyproject["project"]["optional-dependencies"] = extras.into();
    fixture.pyproject["tool"]["uv"]["conflicts"] = vec![toml::Value::Array(conflicts)].into();
    fixture.write();
    fixture
}

/// #20211, #20578, and #20611 optimized expansion and deduplication of included group conflicts.
/// One CPU/GPU pair expands through two small inclusion diamonds (eight groups, 16 resulting
/// conflict pairs); neither thousands of groups nor empty groups are needed to exercise this path.
fn included_groups() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.wheel("backend", "1.0.0", &["shared-0".to_string()], None);
    fixture.wheel("backend", "2.0.0", &["shared-0".to_string()], None);
    fixture.pyproject.insert(
        "dependency-groups".to_string(),
        toml::toml! {
            cpu = ["backend==1.0.0"]
            gpu = ["backend==2.0.0"]
            dev = [{ include-group = "cpu" }]
            test = [{ include-group = "cpu" }]
            ci = [{ include-group = "dev" }, { include-group = "test" }]
            gpu-dev = [{ include-group = "gpu" }]
            gpu-test = [{ include-group = "gpu" }]
            gpu-ci = [{ include-group = "gpu-dev" }, { include-group = "gpu-test" }]
        }
        .into(),
    );
    fixture.pyproject["tool"]["uv"]["conflicts"] = vec![toml::Value::Array(vec![
        toml::toml! { group = "cpu" }.into(),
        toml::toml! { group = "gpu" }.into(),
    ])]
    .into();
    fixture.write();
    fixture
}

/// A reduced workspace fan-in for #21399: 24 members' `test` extras share an eight-package chain,
/// with just one CPU/GPU conflict. The identical graph without that declaration is the control.
/// Frozen traversal also exercises activated-item encoding (#21148) and extra/marker evaluation.
fn shared_extras(conflicts: bool) -> Fixture {
    let mut fixture = Fixture::new();
    fixture.wheel("cpu-backend", "1.0.0", &["shared-0".to_string()], None);
    fixture.wheel("gpu-backend", "1.0.0", &["shared-0".to_string()], None);
    let mut sources = toml::Table::new();
    let mut dependencies = Vec::new();
    for index in 0..WORKSPACE_MEMBERS {
        let name = format!("member-{index}");
        let directory = fixture.root().join(&name);
        fs_err::create_dir(&directory).expect("Failed to create member directory");
        let member = toml::toml! {
            [project]
            name = (name.clone())
            version = "1.0.0"
            requires-python = ">=3.11"
            [project.optional-dependencies]
            test = ["shared-0"]
            [tool.uv]
            package = false
        };
        fs_err::write(
            directory.join("pyproject.toml"),
            toml::to_string(&member).expect("Failed to serialize member"),
        )
        .expect("Failed to write member");
        dependencies.push(format!("{name}[test]"));
        sources.insert(name.clone(), toml::toml! { workspace = true }.into());
        fixture.packages.insert((name, "1.0.0".to_string()));
    }
    fixture.pyproject["project"]["dependencies"] = dependencies.into();
    fixture.pyproject["project"]["optional-dependencies"] = toml::toml! {
        cpu = ["cpu-backend"]
        gpu = ["gpu-backend"]
    }
    .into();
    fixture.pyproject["tool"]["uv"]["sources"] = sources.into();
    fixture.pyproject["tool"]["uv"]["workspace"] = toml::toml! { members = ["member-*"] }.into();
    if conflicts {
        fixture.pyproject["tool"]["uv"]["conflicts"] = vec![toml::Value::Array(vec![
            toml::toml! { extra = "cpu" }.into(),
            toml::toml! { extra = "gpu" }.into(),
        ])]
        .into();
    }
    fixture.write();
    fixture
}

struct Harness {
    runtime: Runtime,
    python: PathBuf,
    initialization: GlobalInitialization,
}

impl Harness {
    fn new() -> Self {
        let cache = Cache::temp().expect("Failed to create interpreter cache");
        let environment = PythonEnvironment::from_root("../../.venv", &cache)
            .expect("Create the repository's .venv before running benchmarks");
        Self {
            runtime: tokio::runtime::Builder::new_current_thread()
                .max_blocking_threads(256)
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
            python: environment.interpreter().sys_executable().to_path_buf(),
            initialization: GlobalInitialization::Initialize,
        }
    }

    fn run(&mut self, fixture: &Fixture, args: &[&str]) {
        let cache = fixture.root().join("cache");
        let config = fixture.root().join("uv.toml");
        let cli = Cli::try_parse_from(
            [
                "uv",
                "--quiet",
                "--offline",
                "--config-file",
                config.to_str().expect("Config path must be UTF-8"),
                "--no-python-downloads",
                "--project",
                fixture.root().to_str().expect("Fixture path must be UTF-8"),
                "--cache-dir",
                cache.to_str().expect("Cache path must be UTF-8"),
            ]
            .into_iter()
            .chain(args.iter().copied())
            .chain([
                "--python",
                self.python.to_str().expect("Python path must be UTF-8"),
            ]),
        )
        .expect("Failed to parse benchmark arguments");
        let status = self
            .runtime
            .block_on(uv::run(cli, self.initialization))
            .expect("Benchmark invocation failed");
        self.initialization = GlobalInitialization::Reuse;
        let success = match status {
            ExitStatus::Success => true,
            ExitStatus::Failure | ExitStatus::Error | ExitStatus::External(_) => false,
        };
        assert!(success, "Benchmark invocation did not succeed");
    }

    fn lock(&mut self, criterion: &mut Criterion, name: &str, fixture: &Fixture) {
        self.run(fixture, &["lock"]);
        fixture.check_lock();
        fixture.remove_lock();
        self.run(fixture, &["lock"]);
        criterion.bench_function(name, |bencher| {
            bencher.iter_batched(
                || fixture.remove_lock(),
                |()| self.run(fixture, &["lock"]),
                BatchSize::PerIteration,
            );
        });
        fixture.check_lock();
    }

    fn export_args<'a>(output: &'a str, selection: &'a [&'a str]) -> Vec<&'a str> {
        let mut args = vec![
            "export",
            "--frozen",
            "--no-default-groups",
            "--format",
            "pylock.toml",
            "--output-file",
            output,
        ];
        args.extend_from_slice(selection);
        args
    }

    /// Check both sides of a conflict outside measurement, not just that locking terminates.
    fn check_selection(
        &mut self,
        fixture: &Fixture,
        selection: &[&str],
        name: &str,
        versions: &[&str],
    ) {
        let output = fixture.root().join("pylock.toml");
        let args = Self::export_args(
            output.to_str().expect("Output path must be UTF-8"),
            selection,
        );
        self.run(fixture, &args);
        let contents = fs_err::read_to_string(output).expect("Failed to read exported lockfile");
        let lock: PylockToml =
            toml::from_str(&contents).expect("Failed to parse exported lockfile");
        let selected: BTreeSet<_> = lock
            .packages
            .iter()
            .filter(|package| package.name.as_ref() == name)
            .map(|package| {
                package
                    .version
                    .as_ref()
                    .expect("Wheel has a version")
                    .to_string()
            })
            .collect();
        assert_eq!(
            selected,
            versions
                .iter()
                .map(|version| (*version).to_string())
                .collect()
        );
    }

    fn export(
        &mut self,
        criterion: &mut Criterion,
        name: &str,
        fixture: &Fixture,
        selection: &[&str],
    ) {
        let output = fixture.root().join("pylock.toml");
        let args = Self::export_args(
            output.to_str().expect("Output path must be UTF-8"),
            selection,
        );
        self.run(fixture, &args);
        criterion.bench_function(name, |bencher| bencher.iter(|| self.run(fixture, &args)));
    }

    fn sync(&mut self, criterion: &mut Criterion, name: &str, fixture: &Fixture) {
        let environment = fixture.root().join(".venv");
        self.run(
            fixture,
            &[
                "venv",
                environment
                    .to_str()
                    .expect("Environment path must be UTF-8"),
            ],
        );
        // Keep the environment empty: measure graph traversal and planning, not installation or
        // creation of a virtual environment. No subprocess is included in the measured operation.
        let args = [
            "sync",
            "--frozen",
            "--dry-run",
            "--no-default-groups",
            "--extra",
            "cpu",
        ];
        self.run(fixture, &args);
        criterion.bench_function(name, |bencher| bencher.iter(|| self.run(fixture, &args)));
    }
}

fn conflicts(criterion: &mut Criterion) {
    let mut harness = Harness::new();

    let fixture = overlapping();
    harness.lock(criterion, "lock_conflicts_overlapping_15", &fixture);
    harness.check_selection(
        &fixture,
        &["--extra", "dependencies_lowest"],
        "choice-0",
        &["1.0.0"],
    );
    harness.check_selection(
        &fixture,
        &["--extra", "dependencies_lower"],
        "choice-0",
        &["2.0.0"],
    );

    let fixture = mutually_exclusive();
    harness.lock(criterion, "lock_conflicts_mutually_exclusive_24", &fixture);
    harness.check_selection(&fixture, &["--extra", "backend-1"], "backend", &["1.0.0"]);
    harness.check_selection(&fixture, &["--extra", "backend-24"], "backend", &["24.0.0"]);
    harness.export(
        criterion,
        "export_frozen_conflicts_mutually_exclusive_24",
        &fixture,
        &["--extra", "backend-1"],
    );

    let fixture = included_groups();
    harness.lock(criterion, "lock_conflicts_included_groups", &fixture);
    harness.check_selection(&fixture, &["--group", "ci"], "backend", &["1.0.0"]);
    harness.check_selection(&fixture, &["--group", "gpu-ci"], "backend", &["2.0.0"]);

    for (conflicts, suffix) in [
        (true, "conflicts_shared_extras_24"),
        (false, "no_conflicts_shared_extras_24"),
    ] {
        let fixture = shared_extras(conflicts);
        harness.lock(criterion, &format!("lock_{suffix}"), &fixture);
        for (selection, cpu, gpu) in [
            ("cpu", vec!["1.0.0"], vec![]),
            ("gpu", vec![], vec!["1.0.0"]),
        ] {
            harness.check_selection(&fixture, &["--extra", selection], "cpu-backend", &cpu);
            harness.check_selection(&fixture, &["--extra", selection], "gpu-backend", &gpu);
        }
        harness.export(
            criterion,
            &format!("export_frozen_{suffix}"),
            &fixture,
            &["--extra", "cpu"],
        );
        harness.sync(criterion, &format!("sync_frozen_{suffix}"), &fixture);
    }
}

criterion_group!(benches, conflicts);
criterion_main!(benches);
