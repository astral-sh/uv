//! Benchmark `uv run python -V` in a warm cache against a fully synced airflow workspace.
//!
//! Lives in its own benchmark binary (separate process from `benches/uv.rs`) so that the
//! resolver benchmarks don't pollute the process state before [`uv::run`] is first invoked.

use std::hint::black_box;

use clap::Parser;
use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_cli::Cli;

#[expect(clippy::print_stderr)]
fn run_noop_airflow(c: &mut Criterion<WallTime>) {
    let airflow_dir = std::path::absolute("../../airflow").unwrap();
    if !airflow_dir.join("uv.lock").exists() {
        let commit = "7fa400745ac7aebc7cc4ec21d3a047e9fb258310";
        let repo = "https://github.com/apache/airflow.git";
        eprintln!(
            "Airflow checkout does not exist, not running benchmark.\n\
            To set up:\n\
            git init ../airflow\n\
            git -C ../airflow remote add origin {repo}\n\
            git -C ../airflow fetch --depth 1 origin {commit}\n\
            git -C ../airflow checkout FETCH_HEAD"
        );
        return;
    }
    let cache_dir = std::path::absolute("../../.cache").unwrap();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let airflow_dir = airflow_dir.to_string_lossy().to_string();
    let cache_dir = cache_dir.to_string_lossy().to_string();

    // First call initializes `uv_flags`, logging, miette, etc. Subsequent calls reuse them.
    let cli = Cli::try_parse_from([
        "uv",
        "run",
        "--directory",
        &airflow_dir,
        "--cache-dir",
        &cache_dir,
        "--quiet",
        "python",
        "-V",
    ])
    .unwrap();
    runtime.block_on(uv::run(cli, true)).unwrap();

    c.bench_function("run_noop_airflow", |b| {
        b.iter(|| {
            let cli = Cli::try_parse_from([
                "uv",
                "run",
                "--directory",
                black_box(&airflow_dir),
                "--cache-dir",
                black_box(&cache_dir),
                "--quiet",
                "--offline",
                "python",
                "-V",
            ])
            .unwrap();
            runtime.block_on(uv::run(cli, false)).unwrap();
        });
    });
}

criterion_group!(airflow_run, run_noop_airflow);
criterion_main!(airflow_run);
