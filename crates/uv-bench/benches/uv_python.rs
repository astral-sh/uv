// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use clap::Parser;
use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv::GlobalInitialization;
use uv::commands::ExitStatus;
use uv_cli::Cli;
use uv_python::downloads::ManagedPythonDownloadList;

fn load_python_download_catalog(c: &mut Criterion<WallTime>) {
    c.bench_function("load_python_download_catalog", |b| {
        b.iter(|| {
            black_box(
                ManagedPythonDownloadList::new_only_embedded()
                    .expect("Failed to load embedded Python download catalog"),
            )
        });
    });
}

fn python_list_downloads(c: &mut Criterion<WallTime>) {
    let cache_dir = tempfile::tempdir().expect("Failed to create temporary cache directory");
    let cache_dir = cache_dir.path().to_string_lossy().to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // Initialize global state and the cache before measuring repeated invocations.
    run_python_list(&runtime, &cache_dir, GlobalInitialization::Initialize);

    c.bench_function("python_list_downloads", |b| {
        b.iter(|| {
            run_python_list(&runtime, black_box(&cache_dir), GlobalInitialization::Reuse);
        });
    });
}

fn run_python_list(
    runtime: &tokio::runtime::Runtime,
    cache_dir: &str,
    global_initialization: GlobalInitialization,
) {
    let cli = Cli::try_parse_from([
        "uv",
        "--offline",
        "--no-config",
        "--quiet",
        "--cache-dir",
        cache_dir,
        "python",
        "list",
        // Avoid interpreter discovery and use the same catalog entries on every host.
        "--only-downloads",
        "--all-versions",
        "--all-platforms",
        "--all-arches",
        "--managed-python",
    ])
    .expect("Failed to parse Python list benchmark arguments");

    let status = runtime
        .block_on(uv::run(cli, global_initialization))
        .expect("Failed to list Python downloads");
    assert!(
        match status {
            ExitStatus::Success => true,
            ExitStatus::Failure | ExitStatus::Error | ExitStatus::External(_) => false,
        },
        "Python list benchmark should succeed"
    );
}

criterion_group!(
    uv_python,
    load_python_download_catalog,
    python_list_downloads
);
criterion_main!(uv_python);
