use std::cell::RefCell;
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};
use std::{assert_matches, io};

use anyhow::{Result, anyhow};
use reqwest::Response;
use wiremock::matchers::{any, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use uv_cache::CacheEntry;
use uv_client::{
    BaseClientBuilder, CacheControl, CachedClient, CachedClientError, DataWithCachePolicy,
    ErrorKind, RetryState,
};

#[test]
fn reject_invalid_cache_lengths() {
    for bytes in [&[u8::MAX; 8][..], &[u8::MAX; 32][..]] {
        let error = DataWithCachePolicy::from_reader(bytes).unwrap_err();
        assert_matches!(error.kind(), ErrorKind::ArchiveRead(_));
    }
}

/// Fetch a text payload through the persistent HTTP cache.
async fn cached_text(
    client: &CachedClient,
    server: &MockServer,
    entry: &CacheEntry,
    control: CacheControl,
) -> Result<String> {
    let url = format!("{}/metadata", server.uri()).parse()?;
    let request = client.uncached().for_host(&url).get(url.as_str()).build()?;
    client
        .get_serde_with_retry(request, entry, control, async |response, _| {
            response.text().await
        })
        .await
        .map_err(|err| anyhow!("{err:?}"))
}

#[tokio::test]
async fn revalidation_updates_only_policy() -> Result<()> {
    let server = MockServer::start().await;
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    let temp_dir = tempfile::tempdir()?;
    let entry = CacheEntry::new(temp_dir.path(), "response");
    let body = "cached metadata".repeat(100_000);

    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=0")
                .insert_header("etag", "\"cached\"")
                .set_body_string(&body),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        body
    );
    server.verify().await;
    server.reset().await;

    let original = fs_err::read(&entry)?;
    let data_len = DataWithCachePolicy::from_reader(original.as_slice())?
        .data
        .len();
    // A hardlink observes in-place changes, but would retain the old file after a rename.
    let alias = temp_dir.path().join("alias");
    fs_err::hard_link(entry.path(), &alias)?;
    let modified = UNIX_EPOCH + Duration::from_hours(24);
    fs_err::OpenOptions::new()
        .write(true)
        .open(entry.path())?
        .set_modified(modified)?;

    Mock::given(method("GET"))
        .and(path("/metadata"))
        .and(header("if-none-match", "\"cached\""))
        .respond_with(
            ResponseTemplate::new(304)
                .insert_header("etag", "\"cached\"")
                .insert_header("cache-control", "public, max-age=3600"),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::MustRevalidate).await?,
        body
    );
    let refreshed = fs_err::read(&entry)?;
    assert_eq!(refreshed[..data_len + 16], original[..data_len + 16]);
    assert_ne!(refreshed, original);
    assert_eq!(fs_err::read(alias)?, refreshed);
    assert!(refreshed.len() - data_len < 1024);
    assert!(fs_err::metadata(&entry)?.modified()? > modified);
    server.verify().await;
    server.reset().await;

    // A new client must honor the persisted freshness without another HTTP request.
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        body
    );
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::AllowStale).await?,
        body
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("request recording is enabled")
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn revalidation_of_readonly_entry() -> Result<()> {
    let server = MockServer::start().await;
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    let temp_dir = tempfile::tempdir()?;
    let entry = CacheEntry::new(temp_dir.path(), "response");
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=0")
                .insert_header("etag", "\"cached\"")
                .set_body_string("cached"),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "cached"
    );
    server.verify().await;
    server.reset().await;

    let original = fs_err::read(&entry)?;
    let permissions = fs_err::metadata(&entry)?.permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs_err::set_permissions(&entry, readonly)?;
    // Privileged users can write despite the read-only permissions.
    if fs_err::OpenOptions::new()
        .write(true)
        .open(entry.path())
        .is_ok()
    {
        fs_err::set_permissions(&entry, permissions)?;
        return Ok(());
    }

    Mock::given(method("GET"))
        .and(header("if-none-match", "\"cached\""))
        .respond_with(
            ResponseTemplate::new(304)
                .insert_header("etag", "\"cached\"")
                .insert_header("cache-control", "public, max-age=3600"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let result = cached_text(&client, &server, &entry, CacheControl::MustRevalidate).await;
    fs_err::set_permissions(&entry, permissions)?;
    assert_eq!(result?, "cached");
    assert_eq!(fs_err::read(&entry)?, original);
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn overlapping_policy_updates_are_refetched() -> Result<()> {
    let server = MockServer::start().await;
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    let temp_dir = tempfile::tempdir()?;
    let entry = CacheEntry::new(temp_dir.path(), "response");
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=3600")
                .insert_header("etag", "\"cached\"")
                .insert_header("vary", format!("x-{}", "large".repeat(100)))
                .set_body_string("cached"),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "cached"
    );
    let large = fs_err::read(&entry)?;
    let data_len = DataWithCachePolicy::from_reader(large.as_slice())?
        .data
        .len();
    server.verify().await;
    server.reset().await;

    Mock::given(method("GET"))
        .and(header("if-none-match", "\"cached\""))
        .respond_with(
            ResponseTemplate::new(304)
                .insert_header("etag", "\"cached\"")
                .insert_header("cache-control", "public, max-age=7200"),
        )
        .expect(2)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::MustRevalidate).await?,
        "cached"
    );
    assert!(fs_err::metadata(&entry)?.len() < large.len() as u64);

    // Model a writer paused between writing a longer policy and setting the file length.
    let mut writer = fs_err::OpenOptions::new().write(true).open(entry.path())?;
    let policy_start = data_len + 16;
    writer.seek(SeekFrom::Start(policy_start as u64))?;
    writer.write_all(&large[policy_start..])?;

    // A second writer completes a shorter refresh before the first writer resumes.
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::MustRevalidate).await?,
        "cached"
    );
    assert!(fs_err::metadata(&entry)?.len() < large.len() as u64);
    writer.set_len(large.len() as u64)?;
    drop(writer);
    let raced = fs_err::read(&entry)?;
    assert_eq!(raced[..policy_start], large[..policy_start]);
    let error = DataWithCachePolicy::from_reader(raced.as_slice()).unwrap_err();
    assert_matches!(error.kind(), ErrorKind::ArchiveRead(_));
    server.verify().await;
    server.reset().await;

    // An invalid policy must trigger a full fetch without sending its cached validator.
    Mock::given(method("GET"))
        .respond_with(|request: &Request| {
            assert!(!request.headers.contains_key("if-none-match"));
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=3600")
                .set_body_string("replacement")
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "replacement"
    );
    server.verify().await;
    server.reset().await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "replacement"
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("request recording is enabled")
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn delayed_revalidation_cannot_modify_replaced_payload() -> Result<()> {
    let server = MockServer::start().await;
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    let temp_dir = tempfile::tempdir()?;
    let entry = CacheEntry::new(temp_dir.path(), "response");
    let newer_entry = CacheEntry::new(temp_dir.path(), "newer");
    let requests = AtomicUsize::new(0);
    Mock::given(method("GET"))
        .respond_with(move |_: &Request| {
            let body = if requests.fetch_add(1, Ordering::Relaxed) == 0 {
                "first"
            } else {
                "other"
            };
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=3600")
                .insert_header("etag", format!("\"{body}\""))
                .set_body_string(body)
        })
        .expect(2)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "first"
    );
    assert_eq!(
        cached_text(&client, &server, &newer_entry, CacheControl::None).await?,
        "other"
    );
    let newer = fs_err::read(&newer_entry)?;
    server.verify().await;
    server.reset().await;

    let path = entry.path().to_path_buf();
    let replacement = newer.clone();
    Mock::given(method("GET"))
        .and(header("if-none-match", "\"first\""))
        .respond_with(move |_: &Request| {
            // Publish a newer 200 while the original entry is awaiting revalidation.
            uv_fs::write_atomic_sync(&path, &replacement).expect("replace cached response");
            ResponseTemplate::new(304)
                .insert_header("etag", "\"first\"")
                .insert_header("cache-control", "public, max-age=7200")
        })
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::MustRevalidate).await?,
        "first"
    );
    assert_eq!(fs_err::read(&entry)?, newer);
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "other"
    );
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn torn_policy_is_refetched() -> Result<()> {
    let server = MockServer::start().await;
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    let temp_dir = tempfile::tempdir()?;
    let entry = CacheEntry::new(temp_dir.path(), "response");
    Mock::given(method("GET"))
        .respond_with(|request: &Request| {
            assert!(!request.headers.contains_key("if-none-match"));
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=3600")
                .set_body_string("cached")
        })
        .expect(3)
        .mount(&server)
        .await;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "cached"
    );
    let original = fs_err::read(&entry)?;
    let data_len = DataWithCachePolicy::from_reader(original.as_slice())?
        .data
        .len();
    let mut torn = original.clone();
    torn[data_len + 16] ^= 1;
    fs_err::write(&entry, &torn)?;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::None).await?,
        "cached"
    );

    // The allow-stale path also validates integrity before returning the payload.
    fs_err::write(&entry, &torn)?;
    assert_eq!(
        cached_text(&client, &server, &entry, CacheControl::AllowStale).await?,
        "cached"
    );
    server.verify().await;
    Ok(())
}

/// Exercise the shared budget through both cached and forced-refresh requests.
async fn assert_retry_budget(
    middleware_failures: usize,
    retry_in_callback: bool,
    expected_callback_retries: &[bool],
) -> Result<()> {
    for skip_cache in [false, true] {
        let server = MockServer::start().await;
        let requests = AtomicUsize::new(0);
        Mock::given(any())
            .respond_with(move |_: &Request| {
                if requests.fetch_add(1, Ordering::Relaxed) < middleware_failures {
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200).set_body_string("response")
                }
            })
            .expect((middleware_failures + expected_callback_retries.len()) as u64)
            .mount(&server)
            .await;

        let cache = tempfile::tempdir()?;
        let entry = CacheEntry::new(cache.path(), "response.msgpack");
        let client = CachedClient::new(
            BaseClientBuilder::default()
                .retries(2)
                .no_retry_delay(true)
                .build()?,
        );
        let url = server.uri().parse()?;
        let request = client.uncached().for_host(&url).get(server.uri()).build()?;
        let callback_retries = RefCell::new(Vec::new());
        let callback = async |response: Response, retry_state: &mut RetryState| {
            response.bytes().await.map_err(io::Error::other)?;
            let error = io::Error::new(io::ErrorKind::TimedOut, "interrupted response");
            callback_retries
                .borrow_mut()
                .push(retry_in_callback && retry_state.should_retry(&error, 0).is_some());
            Err::<String, _>(error)
        };
        let result = if skip_cache {
            client
                .skip_cache_with_retry(request, &entry, CacheControl::None, callback)
                .await
        } else {
            client
                .get_serde_with_retry(request, &entry, CacheControl::None, callback)
                .await
        };

        assert_matches!(result, Err(CachedClientError::Callback { retries: 2, .. }));
        assert_eq!(callback_retries.into_inner(), expected_callback_retries);
        server.verify().await;
    }
    Ok(())
}

#[tokio::test]
async fn callback_and_outer_retries_share_budget() -> Result<()> {
    // One retry in the callback leaves one full restart, whose callback has no budget left.
    assert_retry_budget(0, true, &[true, false]).await
}

#[tokio::test]
async fn middleware_retries_are_counted_before_callback() -> Result<()> {
    // The middleware exhausts the budget before delivering a response to the callback.
    assert_retry_budget(2, true, &[false]).await
}

#[tokio::test]
async fn middleware_retries_are_not_counted_twice() -> Result<()> {
    // One middleware retry leaves one full restart after the callback fails.
    assert_retry_budget(1, false, &[false, false]).await
}

#[tokio::test]
async fn send_counts_middleware_retries() -> Result<()> {
    for network_error in [false, true] {
        let server = MockServer::start().await;
        let mock = if network_error {
            Mock::given(any())
                .respond_with_err(|_: &Request| {
                    io::Error::new(io::ErrorKind::ConnectionReset, "connection reset")
                })
                .expect(3)
        } else {
            let requests = AtomicUsize::new(0);
            Mock::given(any())
                .respond_with(move |_: &Request| {
                    if requests.fetch_add(1, Ordering::Relaxed) == 0 {
                        ResponseTemplate::new(503)
                    } else {
                        ResponseTemplate::new(200)
                    }
                })
                .expect(2)
        };
        mock.mount(&server).await;

        let client = BaseClientBuilder::default()
            .retries(2)
            .no_retry_delay(true)
            .build()?;
        let url = server.uri().parse()?;
        let request = client.for_host(&url).get(server.uri());
        let mut retry_state = RetryState::start(client.retry_policy(), url);
        let result = retry_state.send(request).await;
        assert_eq!(result.is_err(), network_error);

        let error = io::Error::new(io::ErrorKind::TimedOut, "interrupted response");
        if !network_error {
            // The successful request used one retry, leaving one for a body failure.
            assert!(retry_state.should_retry(&error, 0).is_some());
        }
        assert!(retry_state.should_retry(&error, 0).is_none());
        server.verify().await;
    }
    Ok(())
}

#[tokio::test]
async fn revalidation_http_errors_share_retry_budget() -> Result<()> {
    let server = MockServer::start().await;
    let url = format!("{}/metadata", server.uri()).parse()?;
    let client = CachedClient::new(
        BaseClientBuilder::default()
            .retries(2)
            .no_retry_delay(true)
            .build()?,
    );
    let temp_dir = tempfile::tempdir()?;
    let cache_entry = CacheEntry::new(temp_dir.path(), "cached");

    Mock::given(method("GET"))
        .and(path("/metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "public, max-age=3600")
                .insert_header("etag", "\"cached\"")
                .set_body_string("cached"),
        )
        .mount(&server)
        .await;
    let request = client.uncached().for_host(&url).get(url.as_str()).build()?;
    let result = client
        .get_serde_with_retry(
            request,
            &cache_entry,
            CacheControl::None,
            async |response, _| response.text().await,
        )
        .await;
    assert_matches!(result, Ok(_));

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/metadata"))
        .and(header("if-none-match", "\"cached\""))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&server)
        .await;
    let request = client.uncached().for_host(&url).get(url.as_str()).build()?;
    let result = client
        .get_serde_with_retry(
            request,
            &cache_entry,
            CacheControl::MustRevalidate,
            async |response, _| response.text().await,
        )
        .await;

    assert_matches!(result, Err(CachedClientError::Client(_)));
    server.verify().await;
    Ok(())
}
