use std::str::FromStr;

use anyhow::Result;

use distribution_filename::WheelFilename;
use distribution_types::{BuiltDist, DirectUrlBuiltDist};
use pep508_rs::VerbatimUrl;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;

#[tokio::test]
async fn remote_metadata_with_and_without_cache() -> Result<()> {
    let cache = Cache::temp()?;
    let client = RegistryClientBuilder::new(cache).build();

    // The first run is without cache (the tempdir is empty), the second has the cache from the
    // first run.
    for _ in 0..2 {
        let url = "https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl";
        let filename = WheelFilename::from_str(url.rsplit_once('/').unwrap().1)?;
        let dist = BuiltDist::DirectUrl(DirectUrlBuiltDist {
            filename,
            url: VerbatimUrl::from_str(url).unwrap(),
        });
        let metadata = client.wheel_metadata(&dist).await.unwrap();
        assert_eq!(metadata.version.to_string(), "4.66.1");
    }

    Ok(())
}
