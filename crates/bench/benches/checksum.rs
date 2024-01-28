use std::hash::Hasher;

use futures::TryStreamExt;
use reqwest::Url;
use sha2::Digest;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use bench::criterion::{
    BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime,
};
use puffin_extract::{unzip_no_seek, unzip_no_seek_fast};

const FILENAMES: &[&str] = &[
    "numpy-1.26.3-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    // "flask-3.0.1-py3-none-any.whl",
];


fn unzip(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                let url = Url::parse("https://files.pythonhosted.org/packages/e7/43/02684ed09a6a317773d61055ed89d4056bd069fed2dec88ed3d1e5f4397f/numpy-1.26.3-pp39-pypy39_pp73-win_amd64.whl").unwrap();

                let client = reqwest::Client::new();
                let response = client.get(url).send().await.unwrap();
                let reader = response.bytes_stream().map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)).into_async_read();

                let temp_dir =tempfile::tempdir().unwrap();

                unzip_no_seek(reader.compat(), temp_dir.path()).await.unwrap();
            });
        });
    }

    group.finish();
}

fn unzip_fast(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip_fast");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                let url = Url::parse("https://files.pythonhosted.org/packages/e7/43/02684ed09a6a317773d61055ed89d4056bd069fed2dec88ed3d1e5f4397f/numpy-1.26.3-pp39-pypy39_pp73-win_amd64.whl").unwrap();

                let client = reqwest::Client::new();
                let response = client.get(url).send().await.unwrap();
                let reader = response.bytes_stream().map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)).into_async_read();

                let temp_dir =tempfile::tempdir().unwrap();

                unzip_no_seek_fast(reader.compat(), temp_dir.path()).await.unwrap();
            });
        });
    }

    group.finish();
}


criterion_group!(
    checksum,
    unzip,
    unzip_fast,
);
criterion_main!(checksum);
