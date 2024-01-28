use std::hash::Hasher;

use futures::TryStreamExt;
use reqwest::Url;
use sha2::Digest;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use bench::criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion,
};
use puffin_extract::{unzip_no_seek, unzip_no_seek_fast, unzip_no_seek_faster};

const URL: &[&str] = &[
    // Small wheel (~100 KB), few files.
    "https://files.pythonhosted.org/packages/bd/0e/63738e88e981ae57c23bad6c499898314a1110a4141f77d7bd929b552fb4/flask-3.0.1-py3-none-any.whl",
    // Large wheel (~8 MB), many files.
    "https://files.pythonhosted.org/packages/97/67/6804ff6fc4fa6df188924412601cc418ddc2d0a500963b0801a97b7ec08a/Django-5.0.1-py3-none-any.whl",
    // Very large wheel (~15 MB), fewer files.
    "https://files.pythonhosted.org/packages/e7/43/02684ed09a6a317773d61055ed89d4056bd069fed2dec88ed3d1e5f4397f/numpy-1.26.3-pp39-pypy39_pp73-win_amd64.whl",
];

fn unzip(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip");

    for url in URL {
        group.bench_function(BenchmarkId::from_parameter(url), |b| {
            let url = Url::parse(url).unwrap();
            let client = reqwest::Client::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let response = client.get(url.clone()).send().await.unwrap();
                    let reader = response
                        .bytes_stream()
                        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                        .into_async_read();

                    let temp_dir = tempfile::tempdir().unwrap();

                    unzip_no_seek(reader.compat(), temp_dir.path())
                        .await
                        .unwrap();
                });
        });
    }

    group.finish();
}

fn unzip_fast(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip_fast");

    for url in URL {
        group.bench_function(BenchmarkId::from_parameter(url), |b| {
            let url = Url::parse(url).unwrap();
            let client = reqwest::Client::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let response = client.get(url.clone()).send().await.unwrap();
                    let reader = response
                        .bytes_stream()
                        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                        .into_async_read();

                    let temp_dir = tempfile::tempdir().unwrap();

                    unzip_no_seek_fast(reader.compat(), temp_dir.path())
                        .await
                        .unwrap();
                });
        });
    }

    group.finish();
}

fn unzip_faster(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip_faster");

    for url in URL {
        group.bench_function(BenchmarkId::from_parameter(url), |b| {
            let url = Url::parse(url).unwrap();
            let client = reqwest::Client::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async {
                    let response = client.get(url.clone()).send().await.unwrap();
                    let reader = response
                        .bytes_stream()
                        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                        .into_async_read();

                    let temp_dir = tempfile::tempdir().unwrap();

                    unzip_no_seek_faster(reader.compat(), temp_dir.path())
                        .await
                        .unwrap();
                });
        });
    }

    group.finish();
}

criterion_group!(checksum, unzip, unzip_fast, unzip_faster,);
criterion_main!(checksum);
