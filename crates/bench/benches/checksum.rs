use anyhow::anyhow;
use cityhash::cityhash_1_1_1;
use reqwest::{Method, Request, Url};
use std::fs;
use std::hash::Hasher;
use std::str::FromStr;

use bench::criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion,
};
use distribution_filename::WheelFilename;
use futures::{FutureExt, TryStreamExt};
use install_wheel_rs::read_record;
use puffin_extract::{unzip_no_seek, unzip_no_seek_blake3, unzip_no_seek_sha256};
use seahash::SeaHasher;
use sha2::{Digest, Sha256};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use zip::ZipArchive;

const FILENAMES: &[&str] = &[
    "numpy-1.26.3-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    // "flask-3.0.1-py3-none-any.whl",
];

fn crc32_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("crc32_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(crc32fast::hash(&file));
            });
        });
    }

    group.finish();
}

fn checksum_record(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("checksum_record");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let reader = std::io::BufReader::new(file);
                let mut archive = ZipArchive::new(reader).unwrap();
                let record =
                    read_record(&WheelFilename::from_str(filename).unwrap(), &mut archive).unwrap();
                std::hint::black_box(crc32fast::hash(&record));
            });
        });
    }

    group.finish();
}

fn seahash_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("seahash_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(seahash::hash(&file));
            });
        });
    }

    group.finish();
}

fn seahash_record(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("seahash_record");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let reader = std::io::BufReader::new(file);
                let mut archive = ZipArchive::new(reader).unwrap();
                let record =
                    read_record(&WheelFilename::from_str(filename).unwrap(), &mut archive).unwrap();
                std::hint::black_box(seahash::hash(&record));
            });
        });
    }

    group.finish();
}

fn sha256_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("sha256_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(Sha256::new().chain_update(&file).finalize());
            });
        });
    }

    group.finish();
}

fn sha256_record(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("sha256_record");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let reader = std::io::BufReader::new(file);
                let mut archive = ZipArchive::new(reader).unwrap();
                let record =
                    read_record(&WheelFilename::from_str(filename).unwrap(), &mut archive).unwrap();
                std::hint::black_box(Sha256::new().chain_update(&record).finalize());
            });
        });
    }

    group.finish();
}

fn metrohash_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("metrohash_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let mut hasher = metrohash::MetroHash::new();
                hasher.write(&file);
                std::hint::black_box(hasher.finish());
            });
        });
    }

    group.finish();
}

fn xxhash_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("xxhash_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(xxhash_rust::xxh3::xxh3_64(&file))
            });
        });
    }

    group.finish();
}

fn cityhash64_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("cityhash64_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(cityhash_1_1_1::city_hash_64(&file));
            });
        });
    }

    group.finish();
}

fn cityhash128_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("cityhash128_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(cityhash_1_1_1::city_hash_128(&file));
            });
        });
    }

    group.finish();
}

fn blake3_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("blake3_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file = fs::read(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                std::hint::black_box(blake3::hash(&file));
            });
        });
    }

    group.finish();
}

fn blake3_mmap_wheel(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("blake3_mmap_wheel");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let mut hasher = blake3::Hasher::new();
                hasher
                    .update_mmap_rayon(format!("/Users/crmarsh/workspace/puffin/{filename}"))
                    .unwrap();
                std::hint::black_box(hasher.finalize());
            });
        });
    }

    group.finish();
}

fn crc(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("crc");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.iter(|| {
                let file =
                    fs::File::open(format!("/Users/crmarsh/workspace/puffin/{filename}")).unwrap();
                let reader = std::io::BufReader::new(file);
                let mut archive = ZipArchive::new(reader).unwrap();

                let mut hasher = SeaHasher::new();
                for i in 0..archive.len() {
                    let file = archive.by_index(i).unwrap();
                    hasher.write_u32(file.crc32());
                }

                std::hint::black_box(hasher.finish());
            });
        });
    }

    group.finish();
}

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

fn unzip_blake3(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip_blake3");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                let url = Url::parse("https://files.pythonhosted.org/packages/e7/43/02684ed09a6a317773d61055ed89d4056bd069fed2dec88ed3d1e5f4397f/numpy-1.26.3-pp39-pypy39_pp73-win_amd64.whl").unwrap();

                let client = reqwest::Client::new();
                let response = client.get(url).send().await.unwrap();
                let reader = response.bytes_stream().map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)).into_async_read();

                let temp_dir =tempfile::tempdir().unwrap();

                unzip_no_seek_blake3(reader.compat(), temp_dir.path()).await.unwrap();
            });
        });
    }

    group.finish();
}

fn unzip_sha256(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("unzip_sha256");

    for filename in FILENAMES {
        group.bench_function(BenchmarkId::from_parameter(filename), |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
                let url = Url::parse("https://files.pythonhosted.org/packages/e7/43/02684ed09a6a317773d61055ed89d4056bd069fed2dec88ed3d1e5f4397f/numpy-1.26.3-pp39-pypy39_pp73-win_amd64.whl").unwrap();

                let client = reqwest::Client::new();
                let response = client.get(url).send().await.unwrap();
                let reader = response.bytes_stream().map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err)).into_async_read();

                let temp_dir =tempfile::tempdir().unwrap();

                unzip_no_seek_sha256(reader.compat(), temp_dir.path()).await.unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    checksum,
    unzip_blake3,
    unzip_sha256,
    unzip,
    // blake3_mmap_wheel,
    // blake3_wheel,
    // xxhash_wheel,
    // seahash_wheel,
    // metrohash_wheel,
    // cityhash64_wheel,
    // cityhash128_wheel,
    // sha256_wheel,
    // crc32_wheel,
    // sha256_record,
    // checksum_record,
    // seahash_record,
    // crc,
);
criterion_main!(checksum);
