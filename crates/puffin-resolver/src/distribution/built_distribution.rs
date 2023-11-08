use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use fs_err::tokio as fs;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::{DirectUrlBuiltDistribution, DistributionIdentifier, RemoteDistribution};
use pypi_types::Metadata21;

use crate::distribution::cached_wheel::CachedWheel;

const REMOTE_WHEELS_CACHE: &str = "remote-wheels-v0";

/// Fetch a built distribution from a remote source, or from a local cache.
pub(crate) struct BuiltDistributionFetcher<'a>(&'a Path);

impl<'a> BuiltDistributionFetcher<'a> {
    /// Initialize a [`BuiltDistributionFetcher`] from a [`BuildContext`].
    pub(crate) fn new(cache: &'a Path) -> Self {
        Self(cache)
    }

    /// Read the [`Metadata21`] from a wheel, if it exists in the cache.
    pub(crate) fn find_dist_info(
        &self,
        distribution: &DirectUrlBuiltDistribution,
        tags: &Tags,
    ) -> Result<Option<Metadata21>> {
        CachedWheel::find_in_cache(distribution, tags, self.0.join(REMOTE_WHEELS_CACHE))
            .as_ref()
            .map(|wheel| CachedWheel::read_dist_info(wheel).context("Failed to read dist info"))
            .transpose()
    }

    /// Download a wheel, storing it in the cache.
    pub(crate) async fn download_wheel(
        &self,
        distribution: &DirectUrlBuiltDistribution,
        client: &RegistryClient,
    ) -> Result<Metadata21> {
        debug!("Downloading: {distribution}");
        let reader = client.stream_external(&distribution.url).await?;

        // Create a directory for the wheel.
        let wheel_dir = self
            .0
            .join(REMOTE_WHEELS_CACHE)
            .join(distribution.distribution_id());
        fs::create_dir_all(&wheel_dir).await?;

        // Download the wheel.
        let wheel_filename = distribution.filename()?;
        let wheel_file = wheel_dir.join(wheel_filename);
        let mut writer = tokio::fs::File::create(&wheel_file).await?;
        tokio::io::copy(&mut reader.compat(), &mut writer).await?;

        // Read the metadata from the wheel.
        let wheel = CachedWheel::new(wheel_file, WheelFilename::from_str(wheel_filename)?);
        let metadata21 = wheel.read_dist_info()?;

        debug!("Finished downloading: {distribution}");
        Ok(metadata21)
    }
}
