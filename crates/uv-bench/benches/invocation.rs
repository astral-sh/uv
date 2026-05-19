//! Benchmarks for warm, whole-invocation uv command paths.

use std::hint::black_box;

use clap::Parser;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main, measurement::WallTime};
use tempfile::TempDir;
use uv_cli::Cli;

fn pip_compile_warm_jupyter(c: &mut Criterion<WallTime>) {
    let requirements = std::path::absolute("../../test/requirements/jupyter.in").unwrap();
    let cache_dir = std::path::absolute("../../.cache").unwrap();
    let output_dir = TempDir::new().unwrap();
    let output_file = output_dir.path().join("requirements.txt");
    let requirements = requirements.to_string_lossy().into_owned();
    let cache_dir = cache_dir.to_string_lossy().into_owned();
    let output_file = output_file.to_string_lossy().into_owned();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime
        .block_on(uv::benchmark::initialize(cli(
            &requirements,
            &cache_dir,
            &output_file,
        )))
        .unwrap();

    c.bench_function("pip_compile_warm_jupyter", |b| {
        b.iter_batched(
            || {
                cli(
                    black_box(&requirements),
                    black_box(&cache_dir),
                    black_box(&output_file),
                )
            },
            |cli| runtime.block_on(uv::benchmark::invoke(cli)).unwrap(),
            BatchSize::SmallInput,
        );
    });
}

fn cli(requirements: &str, cache_dir: &str, output_file: &str) -> Cli {
    Cli::try_parse_from([
        "uv",
        "pip",
        "compile",
        requirements,
        "--universal",
        "--exclude-newer",
        "2024-08-08",
        "--cache-dir",
        cache_dir,
        "--output-file",
        output_file,
        "--offline",
        "--quiet",
    ])
    .unwrap()
}

criterion_group!(invocation, pip_compile_warm_jupyter);
criterion_main!(invocation);
