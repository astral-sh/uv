use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use bytesize::ByteSize;
use fs_err::tokio as fs;
use puffin_extract::unzip_no_seek;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, instrument};
use url::Url;

use distribution_filename::{WheelFilename, WheelFilenameError};
use distribution_types::{BuiltDist, DirectGitUrl, Dist, LocalEditable, Name, SourceDist};
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_client::RegistryClient;
use puffin_git::GitSource;
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::download::{BuiltWheel, UnzippedWheel};
use crate::locks::Locks;
use crate::reporter::Facade;
use crate::{
    DiskWheel, InMemoryWheel, LocalWheel, Reporter, SourceDistCachedBuilder, SourceDistError,
};

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
    #[instrument(skip(self))]
    pub async fn get_or_build_wheel(
        &self,
        dist: Dist,
    ) -> Result<LocalWheel, DistributionDatabaseError> {
        match &dist {
            Dist::Built(BuiltDist::Registry(wheel)) => {
                let url = wheel
                    .base
                    .join_relative(&wheel.file.url)
                    .map_err(|err| DistributionDatabaseError::Url(wheel.file.url.clone(), err))?;

                // Make cache entry
                let wheel_filename = WheelFilename::from_str(&wheel.file.filename)?;
                let cache_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).remote_wheel_dir(wheel_filename.name.as_ref()),
                    wheel_filename.stem(),
                );

                // Start the download
                let reader = self.client.stream_external(&url).await?;

                // In all wheels we've seen so far, unzipping while downloading is the
                // faster option.
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
                let unzip_while_downloading = true;
                if unzip_while_downloading {
                    // Download and unzip to a temporary dir
                    let temp_dir = tempfile::tempdir_in(self.cache.root())?;
                    let temp_target = temp_dir.path().join(&wheel.file.filename);
                    unzip_no_seek(reader.compat(), &temp_target).await?;

                    // Move the dir to the right place
                    fs::create_dir_all(&cache_entry.dir()).await?;
                    let target = cache_entry.into_path_buf();
                    tokio::fs::rename(temp_target, &target).await?;

                    return Ok(LocalWheel::Unzipped(UnzippedWheel {
                        dist: dist.clone(),
                        target,
                        filename: wheel_filename,
                    }));
                }

                // If the file is greater than 5MB, write it to disk; otherwise, keep it in memory.
                //
                // TODO this is currently dead code. Consider deleting if there's no use for it.
                let byte_size = wheel.file.size.map(ByteSize::b);
                let local_wheel = if let Some(byte_size) =
                    byte_size.filter(|byte_size| *byte_size < ByteSize::mb(5))
                {
                    debug!("Fetching in-memory wheel from registry: {dist} ({byte_size})",);

                    // Read into a buffer.
                    let mut buffer = Vec::with_capacity(
                        wheel
                            .file
                            .size
                            .unwrap_or(0)
                            .try_into()
                            .expect("5MB shouldn't be bigger usize::MAX"),
                    );
                    let mut reader = tokio::io::BufReader::new(reader.compat());
                    tokio::io::copy(&mut reader, &mut buffer).await?;

                    LocalWheel::InMemory(InMemoryWheel {
                        dist: dist.clone(),
                        target: cache_entry.into_path_buf(),
                        buffer,
                        filename: wheel_filename,
                    })
                } else {
                    let size =
                        byte_size.map_or("unknown size".to_string(), |size| size.to_string());

                    debug!("Fetching disk-based wheel from registry: {dist} ({size})");

                    let filename = wheel_filename.to_string();

                    // Download the wheel to a temporary file.
                    let temp_dir = tempfile::tempdir_in(self.cache.root())?;
                    let temp_file = temp_dir.path().join(&filename);
                    let mut writer =
                        tokio::io::BufWriter::new(tokio::fs::File::create(&temp_file).await?);
                    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

                    // Move the temporary file to the cache.
                    let cache_entry = self.cache.entry(
                        CacheBucket::Wheels,
                        WheelCache::Index(&wheel.index)
                            .remote_wheel_dir(wheel_filename.name.as_ref()),
                        filename, // TODO should this be filename.stem() to match the other branch?
                    );
                    fs::create_dir_all(&cache_entry.dir()).await?;
                    tokio::fs::rename(temp_file, &cache_entry.path()).await?;

                    LocalWheel::Disk(DiskWheel {
                        dist: dist.clone(),
                        target: cache_entry
                            .with_file(wheel_filename.stem())
                            .path()
                            .to_path_buf(),
                        path: cache_entry.into_path_buf(),
                        filename: wheel_filename,
                    })
                };

                Ok(local_wheel)
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                debug!("Fetching disk-based wheel from URL: {}", wheel.url);

                let reader = self.client.stream_external(&wheel.url).await?;

                // Download and unzip the wheel to a temporary dir.
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
                tokio::fs::rename(temp_target, &target).await?;

                let local_wheel = LocalWheel::Unzipped(UnzippedWheel {
                    dist: dist.clone(),
                    target,
                    filename: wheel.filename.clone(),
                });

                Ok(local_wheel)
            }

            Dist::Built(BuiltDist::Path(wheel)) => {
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

                let built_wheel = self.builder.download_and_build(source_dist).await?;
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
