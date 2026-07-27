use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
