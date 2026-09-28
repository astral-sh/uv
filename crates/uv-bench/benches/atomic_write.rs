//! Concurrent atomic writes, including replacement of cache entries in a shared directory.

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};
use futures::future::join_all;

const CONCURRENCY: usize = 50;
const FILES_PER_WORKER: usize = 10;
const FILE_SIZE: usize = 256 * 1024;

#[derive(Clone, Copy)]
enum Workload {
    ReplaceShared,
    ReplaceSeparate,
    CreateShared,
}

impl Workload {
    fn name(self) -> &'static str {
        match self {
            Self::ReplaceShared => "replace_shared",
            Self::ReplaceSeparate => "replace_separate",
            Self::CreateShared => "create_shared",
        }
    }

    fn prepare(self, data: &[u8]) -> (tempfile::TempDir, Vec<Vec<PathBuf>>) {
        // Use the caller's temporary directory, so TMPDIR can select the filesystem under test.
        let directory = tempfile::tempdir().expect("Failed to create benchmark directory");
        let mut workers = Vec::with_capacity(CONCURRENCY);
        for worker in 0..CONCURRENCY {
            let parent = match self {
                Self::ReplaceShared | Self::CreateShared => directory.path().to_path_buf(),
                Self::ReplaceSeparate => {
                    let parent = directory.path().join(worker.to_string());
                    fs_err::create_dir(&parent).expect("Failed to create worker directory");
                    parent
                }
            };
            let mut paths = Vec::with_capacity(FILES_PER_WORKER);
            for file in 0..FILES_PER_WORKER {
                let path = parent.join(format!("entry-{worker}-{file}"));
                match self {
                    Self::ReplaceShared | Self::ReplaceSeparate => {
                        let mut entry =
                            fs_err::File::create(&path).expect("Failed to create cache entry");
                        entry
                            .write_all(data)
                            .expect("Failed to populate cache entry");
                        // Replace each entry once, with its old data already flushed. This keeps
                        // writeback of the overwritten inode out of the measured workload.
                        entry.sync_all().expect("Failed to flush cache entry");
                    }
                    Self::CreateShared => {}
                }
                paths.push(path);
            }
            workers.push(paths);
        }
        (directory, workers)
    }
}

#[derive(Clone, Copy)]
enum WriteMode {
    Async,
    Blocking,
}

impl WriteMode {
    fn name(self) -> &'static str {
        match self {
            Self::Async => "async",
            Self::Blocking => "blocking",
        }
    }

    async fn write(self, path: &Path, data: &Arc<[u8]>) {
        match self {
            Self::Async => uv_fs::write_atomic(path, data.as_ref())
                .await
                .expect("Failed to write cache entry"),
            Self::Blocking => {
                let path = path.to_path_buf();
                let data = Arc::clone(data);
                tokio::task::spawn_blocking(move || uv_fs::write_atomic_sync(path, data.as_ref()))
                    .await
                    .expect("Cache write task failed")
                    .expect("Failed to write cache entry");
            }
        }
    }
}

fn atomic_write(c: &mut Criterion<WallTime>) {
    // Filesystem contention requires wall-clock measurements. CodSpeed calls its simulation
    // runner `instrumentation` in current versions.
    if let Ok(mode) = env::var("CODSPEED_RUNNER_MODE")
        && (mode == "instrumentation" || mode == "simulation")
    {
        return;
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(CONCURRENCY)
        .build()
        .expect("Failed to create Tokio runtime");
    let data: Arc<[u8]> = Arc::from(vec![0x55; FILE_SIZE]);
    let mut group = c.benchmark_group("atomic_write");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(
        (CONCURRENCY * FILES_PER_WORKER * FILE_SIZE) as u64,
    ));

    for workload in [
        Workload::ReplaceShared,
        Workload::ReplaceSeparate,
        Workload::CreateShared,
    ] {
        for mode in [WriteMode::Async, WriteMode::Blocking] {
            group.bench_function(BenchmarkId::new(workload.name(), mode.name()), |b| {
                // Setup, flushing old entries, and directory removal are outside the timer.
                // Keep only one fixture alive at a time to bound disk usage.
                b.iter_batched_ref(
                    || workload.prepare(&data),
                    |(_directory, workers)| {
                        runtime.block_on(join_all(workers.iter().map(|paths| {
                            let data = &data;
                            async move {
                                for path in paths {
                                    mode.write(path, data).await;
                                }
                            }
                        })))
                    },
                    BatchSize::PerIteration,
                );
            });
        }
    }
    group.finish();
}

criterion_group!(benches, atomic_write);
criterion_main!(benches);
