// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_settings::Options;

fn parse_build_dependency_settings(criterion: &mut Criterion<WallTime>) {
    let settings = include_str!("fixtures/build-dependencies/uv.toml");
    criterion.bench_function("parse_build_dependency_settings", |benchmark| {
        benchmark.iter(|| {
            toml::from_str::<Options>(black_box(settings)).expect("benchmark settings should parse")
        });
    });
}

criterion_group!(uv_settings, parse_build_dependency_settings);
criterion_main!(uv_settings);
