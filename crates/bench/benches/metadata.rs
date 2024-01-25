use std::fs;
use std::str::FromStr;
use zip::ZipArchive;

use bench::criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion,
};
use distribution_filename::WheelFilename;
use install_wheel_rs::{read_dist_info, read_record};
use pypi_types::Metadata21;

const FILENAMES: &[&str] = &[
    "numpy-1.26.3-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    "flask-3.0.1-py3-none-any.whl",
];

fn file_reader(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("file_reader");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let reader =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let mut archive = ZipArchive::new(reader).unwrap();
                let dist_info =
                    read_dist_info(&WheelFilename::from_str(filename).unwrap(), &mut archive)
                        .unwrap();
                std::hint::black_box(Metadata21::parse(&dist_info).unwrap());
            });
        });
    }

    group.finish();
}

fn buffered_reader(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("buffered_reader");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let reader = std::io::BufReader::new(file);
                let mut archive = ZipArchive::new(reader).unwrap();
                let dist_info =
                    read_dist_info(&WheelFilename::from_str(filename).unwrap(), &mut archive)
                        .unwrap();
                std::hint::black_box(Metadata21::parse(&dist_info).unwrap());
            });
        });
    }

    group.finish();
}

criterion_group!(metadata, file_reader, buffered_reader,);
criterion_main!(metadata);
