//! Compares canonical lock deserialization with the general TOML parser.

extern crate uv_performance_memory_allocator;

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use uv_resolver::Lock;

const REPOSITORY_LOCK: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../uv.lock"));

fn generated_lock(package_count: usize) -> String {
    let mut output = String::with_capacity(package_count * 180);
    output.push_str("version = 1\nrevision = 3\nrequires-python = \">=3.12\"\n");

    for index in 0..package_count {
        writeln!(
            output,
            "\n[[package]]\nname = \"package-{index:05}\"\nversion = \"1.0.0\"\nsource = {{ registry = \"https://example.com/simple\" }}"
        )
        .expect("writing to a string cannot fail");
        if index > 0 {
            writeln!(
                output,
                "dependencies = [\n    {{ name = \"package-{:05}\" }},\n]",
                index - 1
            )
            .expect("writing to a string cannot fail");
        }
    }

    output
}

fn lock_parsing(criterion: &mut Criterion) {
    let generated = generated_lock(5_000);
    let cases = [
        ("repository", REPOSITORY_LOCK),
        ("generated-5000", generated.as_str()),
    ];

    for (name, input) in cases {
        let expected: Lock = toml::from_str(input).expect("valid benchmark lock");
        let actual = Lock::from_toml(input).expect("valid canonical benchmark lock");
        assert_eq!(actual, expected, "{name}: direct lock differs from TOML");
    }

    let mut group = criterion.benchmark_group("lock_parsing");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(8));

    for (name, input) in cases {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::new("toml", name), &input, |bench, input| {
            bench.iter(|| {
                toml::from_str::<Lock>(black_box(input))
                    .expect("benchmark lock deserializes as TOML")
            });
        });
        group.bench_with_input(BenchmarkId::new("direct", name), &input, |bench, input| {
            bench.iter(|| {
                Lock::from_toml(black_box(input)).expect("benchmark lock deserializes directly")
            });
        });
    }

    group.finish();
}

criterion_group!(benches, lock_parsing);
criterion_main!(benches);
