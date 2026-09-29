//! Concurrent HTTP cache revalidation against a local server returning `304 Not Modified`.
//! Set `TMPDIR` to select the filesystem containing the cache entries.

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::env;
use std::hint::black_box;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{
    BatchSize, Criterion, Throughput, criterion_group, criterion_main, measurement::WallTime,
};
use futures::{StreamExt, stream};
use reqwest::Request;
use uv_cache::CacheEntry;
use uv_client::{BaseClientBuilder, CacheControl, CachedClient};
use wiremock::matchers::{header, method};
use wiremock::{Mock, MockServer, Request as MockRequest, ResponseTemplate};

const CONCURRENCY: usize = 50;
const ENTRY_COUNT: usize = 500;
const PAYLOAD_SIZE: usize = 256 * 1024;
const ETAG: &str = "\"metadata\"";

fn prepare_entries(encoded: &[u8]) -> (tempfile::TempDir, Vec<CacheEntry>) {
    let directory = tempfile::tempdir().expect("Failed to create cache directory");
    let mut entries = Vec::with_capacity(ENTRY_COUNT);
    for index in 0..ENTRY_COUNT {
        let entry = CacheEntry::new(directory.path(), format!("entry-{index}"));
        let mut file = fs_err::File::create(entry.path()).expect("Failed to create cache entry");
        file.write_all(encoded)
            .expect("Failed to populate cache entry");
        // Each entry is replaced once, with its old data already flushed before measurement.
        file.sync_all().expect("Failed to flush cache entry");
        entries.push(entry);
    }
    (directory, entries)
}

async fn revalidate_entries(client: &CachedClient, request: &Request, entries: &[CacheEntry]) {
    stream::iter(entries)
        .map(|entry| async move {
            let payload = client
                .get_serde_with_retry(
                    request.try_clone().expect("Failed to clone GET request"),
                    entry,
                    CacheControl::MustRevalidate,
                    async |_, _| {
                        Err::<String, _>(io::Error::other(
                            "Revalidation fetched a new response body",
                        ))
                    },
                )
                .await
                .expect("Failed to revalidate cache entry");
            assert_eq!(payload.len(), PAYLOAD_SIZE);
            black_box(payload);
        })
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
}

fn cached_client_revalidate(c: &mut Criterion<WallTime>) {
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
    let revalidations = Arc::new(AtomicUsize::new(0));
    let (_server, client, request, encoded) = runtime.block_on(async {
        let server = MockServer::builder()
            .disable_request_recording()
            .start()
            .await;
        let responses = Arc::clone(&revalidations);
        let not_modified = ResponseTemplate::new(304)
            .insert_header("cache-control", "public, max-age=3600")
            .insert_header("etag", ETAG);
        Mock::given(method("GET"))
            .and(header("if-none-match", ETAG))
            .respond_with(move |_: &MockRequest| {
                responses.fetch_add(1, Ordering::Relaxed);
                not_modified.clone()
            })
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", "public, max-age=3600")
                    .insert_header("etag", ETAG)
                    .set_body_string("x".repeat(PAYLOAD_SIZE)),
            )
            .with_priority(2)
            .mount(&server)
            .await;

        let client = CachedClient::new(
            BaseClientBuilder::default()
                .retries(0)
                .build()
                .expect("Failed to create cached client"),
        );
        let url = server.uri().parse().expect("Invalid mock server URL");
        let request = client
            .uncached()
            .for_host(&url)
            .get(url.as_str())
            .build()
            .expect("Failed to build GET request");
        let directory = tempfile::tempdir().expect("Failed to create seed cache directory");
        let entry = CacheEntry::new(directory.path(), "seed");
        let payload = client
            .get_serde_with_retry(
                request.try_clone().expect("Failed to clone GET request"),
                &entry,
                CacheControl::None,
                async |response, _| response.text().await,
            )
            .await
            .expect("Failed to seed cache entry");
        assert_eq!(payload.len(), PAYLOAD_SIZE);
        // All entries use the same URL, so they can share the serialized payload and cache policy.
        let encoded = fs_err::read(entry.path()).expect("Failed to read seed cache entry");
        (server, client, request, encoded)
    });

    let mut group = c.benchmark_group("cached_client");
    group.sample_size(10);
    group.throughput(Throughput::Elements(ENTRY_COUNT as u64));
    group.bench_function("revalidate_shared", |b| {
        let initial_revalidations = revalidations.load(Ordering::Relaxed);
        let mut expected_revalidations = 0;
        b.iter_batched_ref(
            || {
                expected_revalidations += ENTRY_COUNT;
                prepare_entries(&encoded)
            },
            |(_directory, entries)| {
                runtime.block_on(revalidate_entries(&client, &request, entries));
            },
            BatchSize::PerIteration,
        );
        // Verify outside the timer that every request reached the server and returned a 304.
        assert_eq!(
            revalidations.load(Ordering::Relaxed) - initial_revalidations,
            expected_revalidations,
        );
    });
    group.finish();
}

criterion_group!(benches, cached_client_revalidate);
criterion_main!(benches);
