use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
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
async fn remote_metadata_uses_head_redirect_target_for_range_requests() -> Result<()> {
    let redirecting_server = MockServer::start().await;
    let wheel_server = MockServer::start().await;
    let wheel = fs_err::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/links/ok-1.0.0-py3-none-any.whl"),
    )?;
    let wheel_len = wheel.len();
    let redirect = ResponseTemplate::new(303)
        .insert_header("Location", format!("{}/wheel", wheel_server.uri()));

    Mock::given(method("HEAD"))
        .and(path("/ok-1.0.0-py3-none-any.whl"))
        .respond_with(redirect.clone())
        .expect(1)
        .named("HEAD request to redirecting wheel URL")
        .mount(&redirecting_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ok-1.0.0-py3-none-any.whl"))
        .respond_with(redirect)
        .expect(0)
        .named("range request to redirecting wheel URL")
        .mount(&redirecting_server)
        .await;
    Mock::given(method("HEAD"))
        .and(path("/wheel"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Accept-Ranges", "bytes")
                .insert_header("Content-Length", wheel_len.to_string()),
        )
        .expect(1)
        .named("HEAD request to final wheel URL")
        .mount(&wheel_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/wheel"))
        .respond_with(move |request: &Request| wheel_range_response(request, &wheel))
        .expect(1..)
        .named("range requests to final wheel URL")
        .mount(&wheel_server)
        .await;

    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build()?;
    let url = format!("{}/ok-1.0.0-py3-none-any.whl", redirecting_server.uri());
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

fn wheel_range_response(request: &Request, wheel: &[u8]) -> ResponseTemplate {
    let Some((start, end)) = request
        .headers
        .get("range")
        .and_then(|range| range.to_str().ok())
        .and_then(|range| parse_byte_range(range, wheel.len()))
    else {
        return ResponseTemplate::new(500);
    };
    ResponseTemplate::new(206)
        .insert_header("Accept-Ranges", "bytes")
        .insert_header(
            "Content-Range",
            format!("bytes {start}-{end}/{}", wheel.len()),
        )
        .set_body_raw(wheel[start..=end].to_vec(), "application/octet-stream")
}

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
