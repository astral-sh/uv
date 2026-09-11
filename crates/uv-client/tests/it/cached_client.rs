use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{assert_matches, io};

use anyhow::Result;
use http::HeaderValue;
use reqwest::{Method, Request as ReqwestRequest, Response};
use wiremock::matchers::{any, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use uv_cache::CacheEntry;
use uv_client::{
    BaseClientBuilder, CacheControl, CachedClient, CachedClientError, DataWithCachePolicy,
    ErrorKind, RetryState,
};

#[test]
fn reject_overflowing_cache_policy_length() {
    let error = DataWithCachePolicy::from_reader(&[u8::MAX; 8][..]).unwrap_err();

    assert_matches!(error.kind(), ErrorKind::ArchiveRead(_));
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
async fn fresh_cache_skips_http_requests() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Cache-Control", "max-age=3600")
                .set_body_string("cached payload"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let directory = tempfile::tempdir()?;
    let entry = CacheEntry::new(directory.path(), "response.msgpack");
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    for _ in 0..2 {
        assert_eq!(
            cached_text(&client, &entry, &server.uri(), CacheControl::None).await?,
            "cached payload"
        );
    }
    Ok(())
}

#[tokio::test]
async fn fresh_cache_respects_overrides_and_revalidation() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Cache-Control", "max-age=0")
                .insert_header("ETag", "\"version-1\"")
                .set_body_string("cached payload"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(header("If-None-Match", "\"version-1\""))
        .and(header("Cache-Control", "no-cache"))
        .respond_with(
            ResponseTemplate::new(304)
                .insert_header("Cache-Control", "max-age=3600")
                .insert_header("ETag", "\"version-1\""),
        )
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    let directory = tempfile::tempdir()?;
    let entry = CacheEntry::new(directory.path(), "response.msgpack");
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    for cache_control in [
        CacheControl::Override(HeaderValue::from_static("max-age=3600")),
        CacheControl::None,
        CacheControl::MustRevalidate,
        CacheControl::None,
    ] {
        assert_eq!(
            cached_text(&client, &entry, &server.uri(), cache_control).await?,
            "cached payload"
        );
    }
    Ok(())
}

#[tokio::test]
async fn fresh_cache_rejects_a_different_request() -> Result<()> {
    let server = MockServer::start().await;
    for (resource, body) in [("/first", "first payload"), ("/second", "second payload")] {
        Mock::given(method("GET"))
            .and(path(resource))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Cache-Control", "max-age=3600")
                    .set_body_string(body),
            )
            .expect(1)
            .mount(&server)
            .await;
    }

    let directory = tempfile::tempdir()?;
    let entry = CacheEntry::new(directory.path(), "response.msgpack");
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    for resource in ["first", "second", "second"] {
        assert_eq!(
            cached_text(
                &client,
                &entry,
                &format!("{}/{resource}", server.uri()),
                CacheControl::None,
            )
            .await?,
            format!("{resource} payload")
        );
    }
    Ok(())
}

#[tokio::test]
async fn fresh_cache_heals_corrupted_policy_and_payload() -> Result<()> {
    for corrupt_policy in [false, true] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Cache-Control", "max-age=3600")
                    .set_body_string("cached payload"),
            )
            .expect(2)
            .mount(&server)
            .await;

        let directory = tempfile::tempdir()?;
        let entry = CacheEntry::new(directory.path(), "response.msgpack");
        let client = CachedClient::new(BaseClientBuilder::default().build()?);
        assert_eq!(
            cached_text(&client, &entry, &server.uri(), CacheControl::None).await?,
            "cached payload"
        );
        if corrupt_policy {
            fs_err::write(entry.path(), [u8::MAX; 8])?;
        } else {
            corrupt_payload(&entry)?;
        }
        for _ in 0..2 {
            assert_eq!(
                cached_text(&client, &entry, &server.uri(), CacheControl::None).await?,
                "cached payload"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn stale_cache_heals_corrupted_payload_after_revalidation() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Cache-Control", "max-age=0")
                .insert_header("ETag", "\"version-1\"")
                .set_body_string("cached payload"),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(header("If-None-Match", "\"version-1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("ETag", "\"version-1\""))
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;

    let directory = tempfile::tempdir()?;
    let entry = CacheEntry::new(directory.path(), "response.msgpack");
    let client = CachedClient::new(BaseClientBuilder::default().build()?);
    assert_eq!(
        cached_text(&client, &entry, &server.uri(), CacheControl::None).await?,
        "cached payload"
    );
    corrupt_payload(&entry)?;
    assert_eq!(
        cached_text(&client, &entry, &server.uri(), CacheControl::MustRevalidate).await?,
        "cached payload"
    );
    Ok(())
}

/// Fetches a text response through the same serialized cache used for parsed metadata.
async fn cached_text(
    client: &CachedClient,
    entry: &CacheEntry,
    url: &str,
    cache_control: CacheControl,
) -> Result<String> {
    client
        .get_serde_with_retry(
            ReqwestRequest::new(Method::GET, url.parse()?),
            entry,
            cache_control,
            async |response, _| response.text().await,
        )
        .await
        .map_err(|err| match err {
            CachedClientError::Client(err) => err.into(),
            CachedClientError::Callback { err, .. } => err.into(),
        })
}

/// Replaces the `MessagePack` payload's first byte with a reserved tag, leaving its policy intact.
fn corrupt_payload(entry: &CacheEntry) -> Result<()> {
    let mut bytes = fs_err::read(entry.path())?;
    bytes[0] = 0xc1;
    fs_err::write(entry.path(), bytes)?;
    Ok(())
}
