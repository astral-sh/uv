use std::str::FromStr;

use anyhow::Result;
use tempfile::tempdir;
use url::Url;

use distribution_filename::WheelFilename;
use puffin_client::RegistryClientBuilder;

#[tokio::test]
async fn remote_metadata_with_and_without_cache() -> Result<()> {
    let temp_cache = tempdir().unwrap();
    let client = RegistryClientBuilder::default()
        .cache(Some(temp_cache.path().to_path_buf()))
        .build();
    // The first run is without cache (the tempdir is empty), the second has the cache from the
    // first run
    for _ in 0..2 {
        let url = "https://files.pythonhosted.org/packages/00/e5/f12a80907d0884e6dff9c16d0c0114d81b8cd07dc3ae54c5e962cc83037e/tqdm-4.66.1-py3-none-any.whl";
        let filename = WheelFilename::from_str(url.rsplit_once('/').unwrap().1).unwrap();
        let metadata = client
            .wheel_metadata_no_index(&filename, &Url::parse(url).unwrap())
            .await
            .unwrap();
        assert_eq!(metadata.summary.unwrap(), "Fast, Extensible Progress Meter");
    }
    Ok(())
}
