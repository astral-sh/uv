use std::path::Path;

use bench::criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

fn unzip(c: &mut Criterion<WallTime>) {
    let file = Path::new("/Users/crmarsh/Downloads/ruff-0.5.5-py3-none-win32.whl");

    let outfile = tempfile::TempDir::new().unwrap();

    let mut group = c.benchmark_group("uv");


    group.bench_function("true", |b| b.iter(|| {
        uv_extract::unzip(std::fs::File::open(file).unwrap(), outfile.path(), true).unwrap();
    }));

    group.bench_function("false", |b| b.iter(|| {
        uv_extract::unzip(std::fs::File::open(file).unwrap(), outfile.path(), false).unwrap();
    }));

    group.finish();
}


criterion_group!(uv, unzip);
criterion_main!(uv);
