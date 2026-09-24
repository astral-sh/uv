// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};
use uv_pypi_types::{Digest, PypiSimpleDetail};

fn simple_api_fixture(file_count: usize) -> serde_json::Value {
    let files = (0..file_count)
        .map(|index| {
            let filename = format!(
                "numpy-2.{}.{}-cp313-cp313-macosx_14_0_arm64.whl",
                index / 64,
                index % 64
            );

            serde_json::json!({
                "core-metadata": false,
                "data-dist-info-metadata": false,
                "filename": filename,
                "hashes": {
                    "sha256": "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                },
                "provenance": null,
                "requires-python": match index % 4 {
                    0 => ">=3.8",
                    1 => ">=3.9,<4",
                    2 => ">=3.10",
                    _ => ">=3.11",
                },
                "size": 12_345_678,
                "upload-time": "2025-06-07T12:34:56.123456Z",
                "url": format!(
                    "https://files.pythonhosted.org/packages/61/93/9fec62902d0b4fc2521333eba047bff4adbba41f1723a6382367f84ee522/{filename}"
                ),
                "yanked": false,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "files": files,
        "meta": { "api-version": "1.4" },
        "name": "numpy",
    })
}

fn deserialize_simple_api(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("simple_api_detail");

    for file_count in [32, 4096] {
        let fixture = simple_api_fixture(file_count);
        let json = serde_json::to_vec(&fixture).expect("benchmark input should serialize");

        group.throughput(Throughput::Bytes(json.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("pypi_json", file_count),
            &json,
            |benchmark, json| {
                benchmark.iter(|| {
                    serde_json::from_slice::<PypiSimpleDetail>(black_box(json))
                        .expect("benchmark input should be valid")
                });
            },
        );
    }

    group.finish();
}

fn digest_size<const BYTES: usize>(group: &mut BenchmarkGroup<'_, WallTime>) {
    const PATTERN: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

    let bytes = std::array::from_fn(|index| PATTERN[index % PATTERN.len()]);
    let digest = Digest::<BYTES>::from_bytes(bytes);
    let uppercase = digest.as_str().to_ascii_uppercase();

    group.throughput(Throughput::Bytes((BYTES * 2) as u64));
    for (case, hex) in [
        ("lowercase", digest.as_str()),
        ("uppercase", uppercase.as_str()),
    ] {
        group.bench_with_input(
            BenchmarkId::new(format!("from_hex/{case}"), BYTES),
            hex,
            |benchmark, hex| {
                benchmark.iter(|| {
                    Digest::<BYTES>::from_hex(black_box(hex))
                        .expect("benchmark input should be valid")
                });
            },
        );
    }

    group.throughput(Throughput::Bytes(BYTES as u64));
    group.bench_with_input(
        BenchmarkId::new("from_bytes", BYTES),
        &bytes,
        |benchmark, bytes| {
            benchmark.iter(|| Digest::from_bytes(black_box(*bytes)));
        },
    );

    group.throughput(Throughput::Bytes((BYTES * 2) as u64));
    group.bench_with_input(
        BenchmarkId::new("decode", BYTES),
        &digest,
        |benchmark, digest| {
            benchmark.iter(|| black_box(digest).decode());
        },
    );
}

fn digest(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("digest");

    digest_size::<16>(&mut group);
    digest_size::<32>(&mut group);
    digest_size::<48>(&mut group);
    digest_size::<64>(&mut group);

    group.finish();
}

criterion_group!(uv_pypi_types, deserialize_simple_api, digest);
criterion_main!(uv_pypi_types);
