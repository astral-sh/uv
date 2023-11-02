use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use fs_err::tokio as fs;
use tempfile::tempdir;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::RemoteDistributionRef;
use puffin_package::pypi_types::Metadata21;

use crate::distribution::cached_wheel::CachedWheel;

const REMOTE_WHEELS_CACHE: &str = "remote-wheels-v0";

/// Fetch a built distribution from a remote source, or from a local cache.
pub(crate) struct WheelFetcher<'a>(Option<&'a Path>);

impl<'a> WheelFetcher<'a> {
    /// Initialize a [`WheelFetcher`] from a [`BuildContext`].
    pub(crate) fn new(cache: Option<&'a Path>) -> Self {
        Self(cache)
    }

    /// Read the [`Metadata21`] from a wheel, if it exists in the cache.
    pub(crate) fn find_dist_info(
        &self,
        distribution: &RemoteDistributionRef<'_>,
        tags: &Tags,
    ) -> Result<Option<Metadata21>> {
        let Some(cache) = self.0 else {
            return Ok(None);
        };
        CachedWheel::find_in_cache(distribution, tags, &cache.join(REMOTE_WHEELS_CACHE))
            .as_ref()
            .map(CachedWheel::read_dist_info)
            .transpose()
    }

    /// Download a wheel, storing it in the cache.
    pub(crate) async fn download_wheel(
        &self,
        distribution: &RemoteDistributionRef<'_>,
        client: &RegistryClient,
    ) -> Result<Metadata21> {
        debug!("Downloading: {distribution}");
        let url = distribution.url()?;
        let reader = client.stream_external(&url).await?;
        let mut reader = tokio::io::BufReader::new(reader.compat());
        let temp_dir = tempdir()?;

        // Create a directory for the wheel.
        let wheel_dir = self.0.map_or_else(
            || temp_dir.path().join(REMOTE_WHEELS_CACHE),
            |cache| cache.join(REMOTE_WHEELS_CACHE).join(distribution.id()),
        );
        fs::create_dir_all(&wheel_dir).await?;

        // Download the wheel.
        let wheel_filename = distribution.filename()?;
        let wheel_file = wheel_dir.join(wheel_filename.as_ref());
        let mut writer = tokio::fs::File::create(&wheel_file).await?;
        tokio::io::copy(&mut reader, &mut writer).await?;

        // Read the metadata from the wheel.
        let wheel = CachedWheel::new(wheel_file, WheelFilename::from_str(&wheel_filename)?);
        let metadata21 = wheel.read_dist_info()?;

        debug!("Finished downloading: {distribution}");
        Ok(metadata21)
    }
}
