use std::hint::black_box;

use criterion::{
    BenchmarkId, Criterion, Throughput, criterion_group, criterion_main, measurement::WallTime,
};
use uv_pypi_types::PypiSimpleDetail;

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

criterion_group!(uv_pypi_types, deserialize_simple_api);
criterion_main!(uv_pypi_types);
