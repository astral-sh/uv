use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use url::Url;
use wiremock::matchers::{basic_auth, header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuiltDist, DirectUrlBuiltDist, IndexCapabilities};
use uv_git::GitResolver;
use uv_pep508::VerbatimUrl;
use uv_redacted::DisplaySafeUrl;

#[tokio::test]
async fn wheel_central_directory_larger_than_initial_range() -> Result<()> {
    let server = MockServer::start().await;
    let mut writer = ZipFileWriter::new(Vec::new());
    for index in 0..500 {
        writer
            .write_entry_whole(
                ZipEntryBuilder::new(
                    format!("package/module_{index:04}.py").into(),
                    Compression::Stored,
                )
                .unix_permissions(0o755),
                b"VALUE = 1\n",
            )
            .await?;
    }
    let contents = writer.close().await?;
    let ranges = contents.clone();
    Mock::given(method("GET"))
        .and(path("/package-1.0.0-py3-none-any.whl"))
        .and(header_exists("range"))
        .and(header("if-match", "\"wheel\""))
        .and(header("accept-encoding", "identity"))
        .and(basic_auth("username", "password"))
        .respond_with(move |request: &wiremock::Request| {
            let range = request
                .headers
                .get("range")
                .and_then(|value| value.to_str().ok());
            let Some((start, end)) = range
                .and_then(|range| range.strip_prefix("bytes="))
                .and_then(|range| range.split_once('-'))
            else {
                return ResponseTemplate::new(400);
            };
            let (start, end) = if start.is_empty() {
                let Ok(length) = end.parse::<usize>() else {
                    return ResponseTemplate::new(400);
                };
                (ranges.len().saturating_sub(length), ranges.len() - 1)
            } else {
                let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) else {
                    return ResponseTemplate::new(400);
                };
                (start, end)
            };
            let Some(bytes) = ranges.get(start..=end) else {
                return ResponseTemplate::new(416);
            };
            ResponseTemplate::new(206)
                .insert_header(
                    "content-range",
                    format!("bytes {start}-{end}/{}", ranges.len()),
                )
                .set_body_bytes(bytes.to_vec())
        })
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/package-1.0.0-py3-none-any.whl"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"wheel\"")
                .set_body_bytes(contents),
        )
        .with_priority(2)
        .mount(&server)
        .await;

    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build()?;
    let url = format!("{}/package-1.0.0-py3-none-any.whl", server.uri())
        .replace("http://", "http://username:password@");
    let url = DisplaySafeUrl::parse(&url)?;
    let response = reqwest::Client::new()
        .get(Url::from(url.clone()))
        .send()
        .await?;
    let directory = client
        .wheel_central_directory(
            &url,
            &WheelFilename::from_str("package-1.0.0-py3-none-any.whl")?,
            &response,
        )
        .await?;
    assert_eq!(directory.entries().len(), 500);
    assert!(
        directory
            .entries()
            .iter()
            .all(|entry| entry.unix_permissions() == Some(0o755))
    );
    let requests = server
        .received_requests()
        .await
        .context("missing requests")?;
    let ranges = requests
        .iter()
        .filter(|request| request.headers.contains_key("range"))
        .count();
    assert_eq!(ranges, 2);
    assert_eq!(requests.len(), 3);
    Ok(())
}

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
