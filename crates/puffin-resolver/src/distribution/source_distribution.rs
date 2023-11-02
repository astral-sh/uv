use std::str::FromStr;

use anyhow::Result;
use fs_err::tokio as fs;
use tempfile::tempdir_in;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::RemoteDistributionRef;
use puffin_git::GitSource;
use puffin_package::pypi_types::Metadata21;
use puffin_traits::BuildContext;

use crate::distribution::cached_wheel::CachedWheel;
use crate::distribution::source::Source;

const BUILT_WHEELS_CACHE: &str = "built-wheels-v0";

const GIT_CACHE: &str = "git-v0";

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub(crate) struct SourceDistributionFetcher<'a, T: BuildContext>(&'a T);

impl<'a, T: BuildContext> SourceDistributionFetcher<'a, T> {
    /// Initialize a [`SourceDistributionFetcher`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self(build_context)
    }

    /// Read the [`Metadata21`] from a built source distribution, if it exists in the cache.
    pub(crate) fn find_dist_info(
        &self,
        distribution: &RemoteDistributionRef<'_>,
        tags: &Tags,
    ) -> Result<Option<Metadata21>> {
        CachedWheel::find_in_cache(distribution, tags, self.0.cache().join(BUILT_WHEELS_CACHE))
            .as_ref()
            .map(CachedWheel::read_dist_info)
            .transpose()
    }

    /// Download and build a source distribution, storing the built wheel in the cache.
    pub(crate) async fn download_and_build_sdist(
        &self,
        distribution: &RemoteDistributionRef<'_>,
        client: &RegistryClient,
    ) -> Result<Metadata21> {
        debug!("Building: {distribution}");

        let source = Source::try_from(distribution)?;
        let sdist_file = match source {
            Source::Url(url) => {
                debug!("Fetching source distribution from: {url}");

                let reader = client.stream_external(&url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempdir_in(self.0.cache())?.into_path();
                let sdist_filename = distribution.filename()?;
                let sdist_file = temp_dir.join(sdist_filename.as_ref());
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                sdist_file
            }
            Source::Git(git) => {
                debug!("Fetching source distribution from: {git}");

                let git_dir = self.0.cache().join(GIT_CACHE);
                let source = GitSource::new(git, git_dir);
                tokio::task::spawn_blocking(move || source.fetch()).await??
            }
        };

        // Create a directory for the wheel.
        let wheel_dir = self
            .0
            .cache()
            .join(BUILT_WHEELS_CACHE)
            .join(distribution.id());
        fs::create_dir_all(&wheel_dir).await?;

        // Build the wheel.
        let disk_filename = self
            .0
            .build_source_distribution(&sdist_file, &wheel_dir)
            .await?;

        // Read the metadata from the wheel.
        let wheel = CachedWheel::new(
            wheel_dir.join(&disk_filename),
            WheelFilename::from_str(&disk_filename)?,
        );
        let metadata21 = wheel.read_dist_info()?;

        debug!("Finished building: {distribution}");
        Ok(metadata21)
    }
}
