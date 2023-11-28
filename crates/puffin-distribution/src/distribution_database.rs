use std::borrow::Cow;
use std::cmp::Reverse;
use std::io;
use std::str::FromStr;
use std::sync::Arc;

use bytesize::ByteSize;
use fs_err::tokio as fs;
use futures::StreamExt;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;

use distribution_filename::{WheelFilename, WheelFilenameError};
use distribution_types::direct_url::DirectGitUrl;
use distribution_types::{BuiltDist, Dist, Metadata, RemoteSource, SourceDist};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket};
use puffin_client::RegistryClient;
use puffin_git::GitSource;
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::download::BuiltWheel;
use crate::locks::Locks;
use crate::reporter::Facade;
use crate::{
    DiskWheel, Download, InMemoryWheel, LocalWheel, Reporter, SourceDistCachedBuilder,
    SourceDistError,
};

#[derive(Debug, Error)]
pub enum DistributionDatabaseError {
    #[error("Failed to parse '{0}' as url")]
    Url(String, #[source] url::ParseError),
    #[error(transparent)]
    WheelFilename(#[from] WheelFilenameError),
    #[error(transparent)]
    Client(#[from] puffin_client::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Distribution(#[from] distribution_types::Error),
    #[error(transparent)]
    SourceBuild(#[from] SourceDistError),
    #[error("Failed to build")]
    Build(#[source] anyhow::Error),
    #[error("Git operation failed")]
    Git(#[source] anyhow::Error),
    /// Should not occur, i've only seen it when another task panicked
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Building source distributions is disabled")]
    NoBuild,
}

/// A cached high-level interface to convert distributions (a requirement resolved to a location)
/// to a wheel or wheel metadata.
///
/// For wheel metadata, this happens by either fetching the metadata from the remote wheel or by
/// building the source distribution. For wheel files, either the wheel is downloaded or a source
/// distribution is downloaded, built and the new wheel gets returned.
///
/// All kinds of wheel sources (index, url, path) and source distribution source (index, url, path,
/// git) are supported.
///
/// This struct also has the task of acquiring locks around source dist builds in general and git
/// operation especially.
pub struct DistributionDatabase<'a, Context: BuildContext + Send + Sync> {
    cache: &'a Cache,
    reporter: Option<Arc<dyn Reporter>>,
    locks: Arc<Locks>,
    client: &'a RegistryClient,
    build_context: &'a Context,
    builder: SourceDistCachedBuilder<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> DistributionDatabase<'a, Context> {
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            cache,
            reporter: None,
            locks: Arc::new(Locks::default()),
            client,
            build_context,
            builder: SourceDistCachedBuilder::new(build_context, client.cached_client(), tags),
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter = Arc::new(reporter);
        Self {
            reporter: Some(reporter.clone()),
            builder: self.builder.with_reporter(reporter),
            ..self
        }
    }

    /// In parallel, either fetch the wheel or fetch and built source distributions.
    pub async fn get_wheels(
        &self,
        dists: Vec<Dist>,
    ) -> Result<Vec<LocalWheel>, DistributionDatabaseError> {
        // Sort the distributions by size.
        let mut dists = dists;
        dists.sort_unstable_by_key(|distribution| {
            Reverse(distribution.size().unwrap_or(usize::MAX))
        });

        // Optimization: Skip source dist download when we must not build them anyway
        if self.build_context.no_build() && dists.iter().any(|dist| matches!(dist, Dist::Source(_)))
        {
            return Err(DistributionDatabaseError::NoBuild);
        }

        // Fetch the distributions in parallel.
        let mut downloads_and_builds = Vec::with_capacity(dists.len());
        let mut fetches = futures::stream::iter(dists)
            .map(|dist| self.get_or_build_wheel(dist))
            .buffer_unordered(50);

        while let Some(result) = fetches.next().await.transpose()? {
            downloads_and_builds.push(result);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_download_and_build_complete();
        }

        Ok(downloads_and_builds)
    }

    /// Either fetch the wheel or fetch and build the source distribution
    async fn get_or_build_wheel(
        &self,
        dist: Dist,
    ) -> Result<LocalWheel, DistributionDatabaseError> {
        match &dist {
            Dist::Built(BuiltDist::Registry(wheel)) => {
                // Fetch the wheel.
                let url = Url::parse(&wheel.file.url).map_err(|err| {
                    DistributionDatabaseError::Url(wheel.file.url.to_string(), err)
                })?;
                let filename = WheelFilename::from_str(&wheel.file.filename)?;
                let reader = self.client.stream_external(&url).await?;

                // If the file is greater than 5MB, write it to disk; otherwise, keep it in memory.
                let small_size = if let Some(size) = wheel.file.size {
                    let byte_size = ByteSize::b(size as u64);
                    if byte_size < ByteSize::mb(5) {
                        Some(size)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let local_wheel = if let Some(small_size) = small_size {
                    debug!(
                        "Fetching in-memory wheel from registry: {dist} ({})",
                        ByteSize::b(small_size as u64)
                    );

                    // Read into a buffer.
                    let mut buffer = Vec::with_capacity(small_size);
                    let mut reader = tokio::io::BufReader::new(reader.compat());
                    tokio::io::copy(&mut reader, &mut buffer).await?;

                    LocalWheel::InMemory(InMemoryWheel {
                        dist: dist.clone(),
                        filename,
                        buffer,
                    })
                } else {
                    let size =
                        small_size.map_or("unknown size".to_string(), |size| size.to_string());
                    debug!("Fetching disk-based wheel from registry: {dist} ({size})");

                    // Create a directory for the wheel.
                    // TODO(konstin): Change this when the built wheel naming scheme is fixed.
                    let wheel_dir = self
                        .cache
                        .bucket(CacheBucket::Archives)
                        .join(wheel.package_id());
                    fs::create_dir_all(&wheel_dir).await?;

                    // Download the wheel to a temporary file.
                    let wheel_filename = &wheel.file.filename;
                    let wheel_file = wheel_dir.join(wheel_filename);
                    let mut writer = tokio::fs::File::create(&wheel_file).await?;
                    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                    LocalWheel::Disk(DiskWheel {
                        dist: dist.clone(),
                        filename,
                        path: wheel_file,
                    })
                };

                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_download_progress(&Download::Wheel(local_wheel.clone()));
                }

                Ok(local_wheel)
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                debug!("Fetching disk-based wheel from URL: {}", &wheel.url);

                // Create a directory for the wheel.
                // TODO(konstin): Change this when the built wheel naming scheme is fixed.
                let wheel_dir = self
                    .cache
                    .bucket(CacheBucket::Archives)
                    .join(wheel.package_id());
                fs::create_dir_all(&wheel_dir).await?;

                // Fetch the wheel.
                let reader = self.client.stream_external(&wheel.url).await?;

                // Download the wheel to the directory.
                let wheel_filename = wheel.filename()?;
                let wheel_file = wheel_dir.join(wheel_filename);
                let mut writer = tokio::fs::File::create(&wheel_file).await?;
                tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                let local_wheel = LocalWheel::Disk(DiskWheel {
                    dist: dist.clone(),
                    filename: wheel.filename.clone(),
                    path: wheel_file,
                });

                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_download_progress(&Download::Wheel(local_wheel.clone()));
                }

                Ok(local_wheel)
            }

            Dist::Built(BuiltDist::Path(wheel)) => Ok(LocalWheel::Disk(DiskWheel {
                dist: dist.clone(),
                path: wheel.path.clone(),
                filename: wheel.filename.clone(),
            })),

            Dist::Source(source_dist) => {
                let lock = self.locks.acquire(&dist).await;
                let _guard = lock.lock().await;

                let built_wheel = self.builder.download_and_build(source_dist).await?;
                Ok(LocalWheel::Built(BuiltWheel {
                    dist: dist.clone(),
                    filename: built_wheel.filename,
                    path: built_wheel.path,
                }))
            }
        }
    }

    /// Either fetch the only wheel metadata (directly from the index or with range requests) or
    /// fetch and build the source distribution.
    ///
    /// Returns the [`Metadata21`], along with a "precise" URL for the source distribution, if
    /// possible. For example, given a Git dependency with a reference to a branch or tag, return a
    /// URL with a precise reference to the current commit of that branch or tag.
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
    ) -> Result<(Metadata21, Option<Url>), DistributionDatabaseError> {
        match dist {
            Dist::Built(built_dist) => Ok((self.client.wheel_metadata(built_dist).await?, None)),
            Dist::Source(source_dist) => {
                // Optimization: Skip source dist download when we must not build them anyway
                if self.build_context.no_build() {
                    return Err(DistributionDatabaseError::NoBuild);
                }

                let lock = self.locks.acquire(dist).await;
                let _guard = lock.lock().await;

                // Insert the `precise` URL, if it exists.
                let precise = self.precise(source_dist).await?;
                let source_dist = match precise.as_ref() {
                    Some(url) => Cow::Owned(source_dist.clone().with_url(url.clone())),
                    None => Cow::Borrowed(source_dist),
                };

                let built_wheel = self.builder.download_and_build(&source_dist).await?;
                Ok((built_wheel.metadata, precise))
            }
        }
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    pub async fn precise(
        &self,
        dist: &SourceDist,
    ) -> Result<Option<Url>, DistributionDatabaseError> {
        let SourceDist::Git(source_dist) = dist else {
            return Ok(None);
        };
        let git_dir = self.build_context.cache().bucket(CacheBucket::Git);

        let DirectGitUrl { url, subdirectory } =
            DirectGitUrl::try_from(&source_dist.url).map_err(DistributionDatabaseError::Git)?;

        // If the commit already contains a complete SHA, short-circuit.
        if url.precise().is_some() {
            return Ok(None);
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let source = if let Some(reporter) = self.reporter.clone() {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter))
        } else {
            GitSource::new(url, git_dir)
        };
        let precise = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(DistributionDatabaseError::Git)?;
        let url = precise.into_git();

        // Re-encode as a URL.
        Ok(Some(DirectGitUrl { url, subdirectory }.into()))
    }
}
