use std::fs;

use bench::criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion,
};
use puffin_extract::{unzip_archive, unzip_archive_fast, unzip_archive_faster};

const FILENAMES: &[&str] = &[
    "Django-5.0.1-py3-none-any.whl",
    // "numpy-1.26.3-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    // "flask-3.0.1-py3-none-any.whl",
];

fn baseline(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("baseline");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/guffin/{filename}")).unwrap();
                let target = tempfile::tempdir().unwrap();
                std::hint::black_box(unzip_archive(file, target.path()).unwrap());
            });
        });
    }

    group.finish();
}

fn fast(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("fast");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/guffin/{filename}")).unwrap();
                let target = tempfile::tempdir().unwrap();
                std::hint::black_box(unzip_archive_fast(file, target.path()).unwrap());
            });
        });
    }

    group.finish();
}

fn faster(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("faster");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/guffin/{filename}")).unwrap();
                let target = tempfile::tempdir().unwrap();
                std::hint::black_box(unzip_archive_faster(file, target.path()).unwrap());
            });
        });
    }

    group.finish();
}

criterion_group!(metadata, fast, faster);
criterion_main!(metadata);
