use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use reqwest::header::{
    ACCEPT_RANGES, AUTHORIZATION, CONTENT_LENGTH, CONTENT_RANGE, HeaderName, LOCATION, RANGE,
};
use wiremock::matchers::{basic_auth, header_exists, header_regex, method, path};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, MetadataRangeRequest, RegistryClientBuilder};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuiltDist, DirectUrlBuiltDist, IndexCapabilities};
use uv_git::GitResolver;
use uv_pep508::VerbatimUrl;
use uv_redacted::DisplaySafeUrl;

#[tokio::test]
async fn remote_metadata_with_and_without_cache() -> Result<()> {
    let server = MockServer::start().await;
    let wheel = fs_err::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/links/ok-1.0.0-py3-none-any.whl"),
    )?;
    Mock::given(method("GET"))
        .and(path("/ok-1.0.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .mount(&server)
        .await;

    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build()?;

    // The first run is without cache (the tempdir is empty), the second has the cache from the
    // first run.
    for _ in 0..2 {
        let url = format!("{}/ok-1.0.0-py3-none-any.whl", server.uri());
        let filename = WheelFilename::from_str("ok-1.0.0-py3-none-any.whl")?;
        let dist = BuiltDist::DirectUrl(DirectUrlBuiltDist {
            filename,
            location: Box::new(DisplaySafeUrl::parse(&url)?),
            url: VerbatimUrl::from_str(&url)?,
            size: None,
        });
        let resolver = GitResolver::default();
        let capabilities = IndexCapabilities::default();
        let metadata = client
            .wheel_metadata(&dist, &resolver, &capabilities, None)
            .await?;
        assert_eq!(metadata.version.to_string(), "1.0.0");
    }

    Ok(())
}

#[tokio::test]
async fn remote_metadata_requires_range_requests() -> Result<()> {
    let server = MockServer::start().await;
    let wheel = fs_err::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/links/ok-1.0.0-py3-none-any.whl"),
    )?;
    Mock::given(method("GET"))
        .and(path("/ok-1.0.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .mount(&server)
        .await;

    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default().metadata_range_request(MetadataRangeRequest::Require),
        cache,
    )
    .build()?;

    let url = format!("{}/ok-1.0.0-py3-none-any.whl", server.uri());
    let filename = WheelFilename::from_str("ok-1.0.0-py3-none-any.whl")?;
    let dist = BuiltDist::DirectUrl(DirectUrlBuiltDist {
        filename,
        location: Box::new(DisplaySafeUrl::parse(&url)?),
        url: VerbatimUrl::from_str(&url)?,
        size: None,
    });
    let resolver = GitResolver::default();
    let capabilities = IndexCapabilities::default();
    let error = client
        .wheel_metadata(&dist, &resolver, &capabilities, None)
        .await
        .expect_err("range requests should be required");

    insta::assert_snapshot!(
        error.to_string().replace(&server.uri(), "[HOST]"),
        @"Wheel metadata range requests are required, but not supported for: `[HOST]/ok-1.0.0-py3-none-any.whl`"
    );

    Ok(())
}

/// Covers same-origin redirect semantics and credential propagation.
#[tokio::test]
async fn remote_metadata_redirect_same_origin() -> Result<()> {
    let server = MockServer::start().await;
    let wheel = wheel()?;
    let wheel_len = wheel.len();

    // The initial metadata probe should authenticate to the source and receive a redirect.
    Mock::given(method("HEAD"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header(LOCATION, format!("{}/head-wheel", server.uri())),
        )
        .expect(1)
        .named("HEAD request to the redirecting wheel URL")
        .mount(&server)
        .await;
    // The range reader should retry the source with an authenticated range request.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header(LOCATION, format!("{}/head-wheel", server.uri())),
        )
        .expect(1)
        .named("ranged GET request to the redirecting wheel URL")
        .mount(&server)
        .await;
    // A streaming retry should be sent to the source when the range request cannot be used.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_missing(RANGE))
        .respond_with(
            ResponseTemplate::new(303)
                .insert_header(LOCATION, format!("{}/head-wheel", server.uri())),
        )
        .expect(1)
        .named("streaming fallback GET request to the redirecting wheel URL")
        .mount(&server)
        .await;
    // The redirected `HEAD` request should retain credentials on the same origin.
    Mock::given(method("HEAD"))
        .and(path("/head-wheel"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(ACCEPT_RANGES, "bytes")
                .insert_header(CONTENT_LENGTH, wheel_len.to_string()),
        )
        .expect(1)
        .named("HEAD request to the same-origin redirect target")
        .mount(&server)
        .await;
    let ranged_wheel = wheel.clone();
    // The range request should not be sent to the redirect target.
    Mock::given(method("GET"))
        .and(path("/head-wheel"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(move |request: &Request| wheel_range_response(request, &ranged_wheel))
        .expect(0)
        .named("ranged GET request to the same-origin redirect target")
        .mount(&server)
        .await;
    // The streaming retry should follow the redirect with the source credentials intact.
    Mock::given(method("GET"))
        .and(path("/head-wheel"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .expect(1)
        .named("streaming GET request to the same-origin redirect target")
        .mount(&server)
        .await;

    assert_wheel_metadata_readable(&server).await?;

    Ok(())
}

/// Models registries that redirect wheels to another artifact origin, such as Azure Artifacts
/// redirecting to `vsblob.vsassets.io` or Gemfury and pypicloud redirecting to Amazon S3. The source
/// `Authorization` header must not be forwarded to the artifact host.
#[tokio::test]
async fn remote_metadata_redirect_cross_origin() -> Result<()> {
    let source_server = MockServer::start().await;
    let target_server = MockServer::start().await;
    let wheel = wheel()?;
    let wheel_len = wheel.len();
    let target = format!("{}/head-wheel", target_server.uri());

    // The initial metadata probe should authenticate to the source and receive a redirect.
    Mock::given(method("HEAD"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target.clone()))
        .expect(1)
        .named("HEAD request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The range reader should retry the source with an authenticated range request.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target.clone()))
        .expect(1)
        .named("ranged GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // A streaming retry should be sent to the source when the range request cannot be used.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target))
        .expect(1)
        .named("streaming fallback GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The redirected `HEAD` request should omit the source credentials on the new origin.
    Mock::given(method("HEAD"))
        .and(path("/head-wheel"))
        .and(header_missing(AUTHORIZATION))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(ACCEPT_RANGES, "bytes")
                .insert_header(CONTENT_LENGTH, wheel_len.to_string()),
        )
        .expect(1)
        .named("unauthenticated HEAD request to the cross-origin redirect target")
        .mount(&target_server)
        .await;
    let ranged_wheel = wheel.clone();
    // The range request should not be sent to the cross-origin redirect target.
    Mock::given(method("GET"))
        .and(path("/head-wheel"))
        .and(header_missing(AUTHORIZATION))
        .and(header_exists(RANGE.as_str()))
        .respond_with(move |request: &Request| wheel_range_response(request, &ranged_wheel))
        .expect(0)
        .named("unauthenticated ranged GET request to the cross-origin redirect target")
        .mount(&target_server)
        .await;
    // The streaming retry should follow the redirect without forwarding source credentials.
    Mock::given(method("GET"))
        .and(path("/head-wheel"))
        .and(header_missing(AUTHORIZATION))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .expect(1)
        .named("unauthenticated streaming GET request to the cross-origin redirect target")
        .mount(&target_server)
        .await;

    assert_wheel_metadata_readable(&source_server).await?;

    Ok(())
}

/// Models registries that issue method-specific signed redirects, such as Gemfury and pypicloud
/// backed by Amazon S3 (astral-sh/uv#2025 and astral-sh/uv#3255) and the public Microsoft package
/// feed backed by Azure Artifacts (astral-sh/uv#21347).
#[tokio::test]
async fn remote_metadata_redirect_method_specific_target() -> Result<()> {
    let source_server = MockServer::start().await;
    let target_server = MockServer::start().await;
    // Separate the metadata from the central directory so reading it requires multiple ranges.
    let mut writer = ZipFileWriter::new(Vec::new());
    writer
        .write_entry_whole(
            ZipEntryBuilder::new("ok-1.0.0.dist-info/METADATA".into(), Compression::Stored),
            b"Metadata-Version: 2.1\nName: ok\nVersion: 1.0.0\n",
        )
        .await?;
    writer
        .write_entry_whole(
            ZipEntryBuilder::new("padding".into(), Compression::Stored),
            &[0; 32_768],
        )
        .await?;
    let wheel = writer.close().await?;
    let wheel_len = wheel.len();
    let head_target = authenticated_url(
        &target_server.uri(),
        "/head-wheel",
        "head-user",
        "head-password",
    )?;
    let get_target = authenticated_url(
        &target_server.uri(),
        "/get-wheel",
        "get-user",
        "get-password",
    )?;

    // The initial authenticated probe should receive the signed `HEAD` target.
    Mock::given(method("HEAD"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, head_target))
        .expect(1)
        .named("HEAD request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The authenticated range request should receive the distinct signed `GET` target.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, get_target.clone()))
        .expect(1)
        .named("ranged GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The streaming retry should receive the same signed `GET` target.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, get_target))
        .expect(1)
        .named("streaming fallback GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The redirected probe should use the credentials embedded in the signed `HEAD` target.
    Mock::given(method("HEAD"))
        .and(path("/head-wheel"))
        .and(basic_auth("head-user", "head-password"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(ACCEPT_RANGES, "bytes")
                .insert_header(CONTENT_LENGTH, wheel_len.to_string()),
        )
        .expect(1)
        .named("HEAD request to the method-specific HEAD redirect target")
        .mount(&target_server)
        .await;
    let ranged_wheel = wheel.clone();
    // The range request should not be sent to the signed `GET` target.
    Mock::given(method("GET"))
        .and(path("/get-wheel"))
        .and(basic_auth("get-user", "get-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(move |request: &Request| wheel_range_response(request, &ranged_wheel))
        .expect(0)
        .named("ranged GET request to the method-specific GET redirect target")
        .mount(&target_server)
        .await;
    // The streaming retry should use the credentials embedded in the signed `GET` target.
    Mock::given(method("GET"))
        .and(path("/get-wheel"))
        .and(basic_auth("get-user", "get-password"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .expect(1)
        .named("streaming GET request to the method-specific GET redirect target")
        .mount(&target_server)
        .await;
    // A `GET` request should not reuse the signed `HEAD` target.
    Mock::given(method("GET"))
        .and(path("/head-wheel"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("GET request must not reuse the HEAD redirect target")
        .mount(&target_server)
        .await;
    // A `HEAD` request should not reuse the signed `GET` target.
    Mock::given(method("HEAD"))
        .and(path("/get-wheel"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("HEAD request must not reuse the GET redirect target")
        .mount(&target_server)
        .await;

    assert_wheel_metadata_readable(&source_server).await?;

    Ok(())
}

/// Some servers support bounded ranges but reject suffix ranges. Wheel metadata should be read with
/// a bounded range request, without attempting a suffix range or streaming fallback.
#[tokio::test]
async fn remote_metadata_bounded_ranges() -> Result<()> {
    let server = MockServer::start().await;
    let wheel = wheel()?;
    // The initial `HEAD` response should advertise bounded range support and the artifact length.
    Mock::given(method("HEAD"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(ACCEPT_RANGES, "bytes")
                .insert_header(CONTENT_LENGTH, wheel.len().to_string()),
        )
        .expect(1)
        .mount(&server)
        .await;
    // The metadata should be read with a bounded range request.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_regex(RANGE.as_str(), "^bytes=[0-9]+-[0-9]+$"))
        .respond_with(move |request: &Request| wheel_range_response(request, &wheel))
        .expect(1)
        .named("bounded range request")
        .mount(&server)
        .await;
    // A suffix range request should not be sent when bounded ranges are supported.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(header_regex(RANGE.as_str(), "^bytes=-"))
        .respond_with(ResponseTemplate::new(416))
        .expect(0)
        .named("unsupported suffix range request")
        .mount(&server)
        .await;
    // A streaming fallback should not be needed when the bounded range request succeeds.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("unnecessary streaming fallback")
        .mount(&server)
        .await;

    assert_wheel_metadata_readable(&server).await
}

/// A redirect target may reject range requests while allowing a full download. The range request
/// should not reach the target; metadata should be read after retrying the source without a `Range`
/// header.
#[tokio::test]
async fn remote_metadata_redirect_range_forbidden() -> Result<()> {
    let source_server = MockServer::start().await;
    let target_server = MockServer::start().await;
    let wheel = wheel()?;
    let target = format!("{}/wheel", target_server.uri());
    // The initial metadata probe should authenticate to the source and receive a redirect.
    Mock::given(method("HEAD"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target.clone()))
        .expect(1)
        .mount(&source_server)
        .await;
    // The range reader should retry the source with an authenticated range request.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_exists(RANGE.as_str()))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target.clone()))
        .expect(1)
        .named("ranged GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // A streaming retry should be sent to the source when the range request cannot be used.
    Mock::given(method("GET"))
        .and(path("/artifact"))
        .and(basic_auth("source-user", "source-password"))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(303).insert_header(LOCATION, target))
        .expect(1)
        .named("streaming fallback GET request to the redirecting wheel URL")
        .mount(&source_server)
        .await;
    // The redirected `HEAD` request should omit the source credentials, and its response should
    // advertise range support.
    Mock::given(method("HEAD"))
        .and(path("/wheel"))
        .and(header_missing(AUTHORIZATION))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(ACCEPT_RANGES, "bytes")
                .insert_header(CONTENT_LENGTH, wheel.len().to_string()),
        )
        .expect(1)
        .mount(&target_server)
        .await;
    // The range request should not be sent to the redirect target.
    Mock::given(method("GET"))
        .and(path("/wheel"))
        .and(header_missing(AUTHORIZATION))
        .and(header_exists(RANGE.as_str()))
        .respond_with(ResponseTemplate::new(403))
        .expect(0)
        .named("forbidden range request to the redirect target")
        .mount(&target_server)
        .await;
    // The streaming retry should follow the redirect without forwarding source credentials.
    Mock::given(method("GET"))
        .and(path("/wheel"))
        .and(header_missing(AUTHORIZATION))
        .and(header_missing(RANGE))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wheel, "application/octet-stream"))
        .expect(1)
        .named("streaming GET request to the redirect target")
        .mount(&target_server)
        .await;

    assert_wheel_metadata_readable(&source_server).await
}

#[derive(Debug)]
struct HeaderMissing(HeaderName);

impl Match for HeaderMissing {
    fn matches(&self, request: &Request) -> bool {
        !request.headers.contains_key(&self.0)
    }
}

/// Matches requests that omit a header, complementing Wiremock's `header_exists` matcher.
fn header_missing(header: HeaderName) -> HeaderMissing {
    HeaderMissing(header)
}

/// Loads the wheel fixture served by each redirect target.
fn wheel() -> Result<Vec<u8>> {
    Ok(fs_err::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/links/ok-1.0.0-py3-none-any.whl"),
    )?)
}

/// Reads wheel metadata through the authenticated source URL shared by each redirect scenario.
async fn assert_wheel_metadata_readable(source_server: &MockServer) -> Result<()> {
    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build()?;
    let url = authenticated_url(
        &source_server.uri(),
        "/artifact",
        "source-user",
        "source-password",
    )?;
    let dist = BuiltDist::DirectUrl(DirectUrlBuiltDist {
        filename: WheelFilename::from_str("ok-1.0.0-py3-none-any.whl")?,
        location: Box::new(DisplaySafeUrl::parse(&url)?),
        url: VerbatimUrl::from_str(&url)?,
        size: None,
    });
    let metadata = client
        .wheel_metadata(
            &dist,
            &GitResolver::default(),
            &IndexCapabilities::default(),
            None,
        )
        .await?;
    assert_eq!(metadata.version.to_string(), "1.0.0");
    Ok(())
}

/// Adds Basic authentication credentials to a Wiremock server URL.
fn authenticated_url(base: &str, path: &str, username: &str, password: &str) -> Result<String> {
    Ok(format!(
        "http://{username}:{password}@{}{path}",
        base.strip_prefix("http://")
            .context("mock server URL should use HTTP")?
    ))
}

/// Serves a byte range from the wheel fixture, as an artifact host would.
fn wheel_range_response(request: &Request, wheel: &[u8]) -> ResponseTemplate {
    let Some((start, end)) = request
        .headers
        .get(RANGE)
        .and_then(|range| range.to_str().ok())
        .and_then(|range| parse_byte_range(range, wheel.len()))
    else {
        return ResponseTemplate::new(416)
            .insert_header(ACCEPT_RANGES, "bytes")
            .insert_header(CONTENT_RANGE, format!("bytes */{}", wheel.len()));
    };
    ResponseTemplate::new(206)
        .insert_header(ACCEPT_RANGES, "bytes")
        .insert_header(
            CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", wheel.len()),
        )
        .set_body_raw(wheel[start..=end].to_vec(), "application/octet-stream")
}

/// Parses the single byte-range forms emitted by the range reader.
fn parse_byte_range(range: &str, length: usize) -> Option<(usize, usize)> {
    let range = range.strip_prefix("bytes=")?;
    let (start, end) = range.split_once('-')?;

    if start.is_empty() {
        let suffix = end.parse::<usize>().ok()?;
        return (suffix > 0 && length > 0).then(|| (length.saturating_sub(suffix), length - 1));
    }

    let start = start.parse::<usize>().ok()?;
    if start >= length {
        return None;
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<usize>().ok()?.min(length - 1)
    };
    (start <= end).then_some((start, end))
}
