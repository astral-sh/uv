// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use clap::{CommandFactory, Parser};
use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_cli::Cli;
use uv_settings::Options;

fn cli_command(criterion: &mut Criterion<WallTime>) {
    criterion.bench_function("cli_command", |benchmark| {
        benchmark.iter(|| black_box(Cli::command()));
    });
}

fn cli_parse(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("cli_parse");
    for (name, arguments) in [
        (
            "pip_compile",
            &[
                "uv",
                "pip",
                "compile",
                "requirements.in",
                "--constraint",
                "constraints.txt",
                "--override",
                "overrides.txt",
                "--exclude",
                "excludes.txt",
                "--build-constraint",
                "build-constraints.txt",
            ][..],
        ),
        (
            "pip_install",
            &[
                "uv",
                "pip",
                "install",
                "requests",
                "--constraint",
                "constraints.txt",
                "--override",
                "overrides.txt",
                "--exclude",
                "excludes.txt",
                "--build-constraint",
                "build-constraints.txt",
            ][..],
        ),
        (
            "tool_install",
            &[
                "uv",
                "tool",
                "install",
                "ruff",
                "--constraint",
                "constraints.txt",
                "--override",
                "overrides.txt",
                "--exclude",
                "excludes.txt",
                "--build-constraint",
                "build-constraints.txt",
            ][..],
        ),
    ] {
        group.bench_function(name, |benchmark| {
            benchmark.iter(|| {
                black_box(
                    Cli::try_parse_from(black_box(arguments))
                        .expect("Benchmark command line should parse"),
                )
            });
        });
    }
    group.finish();
}

fn deserialize_options(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("deserialize_options");
    for (name, configuration) in [
        (
            "trusted_hosts",
            r#"
            allow-insecure-host = [
                "https://example.com:8443",
                { host = "mirror.example.com", scheme = "https", port = 443 },
            ]
        "#,
        ),
        ("preview_bool", "preview-features = true"),
        (
            "preview_list",
            "preview-features = ['format-command', 'pylock']",
        ),
    ] {
        group.bench_function(name, |benchmark| {
            benchmark.iter(|| {
                black_box(
                    toml::from_str::<Options>(black_box(configuration))
                        .expect("Benchmark configuration should deserialize"),
                )
            });
        });
    }
    group.finish();
}

criterion_group!(uv_startup, cli_command, cli_parse, deserialize_options);
criterion_main!(uv_startup);
