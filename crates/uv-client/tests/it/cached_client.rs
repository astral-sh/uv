use std::assert_matches;

use anyhow::Result;
use http::HeaderValue;
use reqwest::{Method, Request};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_cache::CacheEntry;
use uv_client::{
    BaseClientBuilder, CacheControl, CachedClient, CachedClientError, DataWithCachePolicy,
    ErrorKind,
};

#[test]
fn reject_overflowing_cache_policy_length() {
    let error = DataWithCachePolicy::from_reader(&[u8::MAX; 8][..]).unwrap_err();

    assert_matches!(error.kind(), ErrorKind::ArchiveRead(_));
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
            Request::new(Method::GET, url.parse()?),
            entry,
            cache_control,
            async |response| response.text().await,
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
