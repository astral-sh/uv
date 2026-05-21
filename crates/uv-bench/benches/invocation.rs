//! Benchmarks for whole-invocation uv command paths.
//!
//! This suite focuses on repeatable offline work from the project, script, Python, tool,
//! environment, build, and pip interfaces. Service-facing and host-administration commands like
//! `auth`, `publish`, `self`, cache management, Python installation, and tool installation are
//! excluded until they have stable `CodSpeed` fixtures.

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

    // Project and workspace interface.
    init_bare_project(c, &harness);
    add_frozen(c, &harness);
    remove_frozen(c, &harness);
    lock_create(c, &harness);
    lock_check(c, &harness);
    sync_install(c, &harness);
    sync_noop(c, &harness);
    sync_reinstall(c, &harness);
    run_project_python(c, &harness);
    export_frozen(c, &harness);
    tree_frozen(c, &harness);
    workspace_metadata_frozen(c, &harness);

    // Script interface.
    add_script_frozen(c, &harness);
    remove_script_frozen(c, &harness);
    run_script_python(c, &harness);

    // Python and tool interfaces.
    python_list_downloads(c, &harness);
    python_find_environment(c, &harness);
    python_pin(c, &harness);
    tool_run_warm_ruff(c, &harness);

    // Environment and build operations.
    venv_create(c, &harness);
    venv_clear(c, &harness);
    build_wheel(c, &harness);
    build_sdist(c, &harness);

    // pip interface.
    pip_install_warm(c, &harness);
    pip_uninstall(c, &harness);
    pip_sync_install(c, &harness);
    pip_sync_noop(c, &harness);
    pip_sync_reinstall(c, &harness);
    pip_freeze(c, &harness);
    pip_list(c, &harness);
    pip_show_idna(c, &harness);
    pip_check(c, &harness);
    pip_tree(c, &harness);
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

fn add_script_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create add script benchmark directory");
    let script = temporary_dir.path().join("script.py");
    let script = path_string(&script);

    c.bench_function("add_script_frozen", |b| {
        b.iter_batched(
            || {
                harness.reset_script(&script);
                script_cli(
                    harness.cache_dir(),
                    &["add", "idna==3.7", "--frozen", "--script", &script],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn remove_script_frozen(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create remove script benchmark directory");
    let script = temporary_dir.path().join("script.py");
    let script = path_string(&script);

    c.bench_function("remove_script_frozen", |b| {
        b.iter_batched(
            || {
                harness.reset_script_with_dependency(&script);
                script_cli(
                    harness.cache_dir(),
                    &["remove", "idna", "--frozen", "--script", &script],
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

fn run_script_python(c: &mut Criterion<WallTime>, harness: &Harness) {
    let script = path_string(harness.script_fixture());

    bench_cli(c, harness, "run_script_python", || {
        run_script_cli(harness.cache_dir(), &script)
    });
}

fn python_list_downloads(c: &mut Criterion<WallTime>, harness: &Harness) {
    bench_cli(c, harness, "python_list_downloads", || {
        python_cli(harness.cache_dir(), &["list", "--only-downloads"])
    });
}

fn python_find_environment(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create Python find benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);
    harness.invoke(venv_cli(harness.cache_dir(), &environment, false));

    bench_cli(c, harness, "python_find_environment", || {
        python_cli(harness.cache_dir(), &["find", &environment])
    });
}

fn python_pin(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create Python pin benchmark directory");
    let directory = path_string(temporary_dir.path());
    let pin = temporary_dir.path().join(".python-version");

    c.bench_function("python_pin", |b| {
        b.iter_batched(
            || {
                remove_file(&pin);
                python_pin_cli(harness.cache_dir(), &directory)
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn tool_run_warm_ruff(c: &mut Criterion<WallTime>, harness: &Harness) {
    bench_cli(c, harness, "tool_run_warm_ruff", || {
        tool_run_cli(harness.cache_dir())
    });
}

fn build_wheel(c: &mut Criterion<WallTime>, harness: &Harness) {
    let package = path_string(harness.package_fixture());
    let output_dir = TempDir::new().expect("create build benchmark directory");
    let output = output_dir.path().join("dist");
    let output = path_string(&output);

    c.bench_function("build_wheel", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&output));
                build_cli(harness.cache_dir(), &package, &output, "--wheel")
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn build_sdist(c: &mut Criterion<WallTime>, harness: &Harness) {
    let package = path_string(harness.package_fixture());
    let output_dir = TempDir::new().expect("create sdist benchmark directory");
    let output = output_dir.path().join("dist");
    let output = path_string(&output);

    c.bench_function("build_sdist", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&output));
                build_cli(harness.cache_dir(), &package, &output, "--sdist")
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

fn pip_install_warm(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create pip install benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);

    c.bench_function("pip_install_warm", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&environment));
                harness.invoke(venv_cli(harness.cache_dir(), &environment, false));
                pip_install_cli(harness.cache_dir(), &environment, "idna==3.7")
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn pip_uninstall(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create pip uninstall benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);

    c.bench_function("pip_uninstall", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&environment));
                prepare_pip_environment(harness, &environment);
                pip_cli(
                    harness.cache_dir(),
                    &["uninstall", "idna", "--python", &environment],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn pip_sync_install(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create pip sync benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);

    c.bench_function("pip_sync_install", |b| {
        b.iter_batched(
            || {
                reset_path(Path::new(&environment));
                harness.invoke(venv_cli(harness.cache_dir(), &environment, false));
                pip_sync_cli(
                    harness.cache_dir(),
                    harness.pip_requirements(),
                    &environment,
                    &[],
                )
            },
            |cli| harness.invoke(cli),
            BatchSize::PerIteration,
        );
    });
}

fn pip_sync_noop(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create pip sync benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);
    harness.invoke(venv_cli(harness.cache_dir(), &environment, false));
    harness.invoke(pip_sync_cli(
        harness.cache_dir(),
        harness.pip_requirements(),
        &environment,
        &[],
    ));

    bench_cli(c, harness, "pip_sync_noop", || {
        pip_sync_cli(
            harness.cache_dir(),
            harness.pip_requirements(),
            &environment,
            &[],
        )
    });
}

fn pip_sync_reinstall(c: &mut Criterion<WallTime>, harness: &Harness) {
    let temporary_dir = TempDir::new().expect("create pip sync benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);
    harness.invoke(venv_cli(harness.cache_dir(), &environment, false));
    harness.invoke(pip_sync_cli(
        harness.cache_dir(),
        harness.pip_requirements(),
        &environment,
        &[],
    ));

    bench_cli(c, harness, "pip_sync_reinstall", || {
        pip_sync_cli(
            harness.cache_dir(),
            harness.pip_requirements(),
            &environment,
            &["--reinstall"],
        )
    });
}

fn pip_freeze(c: &mut Criterion<WallTime>, harness: &Harness) {
    let (_temporary_dir, environment) = pip_environment(harness);

    bench_cli(c, harness, "pip_freeze", || {
        pip_cli(harness.cache_dir(), &["freeze", "--python", &environment])
    });
}

fn pip_list(c: &mut Criterion<WallTime>, harness: &Harness) {
    let (_temporary_dir, environment) = pip_environment(harness);

    bench_cli(c, harness, "pip_list", || {
        pip_cli(harness.cache_dir(), &["list", "--python", &environment])
    });
}

fn pip_show_idna(c: &mut Criterion<WallTime>, harness: &Harness) {
    let (_temporary_dir, environment) = pip_environment(harness);

    bench_cli(c, harness, "pip_show_idna", || {
        pip_cli(
            harness.cache_dir(),
            &["show", "idna", "--python", &environment],
        )
    });
}

fn pip_check(c: &mut Criterion<WallTime>, harness: &Harness) {
    let (_temporary_dir, environment) = pip_environment(harness);

    bench_cli(c, harness, "pip_check", || {
        pip_cli(harness.cache_dir(), &["check", "--python", &environment])
    });
}

fn pip_tree(c: &mut Criterion<WallTime>, harness: &Harness) {
    let (_temporary_dir, environment) = pip_environment(harness);

    bench_cli(c, harness, "pip_tree", || {
        pip_cli(
            harness.cache_dir(),
            &["tree", "--depth", "0", "--python", &environment],
        )
    });
}

fn pip_compile_warm_jupyter(c: &mut Criterion<WallTime>, harness: &Harness) {
    let output_dir = TempDir::new().expect("create pip compile benchmark directory");
    let output_file = output_dir.path().join("requirements.txt");
    let output_file = path_string(&output_file);

    bench_cli(c, harness, "pip_compile_warm_jupyter", || {
        pip_compile_cli(
            harness.jupyter_requirements(),
            harness.cache_dir(),
            black_box(&output_file),
        )
    });
}

fn pip_environment(harness: &Harness) -> (TempDir, String) {
    let temporary_dir = TempDir::new().expect("create pip query benchmark directory");
    let environment = temporary_dir.path().join(".venv");
    let environment = path_string(&environment);
    prepare_pip_environment(harness, &environment);
    (temporary_dir, environment)
}

fn prepare_pip_environment(harness: &Harness, environment: &str) {
    harness.invoke(venv_cli(harness.cache_dir(), environment, false));
    harness.invoke(pip_sync_cli(
        harness.cache_dir(),
        harness.pip_requirements(),
        environment,
        &[],
    ));
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
    jupyter_requirements: String,
    pip_requirements: String,
    script_fixture: PathBuf,
    script_with_dependency_fixture: PathBuf,
    workspace_fixture: PathBuf,
    _initialization_dir: TempDir,
}

impl Harness {
    fn new() -> Self {
        let manifest_dir = env::current_dir().expect("resolve benchmark manifest directory");
        let workspace_root = manifest_dir.join("../..");
        let cache_dir = path_string(&workspace_root.join(".cache"));
        let jupyter_requirements =
            path_string(&workspace_root.join("test/requirements/jupyter.in"));
        let pip_requirements = path_string(&manifest_dir.join("fixtures/requirements.txt"));
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
            jupyter_requirements,
            pip_requirements,
            project_fixture: manifest_dir.join("fixtures/project"),
            script_fixture: manifest_dir.join("fixtures/script.py"),
            script_with_dependency_fixture: manifest_dir.join("fixtures/script_with_dependency.py"),
            workspace_fixture: manifest_dir.join("fixtures/workspace"),
            _initialization_dir: initialization_dir,
        }
    }

    fn cache_dir(&self) -> &str {
        &self.cache_dir
    }

    fn jupyter_requirements(&self) -> &str {
        &self.jupyter_requirements
    }

    fn pip_requirements(&self) -> &str {
        &self.pip_requirements
    }

    fn script_fixture(&self) -> &Path {
        &self.script_fixture
    }

    fn invoke(&self, cli: Cli) {
        self.runtime
            .block_on(uv::benchmark::invoke(black_box(cli)))
            .expect("benchmark invocation succeeds");
    }

    fn project(&self) -> Fixture {
        Fixture::copy(&self.project_fixture, "project")
    }

    fn package_fixture(&self) -> &Path {
        &self.package_fixture
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

    fn reset_script(&self, destination: &str) {
        reset_file(&self.script_fixture, Path::new(destination));
    }

    fn reset_script_with_dependency(&self, destination: &str) {
        reset_file(&self.script_with_dependency_fixture, Path::new(destination));
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

fn remove_file(path: &Path) {
    if path.try_exists().expect("check benchmark file") {
        fs_err::remove_file(path).expect("remove benchmark file");
    }
}

fn reset_file(source: &Path, destination: &Path) {
    remove_file(destination);
    fs_err::copy(source, destination).expect("copy benchmark fixture");
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

fn run_script_cli(cache_dir: &str, script: &str) -> Cli {
    cli(&[
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
        "run",
        "--no-project",
        script,
    ])
}

fn script_cli(cache_dir: &str, command: &[&str]) -> Cli {
    let mut args = vec![
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
    ];
    args.extend_from_slice(command);
    cli(&args)
}

fn python_cli(cache_dir: &str, command: &[&str]) -> Cli {
    let mut args = vec![
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
        "python",
    ];
    args.extend_from_slice(command);
    cli(&args)
}

fn python_pin_cli(cache_dir: &str, directory: &str) -> Cli {
    cli(&[
        "--cache-dir",
        cache_dir,
        "--project",
        directory,
        "--no-config",
        "--offline",
        "--quiet",
        "python",
        "pin",
        "3.12",
        "--no-project",
    ])
}

fn tool_run_cli(cache_dir: &str) -> Cli {
    cli(&[
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
        "tool",
        "run",
        "--isolated",
        "--from",
        "ruff==0.5.0",
        "ruff",
        "--version",
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
        "2024-08-08T00:00:00Z",
        "--output-file",
        output_file,
    ])
}

fn pip_install_cli(cache_dir: &str, environment: &str, requirement: &str) -> Cli {
    pip_cli(
        cache_dir,
        &[
            "install",
            requirement,
            "--python",
            environment,
            "--exclude-newer",
            "2024-08-08T00:00:00Z",
        ],
    )
}

fn pip_sync_cli(cache_dir: &str, requirements: &str, environment: &str, args: &[&str]) -> Cli {
    let mut command = vec![
        "sync",
        requirements,
        "--python",
        environment,
        "--exclude-newer",
        "2024-08-08T00:00:00Z",
    ];
    command.extend_from_slice(args);
    pip_cli(cache_dir, &command)
}

fn pip_cli(cache_dir: &str, command: &[&str]) -> Cli {
    let mut args = vec![
        "--cache-dir",
        cache_dir,
        "--no-config",
        "--offline",
        "--quiet",
        "pip",
    ];
    args.extend_from_slice(command);
    cli(&args)
}

fn build_cli(cache_dir: &str, package: &str, output: &str, distribution: &str) -> Cli {
    cli(&[
        "--no-config",
        "--cache-dir",
        cache_dir,
        "--offline",
        "--quiet",
        "build",
        distribution,
        "--no-build-logs",
        "--out-dir",
        output,
        package,
    ])
}

criterion_group!(invocation, whole_invocation);
criterion_main!(invocation);
