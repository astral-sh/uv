use std::str::FromStr;

use anyhow::{anyhow, Result};
use fs_err::tokio as fs;
use tempfile::tempdir_in;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_git::{Git, GitSource};
use puffin_package::pypi_types::Metadata21;
use puffin_traits::BuildContext;

use crate::distribution::cached_wheel::CachedWheel;
use crate::distribution::precise::Precise;

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
        distribution: &Precise<'_>,
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
        distribution: &Precise<'_>,
        client: &RegistryClient,
    ) -> Result<Metadata21> {
        debug!("Building: {distribution}");

        let sdist_file = match distribution {
            Precise::Registry(.., file) => {
                debug!("Fetching source distribution from registry: {}", file.url);

                let reader = client.stream_external(&file.url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempdir_in(self.0.cache())?.into_path();
                let sdist_file = temp_dir.join(&file.filename);
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                sdist_file
            }
            Precise::Url(.., url) => {
                debug!("Fetching source distribution from URL: {url}");

                let reader = client.stream_external(&url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempdir_in(self.0.cache())?.into_path();
                let sdist_file = temp_dir.join(url.path_segments()
                    .and_then(Iterator::last)
                    .ok_or_else(|| anyhow!("Could not parse filename from URL: {url}"))?);
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                sdist_file
            }
            Precise::Git(.., git) => {
                debug!(
                    "Building source distribution from Git checkout: {}",
                    git.revision()
                );

                git.checkout().to_path_buf()
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

    /// Given a URL dependency for a source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    pub(crate) async fn precise(&self, url: &Url) -> Result<Option<Url>> {
        let url = url.as_str().strip_prefix("git+")?;
        let url = Url::parse(url)?;
        let git = Git::try_from(url)?;
        let git_dir = self.0.cache().join(GIT_CACHE);
        let source = GitSource::new(git, git_dir);
        let precise = tokio::task::spawn_blocking(move || source.fetch()).await??;
        Ok(Some(Url::from(precise)))
    }
}
