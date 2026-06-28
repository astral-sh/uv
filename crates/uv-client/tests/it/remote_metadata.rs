use std::{path::Path, str::FromStr};

use anyhow::{Result, anyhow};
use fs_err as fs;
use url::Url;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, RegistryClientBuilder};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuiltDist, DirectUrlBuiltDist, File, FileLocation, IndexCapabilities, IndexUrl,
    RegistryBuiltDist, RegistryBuiltWheel,
};
use uv_git::GitResolver;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::HashDigests;
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;

#[tokio::test]
async fn remote_metadata_with_and_without_cache() -> Result<()> {
    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build()?;

    // The first run is without cache (the tempdir is empty), the second has the cache from the
    // first run.
    for _ in 0..2 {
        let url = "https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl";
        let filename = WheelFilename::from_str(url.rsplit_once('/').unwrap().1)?;
        let dist = BuiltDist::DirectUrl(DirectUrlBuiltDist {
            filename,
            location: Box::new(DisplaySafeUrl::parse(url)?),
            url: VerbatimUrl::from_str(url)?,
        });
        let resolver = GitResolver::default();
        let capabilities = IndexCapabilities::default();
        let metadata = client
            .wheel_metadata(&dist, &resolver, &capabilities, None)
            .await?;
        assert_eq!(metadata.version.to_string(), "4.66.1");
    }

    Ok(())
}

#[tokio::test]
async fn offline_registry_metadata_is_cached_in_memory() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let wheel_name = "basic_package-0.1.0-py3-none-any.whl";
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/links")
        .join(wheel_name);
    let wheel_path = directory.path().join(wheel_name);
    fs::copy(source, &wheel_path)?;

    let wheel_url = Url::from_file_path(&wheel_path)
        .map_err(|()| anyhow!("wheel path cannot be represented as a URL"))?;
    let filename = WheelFilename::from_str(wheel_name)?;
    let distribution = BuiltDist::Registry(RegistryBuiltDist {
        wheels: vec![RegistryBuiltWheel {
            filename,
            file: Box::new(File {
                dist_info_metadata: false,
                filename: SmallString::from(wheel_name),
                hashes: HashDigests::empty(),
                requires_python: None,
                size: None,
                upload_time_utc_ms: None,
                url: FileLocation::new(
                    SmallString::from(wheel_url.as_str()),
                    &SmallString::from(""),
                ),
                yanked: None,
                zstd: None,
            }),
            index: IndexUrl::from_str("https://example.com/simple")?,
        }],
        best_wheel_index: 0,
        sdist: None,
    });
    let client = RegistryClientBuilder::new(
        BaseClientBuilder::default().connectivity(Connectivity::Offline),
        Cache::temp()?,
    )
    .build()?;
    let resolver = GitResolver::default();
    let capabilities = IndexCapabilities::default();

    let metadata = client
        .wheel_metadata(&distribution, &resolver, &capabilities, None)
        .await?;
    assert_eq!(metadata.name.as_ref(), "basic-package");
    assert_eq!(metadata.version.to_string(), "0.1.0");

    fs::remove_file(&wheel_path)?;
    let metadata = client
        .clone()
        .wheel_metadata(&distribution, &resolver, &capabilities, None)
        .await?;
    assert_eq!(metadata.name.as_ref(), "basic-package");
    assert_eq!(metadata.version.to_string(), "0.1.0");

    Ok(())
}
