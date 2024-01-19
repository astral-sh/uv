use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use fs_err::tokio as fs;
use futures::FutureExt;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::instrument;
use url::Url;

use distribution_filename::{WheelFilename, WheelFilenameError};
use distribution_types::{
    BuiltDist, DirectGitUrl, Dist, FileLocation, LocalEditable, Name, SourceDist,
};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_client::RegistryClient;
use puffin_extract::unzip_no_seek;
use puffin_git::GitSource;
use puffin_traits::{BuildContext, NoBinary};
use pypi_types::Metadata21;

use crate::download::{BuiltWheel, UnzippedWheel};
use crate::locks::Locks;
use crate::reporter::Facade;
use crate::{DiskWheel, LocalWheel, Reporter, SourceDistCachedBuilder, SourceDistError};

#[derive(Debug, Error)]
pub enum DistributionDatabaseError {
    #[error("Failed to parse URL: {0}")]
    Url(String, #[source] url::ParseError),
    #[error(transparent)]
    WheelFilename(#[from] WheelFilenameError),
    #[error(transparent)]
    Client(#[from] puffin_client::Error),
    #[error(transparent)]
    Extract(#[from] puffin_extract::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Distribution(#[from] distribution_types::Error),
    #[error(transparent)]
    SourceBuild(#[from] SourceDistError),
    #[error("Git operation failed")]
    Git(#[source] anyhow::Error),
    /// Should not occur, i've only seen it when another task panicked
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Building source distributions is disabled")]
    NoBuild,
    #[error("Using pre-built wheels is disabled")]
    NoBinary,
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

    /// Either fetch the wheel or fetch and build the source distribution
    ///
    /// If `no_remote_wheel` is set, the wheel will be built from a source distribution
    /// even if compatible pre-built wheels are available.
    #[instrument(skip(self))]
    pub async fn get_or_build_wheel(
        &self,
        dist: Dist,
    ) -> Result<LocalWheel, DistributionDatabaseError> {
        let no_binary = match self.build_context.no_binary() {
            NoBinary::None => false,
            NoBinary::All => true,
            NoBinary::Packages(packages) => packages.contains(dist.name()),
        };
        match &dist {
            Dist::Built(BuiltDist::Registry(wheel)) => {
                if no_binary {
                    return Err(DistributionDatabaseError::NoBinary);
                }

                let url = match &wheel.file.url {
                    FileLocation::RelativeUrl(base, url) => base
                        .join_relative(url)
                        .map_err(|err| DistributionDatabaseError::Url(url.clone(), err))?,
                    FileLocation::AbsoluteUrl(url) => Url::parse(url)
                        .map_err(|err| DistributionDatabaseError::Url(url.clone(), err))?,
                    FileLocation::Path(path) => {
                        let url = Url::from_file_path(path).expect("path is absolute");
                        let cache_entry = self.cache.entry(
                            CacheBucket::Wheels,
                            WheelCache::Url(&url).remote_wheel_dir(wheel.name().as_ref()),
                            wheel.filename.stem(),
                        );

                        return Ok(LocalWheel::Disk(DiskWheel {
                            dist: dist.clone(),
                            path: path.clone(),
                            target: cache_entry.into_path_buf(),
                            filename: wheel.filename.clone(),
                        }));
                    }
                };

                // Download and unzip on the same tokio task.
                //
                // In all wheels we've seen so far, unzipping while downloading is
                // faster than downloading into a file and then unzipping on multiple
                // threads.
                //
                // Writing to a file first may be faster if the wheel takes longer to
                // unzip than it takes to download. This may happen if the wheel is a
                // zip bomb, or if the machine has a weak cpu (with many cores), but a
                // fast network.
                //
                // If we find such a case, it may make sense to create separate tasks
                // for downloading and unzipping (with a buffer in between) and switch
                // to rayon if this buffer grows large by the time the file is fully
                // downloaded.
                let reader = self.client.stream_external(&url).await?;

                // Download and unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.cache.root())?;
                let temp_target = temp_dir.path().join(&wheel.file.filename);
                unzip_no_seek(reader.compat(), &temp_target).await?;

                // Move the temporary file to the cache.
                let wheel_filename = WheelFilename::from_str(&wheel.file.filename)?;
                let cache_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).remote_wheel_dir(wheel_filename.name.as_ref()),
                    wheel_filename.stem(),
                );
                fs::create_dir_all(&cache_entry.dir()).await?;
                let target = cache_entry.into_path_buf();
                fs_err::tokio::rename(temp_target, &target).await?;

                Ok(LocalWheel::Unzipped(UnzippedWheel {
                    dist: dist.clone(),
                    target,
                    filename: wheel_filename,
                }))
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                if no_binary {
                    return Err(DistributionDatabaseError::NoBinary);
                }

                let reader = self.client.stream_external(&wheel.url).await?;

                // Download and unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.cache.root())?;
                let temp_target = temp_dir.path().join(wheel.filename.to_string());
                unzip_no_seek(reader.compat(), &temp_target).await?;

                // Move the temporary file to the cache.
                let cache_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );
                fs::create_dir_all(&cache_entry.dir()).await?;
                let target = cache_entry.into_path_buf();
                fs_err::tokio::rename(temp_target, &target).await?;

                let local_wheel = LocalWheel::Unzipped(UnzippedWheel {
                    dist: dist.clone(),
                    target,
                    filename: wheel.filename.clone(),
                });

                Ok(local_wheel)
            }

            Dist::Built(BuiltDist::Path(wheel)) => {
                if no_binary {
                    return Err(DistributionDatabaseError::NoBinary);
                }

                let cache_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                Ok(LocalWheel::Disk(DiskWheel {
                    dist: dist.clone(),
                    path: wheel.path.clone(),
                    target: cache_entry.into_path_buf(),
                    filename: wheel.filename.clone(),
                }))
            }

            Dist::Source(source_dist) => {
                let lock = self.locks.acquire(&dist).await;
                let _guard = lock.lock().await;

                let built_wheel = self.builder.download_and_build(source_dist).boxed().await?;
                Ok(LocalWheel::Built(BuiltWheel {
                    dist: dist.clone(),
                    path: built_wheel.path,
                    target: built_wheel.target,
                    filename: built_wheel.filename,
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
    #[instrument(skip(self))]
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
    ) -> Result<(Metadata21, Option<Url>), DistributionDatabaseError> {
        match dist {
            Dist::Built(built_dist) => {
                Ok((self.client.wheel_metadata(built_dist).boxed().await?, None))
            }
            Dist::Source(source_dist) => {
                // Optimization: Skip source dist download when we must not build them anyway.
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

                let metadata = self
                    .builder
                    .download_and_build_metadata(&source_dist)
                    .boxed()
                    .await?;
                Ok((metadata, precise))
            }
        }
    }

    /// Build a directory into an editable wheel.
    pub async fn build_wheel_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(LocalWheel, Metadata21), DistributionDatabaseError> {
        let (dist, disk_filename, filename, metadata) = self
            .builder
            .build_editable(editable, editable_wheel_dir)
            .await?;

        let built_wheel = BuiltWheel {
            dist,
            filename,
            path: editable_wheel_dir.join(disk_filename),
            target: editable_wheel_dir.join(cache_key::digest(&editable.path)),
        };
        Ok((LocalWheel::Built(built_wheel), metadata))
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    async fn precise(&self, dist: &SourceDist) -> Result<Option<Url>, DistributionDatabaseError> {
        let SourceDist::Git(source_dist) = dist else {
            return Ok(None);
        };
        let git_dir = self.build_context.cache().bucket(CacheBucket::Git);

        let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(source_dist.url.raw())
            .map_err(DistributionDatabaseError::Git)?;

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
        Ok(Some(Url::from(DirectGitUrl { url, subdirectory })))
    }
}
