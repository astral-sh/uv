//! Benchmarks for whole-invocation uv command paths.

use std::env;
use std::hint::black_box;
use std::path::{Path, PathBuf};

use clap::Parser;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main, measurement::WallTime};
use tempfile::TempDir;
use uv_cli::Cli;
use uv_fs::copy_dir_all;

fn whole_invocation(c: &mut Criterion<WallTime>) {
    let harness = Harness::new();

    init_bare_project(c, &harness);
    add_frozen(c, &harness);
    remove_frozen(c, &harness);
    venv_create(c, &harness);
    venv_clear(c, &harness);
    lock_create(c, &harness);
    lock_check(c, &harness);
    sync_install(c, &harness);
    sync_noop(c, &harness);
    sync_reinstall(c, &harness);
    run_project_python(c, &harness);
    build_wheel(c, &harness);
    export_frozen(c, &harness);
    tree_frozen(c, &harness);
    workspace_metadata_frozen(c, &harness);
    pip_compile_warm_jupyter(c, &harness);
}

fn init_bare_project(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create init benchmark directory");
    let project = temporary_dir.path().join("project");
    let project = path_string(&project);

    c.bench_function("init_bare_project", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&project));
                init_cli(&project)
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn add_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create add benchmark directory");
    let project = temporary_dir.path().join("project");
    let project = path_string(&project);

    c.bench_function("add_frozen", |b| {
        b.iter_batched(
            || {
                harness.reset_project(&project);
                project_cli(
                    harness.cache_dir(),
                    &project,
                    &["add", "sniffio==1.3.1", "--frozen"],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn remove_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create remove benchmark directory");
    let project = temporary_dir.path().join("project");
    let project = path_string(&project);

    c.bench_function("remove_frozen", |b| {
        b.iter_batched(
            || {
                harness.reset_project(&project);
                project_cli(
                    harness.cache_dir(),
                    &project,
                    &["remove", "iniconfig", "--frozen"],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn venv_create(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create venv benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);

    c.bench_function("venv_create", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&environment));
                venv_cli(harness.cache_dir(), &environment, false)
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn venv_clear(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create clear venv benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);
    harness.invoke(venv_cli(harness.cache_dir(), &environment, false));

    bench_cli(c, harness, "venv_clear", || {
        venv_cli(harness.cache_dir(), &environment, true)
    });
}

fn lock_check(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());

    bench_cli(c, harness, "lock_check", || {
        project_cli(
            harness.cache_dir(),
            &project,
            &[
                "lock",
                "--locked",
                "--exclude-newer",
                "2024-08-08T00:00:00Z",
            ],
        )
    });
}

fn lock_create(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create lock benchmark directory");
    let project = temporary_dir.path().join("project");
    let project = path_string(&project);

    c.bench_function("lock_create", |b| {
        b.iter_batched(
            || {
                harness.reset_project_without_lock(&project);
                project_cli(
                    harness.cache_dir(),
                    &project,
                    &["lock", "--exclude-newer", "2024-08-08T00:00:00Z"],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn sync_install(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());
    let environment = Path::new(&project).join(".venv");
    let environment = path_string(&environment);

    c.bench_function("sync_install", |b| {
        b.iter_batched(
            || {
                harness.invoke(project_venv_cli(
                    harness.cache_dir(),
                    &project,
                    &environment,
                ));
                sync_cli(harness.cache_dir(), &project, &[])
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn sync_noop(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());
    harness.invoke(sync_cli(harness.cache_dir(), &project, &[]));

    bench_cli(c, harness, "sync_noop", || {
        sync_cli(harness.cache_dir(), &project, &[])
    });
}

fn sync_reinstall(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());
    harness.invoke(sync_cli(harness.cache_dir(), &project, &[]));

    bench_cli(c, harness, "sync_reinstall", || {
        sync_cli(harness.cache_dir(), &project, &["--reinstall"])
    });
}

fn run_project_python(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());
    harness.invoke(sync_cli(harness.cache_dir(), &project, &[]));

    bench_cli(c, harness, "run_project_python", || {
        project_cli(
            harness.cache_dir(),
            &project,
            &["run", "--frozen", "python", "-c", "pass"],
        )
    });
}

fn build_wheel(c: &mut Criterion<WallTime>, harness: &Harness) {
    let package = harness.package();
    let package = path_string(package.path());
    let output_dir = TempDir::new().expect("create build benchmark directory");
    let output = output_dir.path().join("dist");
    let output = path_string(&output);

    c.bench_function("build_wheel", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&output));
                build_cli(harness.cache_dir(), &package, &output)
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn export_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());
    let output_dir = TempDir::new().expect("create export benchmark directory");
    let output_file = output_dir.path().join("requirements.txt");
    let output_file = path_string(&output_file);

    bench_cli(c, harness, "export_frozen", || {
        project_cli(
            harness.cache_dir(),
            &project,
            &["export", "--frozen", "--output-file", &output_file],
        )
    });
}

fn tree_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let project = harness.project();
    let project = path_string(project.path());

    bench_cli(c, harness, "tree_frozen", || {
        project_cli(
            harness.cache_dir(),
            &project,
            &["tree", "--frozen", "--universal", "--depth", "0"],
        )
    });
}

fn workspace_metadata_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let workspace = harness.workspace();
    let member = workspace.path().join("packages/bird-feeder");
    let member = path_string(&member);

    bench_cli(c, harness, "workspace_metadata_frozen", || {
        project_cli(
            harness.cache_dir(),
            &member,
            &["workspace", "metadata", "--frozen"],
        )
    });
}

fn pip_compile_warm_jupyter(c: &mut Criterion<WallTime>, harness: &Harness) {
    let output_dir = TempDir::new().expect("create pip compile benchmark directory");
    let output_file = output_dir.path().join("requirements.txt");
    let output_file = path_string(&output_file);

    bench_cli(c, harness, "pip_compile_warm_jupyter", || {
        pip_compile_cli(
            harness.requirements(),
            harness.cache_dir(),
            black_box(&output_file),
        )
    });
}

fn bench_cli(c: &mut Criterion<WallTime>, harness: &Harness, name: &str, cli: impl Fn() -> Cli) {
    c.bench_function(name, |b| {
        b.iter_batched(&cli, |cli| harness.invoke(cli), BatchSize::SmallInput);
    });
}

struct Harness {
    runtime: tokio::runtime::Runtime,
    cache_dir: String,
    package_fixture: PathBuf,
    project_fixture: PathBuf,
    requirements: String,
    workspace_fixture: PathBuf,
    _initialization_dir: TempDir,
}

impl Harness {
    fn new() -> Self {
        let manifest_dir = env::current_dir().expect("resolve benchmark manifest directory");
        let workspace_root = manifest_dir.join("../..");
        let cache_dir = path_string(&workspace_root.join(".cache"));
        let requirements = path_string(&workspace_root.join("test/requirements/jupyter.in"));
        let initialization_dir = TempDir::new().expect("create initialization directory");
        let initialization_environment = initialization_dir.path().join(".venv");
        let initialization_environment = path_string(&initialization_environment);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create benchmark runtime");

        runtime
            .block_on(uv::benchmark::initialize(venv_cli(
                &cache_dir,
                &initialization_environment,
                false,
            )))
            .expect("initialize uv benchmark process state");

        Self {
            runtime,
            cache_dir,
            package_fixture: manifest_dir.join("fixtures/package"),
            project_fixture: manifest_dir.join("fixtures/project"),
            requirements,
            workspace_fixture: manifest_dir.join("fixtures/workspace"),
            _initialization_dir: initialization_dir,
        }
    }

    fn cache_dir(&self) -> &str {
        &self.cache_dir
    }

    fn requirements(&self) -> &str {
        &self.requirements
    }

    fn invoke(&self, cli: Cli) {
        self.runtime
            .block_on(uv::benchmark::invoke(black_box(cli)))
            .expect("benchmark invocation succeeds");
    }

    fn project(&self) -> Fixture {
        Fixture::copy(&self.project_fixture, "project")
    }

    fn package(&self) -> Fixture {
        Fixture::copy(&self.package_fixture, "package")
    }

    fn workspace(&self) -> Fixture {
        Fixture::copy(&self.workspace_fixture, "workspace")
    }

    fn reset_project(&self, destination: &str) {
        reset_path(Path::new(destination));
        copy_dir_all(&self.project_fixture, destination).expect("copy project benchmark fixture");
    }

    fn reset_project_without_lock(&self, destination: &str) {
        self.reset_project(destination);
        fs_err::remove_file(Path::new(destination).join("uv.lock"))
            .expect("remove project benchmark lockfile");
    }
}

struct Fixture {
    root: PathBuf,
    _temporary_dir: TempDir,
}

impl Fixture {
    fn copy(source: &Path, name: &str) -> Self {
        let temporary_dir = TempDir::new().expect("create fixture directory");
        let root = temporary_dir.path().join(name);
        copy_dir_all(source, &root).expect("copy benchmark fixture");
        Self {
            root,
            _temporary_dir: temporary_dir,
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

fn reset_path(path: &Path) {
    if path.try_exists().expect("check benchmark path") {
        fs_err::remove_dir_all(path).expect("remove benchmark path");
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cli(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("uv").chain(args.iter().copied()))
        .expect("parse benchmark invocation")
}

fn init_cli(project: &str) -> Cli {
    cli(&[
        "--no-config",
        "--offline",
        "--quiet",
        "init",
        "--bare",
        "--no-pin-python",
        "--no-workspace",
        "--vcs",
        "none",
        project,
    ])
}

fn project_cli(cache_dir: &str, project: &str, command: &[&str]) -> Cli {
    let mut args = vec![
        "--project",
        project,
        "--no-config",
        "--cache-dir",
        cache_dir,
        "--offline",
        "--quiet",
    ];
    args.extend_from_slice(command);
    cli(&args)
}

fn sync_cli(cache_dir: &str, project: &str, args: &[&str]) -> Cli {
    let mut command = vec!["sync", "--frozen"];
    command.extend_from_slice(args);
    project_cli(cache_dir, project, &command)
}

fn venv_cli(cache_dir: &str, environment: &str, clear: bool) -> Cli {
    if clear {
        cli(&[
            "--cache-dir",
            cache_dir,
            "--no-config",
            "--offline",
            "--quiet",
            "venv",
            "--no-project",
            "--clear",
            environment,
        ])
    } else {
        cli(&[
            "--cache-dir",
            cache_dir,
            "--no-config",
            "--offline",
            "--quiet",
            "venv",
            "--no-project",
            environment,
        ])
    }
}

fn project_venv_cli(cache_dir: &str, project: &str, environment: &str) -> Cli {
    cli(&[
        "--project",
        project,
        "--no-config",
        "--cache-dir",
        cache_dir,
        "--offline",
        "--quiet",
        "venv",
        "--clear",
        environment,
    ])
}

fn pip_compile_cli(requirements: &str, cache_dir: &str, output_file: &str) -> Cli {
    cli(&[
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
        "pip",
        "compile",
        requirements,
        "--universal",
        "--exclude-newer",
        "2024-08-08",
        "--output-file",
        output_file,
    ])
}

fn build_cli(cache_dir: &str, package: &str, output: &str) -> Cli {
    cli(&[
        "--no-config",
        "--cache-dir",
        cache_dir,
        "--offline",
        "--quiet",
        "build",
        "--wheel",
        "--no-build-logs",
        "--out-dir",
        output,
        package,
    ])
}

criterion_group!(invocation, whole_invocation);
criterion_main!(invocation);
