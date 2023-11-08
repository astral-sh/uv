//! Fetch and build source distributions from remote sources.
//!
//! TODO(charlie): Unify with `crates/puffin-installer/src/sdist_builder.rs`.

use std::str::FromStr;
use std::sync::Arc;

use anyhow::{bail, Result};
use fs_err::tokio as fs;

use tempfile::tempdir_in;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_client::RegistryClient;
use puffin_distribution::source::Source;
use puffin_distribution::{Distribution, RemoteDistributionRef};
use puffin_git::{GitSource, GitUrl};
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::distribution::cached_wheel::CachedWheel;

const BUILT_WHEELS_CACHE: &str = "built-wheels-v0";

const GIT_CACHE: &str = "git-v0";

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub(crate) struct SourceDistributionFetcher<'a, T: BuildContext> {
    build_context: &'a T,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, T: BuildContext> SourceDistributionFetcher<'a, T> {
    /// Initialize a [`SourceDistributionFetcher`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub(crate) fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Arc::new(reporter)),
            ..self
        }
    }

    /// Read the [`Metadata21`] from a built source distribution, if it exists in the cache.
    pub(crate) fn find_dist_info(
        &self,
        distribution: &Distribution,
        tags: &Tags,
    ) -> Result<Option<Metadata21>> {
        CachedWheel::find_in_cache(
            distribution,
            tags,
            self.build_context.cache().join(BUILT_WHEELS_CACHE),
        )
        .as_ref()
        .map(CachedWheel::read_dist_info)
        .transpose()
    }

    /// Download and build a source distribution, storing the built wheel in the cache.
    pub(crate) async fn download_and_build_sdist(
        &self,
        distribution: &Distribution,
        client: &RegistryClient,
    ) -> Result<Metadata21> {
        debug!("Building: {distribution}");

        if self.build_context.no_build() {
            bail!("Building source distributions is disabled");
        }

        // This could extract the subdirectory.
        let source = Source::try_from(distribution)?;
        let (sdist_file, subdirectory) = match source {
            Source::RegistryUrl(url) => {
                debug!("Fetching source distribution from registry: {url}");

                let reader = client.stream_external(&url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempdir_in(self.build_context.cache())?.into_path();
                let sdist_filename = distribution.filename()?;
                let sdist_file = temp_dir.join(sdist_filename.as_ref());
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                // Registry dependencies can't specify a subdirectory.
                let subdirectory = None;

                (sdist_file, subdirectory)
            }
            Source::RemoteUrl(url, subdirectory) => {
                debug!("Fetching source distribution from URL: {url}");

                let reader = client.stream_external(url).await?;
                let mut reader = tokio::io::BufReader::new(reader.compat());

                // Download the source distribution.
                let temp_dir = tempdir_in(self.build_context.cache())?.into_path();
                let sdist_filename = distribution.filename()?;
                let sdist_file = temp_dir.join(sdist_filename.as_ref());
                let mut writer = tokio::fs::File::create(&sdist_file).await?;
                tokio::io::copy(&mut reader, &mut writer).await?;

                (sdist_file, subdirectory)
            }
            Source::Git(git, subdirectory) => {
                debug!("Fetching source distribution from Git: {git}");

                let git_dir = self.build_context.cache().join(GIT_CACHE);
                let source = if let Some(reporter) = &self.reporter {
                    GitSource::new(git, git_dir).with_reporter(Facade::from(reporter.clone()))
                } else {
                    GitSource::new(git, git_dir)
                };
                let sdist_file = tokio::task::spawn_blocking(move || source.fetch())
                    .await??
                    .into();

                (sdist_file, subdirectory)
            }
        };

        // Create a directory for the wheel.
        let wheel_dir = self
            .build_context
            .cache()
            .join(BUILT_WHEELS_CACHE)
            .join(distribution.id());
        fs::create_dir_all(&wheel_dir).await?;

        // Build the wheel.
        let disk_filename = self
            .build_context
            .build_source(
                &sdist_file,
                subdirectory.as_deref(),
                &wheel_dir,
                &distribution.to_string(),
            )
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

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    pub(crate) async fn precise(&self, distribution: &Distribution) -> Result<Option<Url>> {
        // STOPSHIP: Match on Git...
        let source = Source::try_from(distribution)?;

        // Generate a DirectGitUrl from the Url.

        let Source::Git(git, subdirectory) = source else {
            return Ok(None);
        };

        // If the commit already contains a complete SHA, short-circuit.
        if git.precise().is_some() {
            return Ok(None);
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let git_dir = self.build_context.cache().join(GIT_CACHE);
        let source = if let Some(reporter) = &self.reporter {
            GitSource::new(git, git_dir).with_reporter(Facade::from(reporter.clone()))
        } else {
            GitSource::new(git, git_dir)
        };
        let precise = tokio::task::spawn_blocking(move || source.fetch()).await??;
        let git = GitUrl::from(precise);

        // Convert DirectGitUrl back to a Url.

        // Re-encode as a URL.
        let source = Source::Git(git, subdirectory);
        Ok(Some(source.into()))
    }
}

pub(crate) trait Reporter: Send + Sync {
    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to  [`puffin_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl From<Arc<dyn Reporter>> for Facade {
    fn from(reporter: Arc<dyn Reporter>) -> Self {
        Self { reporter }
    }
}

impl puffin_git::Reporter for Facade {
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}
