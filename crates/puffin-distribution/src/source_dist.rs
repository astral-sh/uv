//! Fetch and build source distributions from remote sources.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use fs_err::tokio as fs;
use futures::TryStreamExt;
use reqwest::Response;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, warn};
use url::Url;
use zip::result::ZipError;
use zip::ZipArchive;

use distribution_filename::{WheelFilename, WheelFilenameError};
use distribution_types::direct_url::{DirectArchiveUrl, DirectGitUrl};
use distribution_types::{GitSourceDist, Metadata, PathSourceDist, RemoteSource, SourceDist};
use install_wheel_rs::read_dist_info;
use platform_tags::Tags;
use puffin_cache::{
    digest, CacheBucket, CacheEntry, CacheShard, CachedByTimestamp, CanonicalUrl, WheelCache,
};
use puffin_client::{CachedClient, CachedClientError, DataWithCachePolicy};
use puffin_fs::write_atomic;
use puffin_git::{Fetch, GitSource};
use puffin_normalize::PackageName;
use puffin_traits::{BuildContext, SourceBuildTrait};
use pypi_types::Metadata21;

use crate::locks::LockedFile;
use crate::Reporter;

/// The caller is responsible for adding the source dist information to the error chain
#[derive(Debug, Error)]
pub enum SourceDistError {
    #[error("Building source distributions is disabled")]
    NoBuild,

    // Network error
    #[error("Failed to parse URL: `{0}`")]
    UrlParse(String, #[source] url::ParseError),
    #[error("Git operation failed")]
    Git(#[source] anyhow::Error),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error(transparent)]
    Client(#[from] puffin_client::Error),

    // Cache writing error
    #[error("Failed to write to source dist cache")]
    Io(#[from] std::io::Error),
    #[error("Cache deserialization failed")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("Cache serialization failed")]
    Encode(#[from] rmp_serde::encode::Error),

    // Build error
    #[error("Failed to build: {0}")]
    Build(Box<SourceDist>, #[source] anyhow::Error),
    #[error("Built wheel has an invalid filename")]
    WheelFilename(#[from] WheelFilenameError),
    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },
    #[error("Failed to parse metadata from built wheel")]
    Metadata(#[from] pypi_types::Error),
    #[error("Failed to read `dist-info` metadata from built wheel")]
    DistInfo(#[from] install_wheel_rs::Error),
    #[error("Failed to read zip archive from built wheel")]
    Zip(#[from] ZipError),
    #[error("Source distribution directory contains neither readable pyproject.toml nor setup.py")]
    DirWithoutEntrypoint,
    #[error("Failed to extract source distribution: {0}")]
    Extract(#[from] puffin_extract::Error),

    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskFilenameAndMetadata {
    /// Relative, un-normalized wheel filename in the cache, which can be different than
    /// `WheelFilename::to_string`.
    disk_filename: String,
    metadata: Metadata21,
}

/// The information about the wheel we either just built or got from the cache.
#[derive(Debug, Clone)]
pub struct BuiltWheelMetadata {
    /// The path to the built wheel.
    pub path: PathBuf,
    /// The expected path to the downloaded wheel's entry in the cache.
    pub target: PathBuf,
    /// The parsed filename.
    pub filename: WheelFilename,
    /// The metadata of the built wheel.
    pub metadata: Metadata21,
}

impl BuiltWheelMetadata {
    /// Find a compatible wheel in the cache based on the given manifest.
    fn find_in_cache(tags: &Tags, manifest: &Manifest, cache_entry: &CacheEntry) -> Option<Self> {
        // Find a compatible cache entry in the manifest.
        let (filename, cached_dist) = manifest.find_compatible(tags)?;
        let metadata = Self::from_cached(filename.clone(), cached_dist.clone(), cache_entry);

        // Validate that the wheel exists on disk.
        if !metadata.path.is_file() {
            warn!(
                "Wheel `{}` is present in the manifest, but not on disk",
                metadata.path.display()
            );
            return None;
        }

        Some(metadata)
    }

    /// Create a [`BuiltWheelMetadata`] from a cached entry.
    fn from_cached(
        filename: WheelFilename,
        cached_dist: DiskFilenameAndMetadata,
        cache_entry: &CacheEntry,
    ) -> Self {
        Self {
            path: cache_entry.dir.join(&cached_dist.disk_filename),
            target: cache_entry.dir.join(filename.stem()),
            filename,
            metadata: cached_dist.metadata,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Manifest(FxHashMap<WheelFilename, DiskFilenameAndMetadata>);

impl Manifest {
    /// Initialize a [`Manifest`] from an iterator over entries.
    fn from_iter(iter: impl IntoIterator<Item = (WheelFilename, DiskFilenameAndMetadata)>) -> Self {
        Self(iter.into_iter().collect())
    }

    /// Find a compatible wheel in the cache.
    fn find_compatible(&self, tags: &Tags) -> Option<(&WheelFilename, &DiskFilenameAndMetadata)> {
        self.0
            .iter()
            .find(|(filename, _metadata)| filename.is_compatible(tags))
    }
}

impl std::ops::Deref for Manifest {
    type Target = FxHashMap<WheelFilename, DiskFilenameAndMetadata>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Manifest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub struct SourceDistCachedBuilder<'a, T: BuildContext> {
    build_context: &'a T,
    cached_client: &'a CachedClient,
    reporter: Option<Arc<dyn Reporter>>,
    tags: &'a Tags,
}

/// The name of the file that contains the cached metadata, encoded via `MsgPack`.
const METADATA: &str = "metadata.msgpack";

impl<'a, T: BuildContext> SourceDistCachedBuilder<'a, T> {
    /// Initialize a [`SourceDistCachedBuilder`] from a [`BuildContext`].
    pub fn new(build_context: &'a T, cached_client: &'a CachedClient, tags: &'a Tags) -> Self {
        Self {
            build_context,
            reporter: None,
            cached_client,
            tags,
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    pub async fn download_and_build(
        &self,
        source_dist: &SourceDist,
    ) -> Result<BuiltWheelMetadata, SourceDistError> {
        let built_wheel_metadata = match &source_dist {
            SourceDist::DirectUrl(direct_url_source_dist) => {
                let filename = direct_url_source_dist
                    .filename()
                    .expect("Distribution must have a filename");
                let DirectArchiveUrl { url, subdirectory } =
                    DirectArchiveUrl::from(direct_url_source_dist.url.raw());

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Url(&url).remote_wheel_dir(direct_url_source_dist.name().as_ref()),
                );

                self.url(
                    source_dist,
                    filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                )
                .await?
            }
            SourceDist::Registry(registry_source_dist) => {
                let url = Url::parse(&registry_source_dist.file.url).map_err(|err| {
                    SourceDistError::UrlParse(registry_source_dist.file.url.clone(), err)
                })?;

                // For registry source distributions, shard by package, then by SHA.
                // Ex) `pypi/requests/a673187abc19fe6c`
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Index(&registry_source_dist.index)
                        .remote_wheel_dir(registry_source_dist.name.as_ref())
                        .join(&registry_source_dist.file.hashes.sha256[..16]),
                );

                self.url(
                    source_dist,
                    &registry_source_dist.file.filename,
                    &url,
                    &cache_shard,
                    None,
                )
                .await?
            }
            SourceDist::Git(git_source_dist) => self.git(source_dist, git_source_dist).await?,
            SourceDist::Path(path_source_dist) => self.path(source_dist, path_source_dist).await?,
        };

        Ok(built_wheel_metadata)
    }

    /// Build a source distribution from a remote URL.
    #[allow(clippy::too_many_arguments)]
    async fn url<'data>(
        &self,
        source_dist: &'data SourceDist,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
    ) -> Result<BuiltWheelMetadata, SourceDistError> {
        let cache_entry = cache_shard.entry(METADATA.to_string());

        let response_callback = |response| async {
            // At this point, we're seeing a new or updated source distribution; delete all
            // wheels, and rebuild.
            match fs::remove_dir_all(&cache_entry.dir).await {
                Ok(()) => debug!("Cleared built wheels and metadata for {source_dist}"),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
                Err(err) => return Err(err.into()),
            }

            debug!("Downloading and building source distribution: {source_dist}");
            let task = self
                .reporter
                .as_ref()
                .map(|reporter| reporter.on_build_start(source_dist));

            // Download the source distribution.
            let cache_dir = self
                .persist_source_dist_url(response, source_dist, filename, cache_shard)
                .await?;

            // Build the source distribution.
            let (disk_filename, wheel_filename, metadata) = self
                .build_source_dist(source_dist, &cache_dir, subdirectory, &cache_entry)
                .await?;

            if let Some(task) = task {
                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_build_complete(source_dist, task);
                }
            }

            Ok(Manifest::from_iter([(
                wheel_filename,
                DiskFilenameAndMetadata {
                    disk_filename,
                    metadata,
                },
            )]))
        };
        let req = self.cached_client.uncached().get(url.clone()).build()?;
        let manifest = self
            .cached_client
            .get_cached_with_callback(req, &cache_entry, response_callback)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => SourceDistError::Client(err),
            })?;

        // If the cache contains a compatible wheel, return it.
        if let Some(metadata) =
            BuiltWheelMetadata::find_in_cache(self.tags, &manifest, &cache_entry)
        {
            return Ok(metadata);
        }

        // At this point, we're seeing cached metadata (as in, we have an up-to-date source
        // distribution), but the wheel(s) we built previously are incompatible.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        // Start by downloading the source distribution.
        let response = self
            .cached_client
            .uncached()
            .get(url.clone())
            .send()
            .await
            .map_err(puffin_client::Error::RequestMiddlewareError)?;
        let cache_dir = self
            .persist_source_dist_url(response, source_dist, filename, cache_shard)
            .await?;

        // Build the source distribution.
        let (disk_filename, wheel_filename, metadata) = self
            .build_source_dist(source_dist, &cache_dir, subdirectory, &cache_entry)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        let cached_data = DiskFilenameAndMetadata {
            disk_filename: disk_filename.clone(),
            metadata: metadata.clone(),
        };

        // Not elegant that we have to read again here, but also not too relevant given that we
        // have to build a source dist next.
        // Just return if the response wasn't cacheable or there was another errors that
        // `CachedClient` already complained about
        if let Ok(cached) = fs::read(cache_entry.path()).await {
            // If the file exists and it was just read or written by `CachedClient`, we assume it must
            // be correct.
            let mut cached = rmp_serde::from_slice::<DataWithCachePolicy<Manifest>>(&cached)?;

            cached
                .data
                .insert(wheel_filename.clone(), cached_data.clone());
            write_atomic(cache_entry.path(), rmp_serde::to_vec(&cached)?).await?;
        };

        Ok(BuiltWheelMetadata::from_cached(
            wheel_filename,
            cached_data,
            &cache_entry,
        ))
    }

    /// Build a source distribution from a local path.
    async fn path(
        &self,
        source_dist: &SourceDist,
        path_source_dist: &PathSourceDist,
    ) -> Result<BuiltWheelMetadata, SourceDistError> {
        let cache_entry = self.build_context.cache().entry(
            CacheBucket::BuiltWheels,
            WheelCache::Path(&path_source_dist.url)
                .remote_wheel_dir(path_source_dist.name().as_ref()),
            METADATA.to_string(),
        );

        // Determine the last-modified time of the source distribution.
        let file_metadata = fs_err::metadata(&path_source_dist.path)?;
        let modified = if file_metadata.is_file() {
            // `modified()` is infallible on windows and unix (i.e., all platforms we support).
            file_metadata.modified()?
        } else {
            if let Some(metadata) = path_source_dist
                .path
                .join("pyproject.toml")
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
            {
                metadata.modified()?
            } else if let Some(metadata) = path_source_dist
                .path
                .join("setup.py")
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
            {
                metadata.modified()?
            } else {
                return Err(SourceDistError::DirWithoutEntrypoint);
            }
        };

        // Read the existing metadata from the cache.
        let mut manifest = Self::read_fresh_metadata(&cache_entry, modified)
            .await?
            .unwrap_or_default();

        // If the cache contains a compatible wheel, return it.
        if let Some(metadata) =
            BuiltWheelMetadata::find_in_cache(self.tags, &manifest, &cache_entry)
        {
            return Ok(metadata);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        let (disk_filename, filename, metadata) = self
            .build_source_dist(source_dist, &path_source_dist.path, None, &cache_entry)
            .await?;

        if metadata.name != path_source_dist.name {
            return Err(SourceDistError::NameMismatch {
                metadata: metadata.name,
                given: path_source_dist.name.clone(),
            });
        }

        // Store the metadata for this build along with all the other builds.
        manifest.insert(
            filename.clone(),
            DiskFilenameAndMetadata {
                disk_filename: disk_filename.clone(),
                metadata: metadata.clone(),
            },
        );
        let cached = CachedByTimestamp {
            timestamp: modified,
            data: manifest,
        };
        let data = rmp_serde::to_vec(&cached)?;
        write_atomic(cache_entry.path(), data).await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        let path = cache_entry.dir.join(&disk_filename);
        let target = cache_entry.dir.join(filename.stem());

        Ok(BuiltWheelMetadata {
            path,
            target,
            filename,
            metadata,
        })
    }

    /// Build a source distribution from a Git repository.
    async fn git(
        &self,
        source_dist: &SourceDist,
        git_source_dist: &GitSourceDist,
    ) -> Result<BuiltWheelMetadata, SourceDistError> {
        let (fetch, subdirectory) = self.download_source_dist_git(&git_source_dist.url).await?;

        // TODO(konstin): Do we want to delete old built wheels when the git sha changed?
        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_entry = self.build_context.cache().entry(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&git_source_dist.url, &git_sha.to_short_string())
                .remote_wheel_dir(git_source_dist.name().as_ref()),
            METADATA.to_string(),
        );

        // Read the existing metadata from the cache.
        let mut manifest = Self::read_metadata(&cache_entry).await?.unwrap_or_default();

        // If the cache contains a compatible wheel, return it.
        if let Some(metadata) =
            BuiltWheelMetadata::find_in_cache(self.tags, &manifest, &cache_entry)
        {
            return Ok(metadata);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        let (disk_filename, filename, metadata) = self
            .build_source_dist(
                source_dist,
                fetch.path(),
                subdirectory.as_deref(),
                &cache_entry,
            )
            .await?;

        if metadata.name != git_source_dist.name {
            return Err(SourceDistError::NameMismatch {
                metadata: metadata.name,
                given: git_source_dist.name.clone(),
            });
        }

        // Store the metadata for this build along with all the other builds.
        manifest.insert(
            filename.clone(),
            DiskFilenameAndMetadata {
                disk_filename: disk_filename.clone(),
                metadata: metadata.clone(),
            },
        );
        let data = rmp_serde::to_vec(&manifest)?;
        write_atomic(cache_entry.path(), data).await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        let path = cache_entry.dir.join(&disk_filename);
        let target = cache_entry.dir.join(filename.stem());

        Ok(BuiltWheelMetadata {
            path,
            target,
            filename,
            metadata,
        })
    }

    /// Download and unzip a source distribution into the cache from an HTTP response.
    async fn persist_source_dist_url(
        &self,
        response: Response,
        source_dist: &SourceDist,
        filename: &str,
        cache_shard: &CacheShard,
    ) -> Result<PathBuf, SourceDistError> {
        let cache_entry = cache_shard.entry(filename);
        let cache_path = cache_entry.path();
        if cache_path.is_dir() {
            debug!("Distribution is already cached: {source_dist}");
            return Ok(cache_path);
        }

        // Download the source distribution to a temporary file.
        let span =
            info_span!("download_source_dist", filename = filename, source_dist = %source_dist);
        let (temp_dir, source_dist_archive) =
            self.download_source_dist_url(response, filename).await?;
        drop(span);

        // Unzip the source distribution to a temporary directory.
        let span =
            info_span!("extract_source_dist", filename = filename, source_dist = %source_dist);
        let source_dist_dir = puffin_extract::extract_source(
            &source_dist_archive,
            temp_dir.path().join("extracted"),
        )?;
        drop(span);

        // Persist the unzipped distribution to the cache.
        fs::create_dir_all(&cache_entry.dir).await?;
        if let Err(err) = fs_err::rename(&source_dist_dir, &cache_path) {
            // If another thread already cached the distribution, we can ignore the error.
            if cache_path.is_dir() {
                warn!("Downloaded already-cached distribution: {source_dist}");
            } else {
                return Err(err.into());
            };
        }

        Ok(cache_path)
    }

    /// Download a source distribution from a URL to a temporary file.
    async fn download_source_dist_url(
        &self,
        response: Response,
        source_dist_filename: &str,
    ) -> Result<(TempDir, PathBuf), puffin_client::Error> {
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();
        let mut reader = tokio::io::BufReader::new(reader.compat());

        // Create a temporary directory.
        let cache_dir = self.build_context.cache().bucket(CacheBucket::BuiltWheels);
        fs::create_dir_all(&cache_dir)
            .await
            .map_err(puffin_client::Error::CacheWrite)?;
        let temp_dir = tempfile::tempdir_in(cache_dir).map_err(puffin_client::Error::CacheWrite)?;

        // Download the source distribution to a temporary file.
        let sdist_file = temp_dir.path().join(source_dist_filename);
        let mut writer = tokio::io::BufWriter::new(
            tokio::fs::File::create(&sdist_file)
                .await
                .map_err(puffin_client::Error::CacheWrite)?,
        );
        tokio::io::copy(&mut reader, &mut writer)
            .await
            .map_err(puffin_client::Error::CacheWrite)?;

        Ok((temp_dir, sdist_file))
    }

    /// Download a source distribution from a Git repository.
    async fn download_source_dist_git(
        &self,
        url: &Url,
    ) -> Result<(Fetch, Option<PathBuf>), SourceDistError> {
        debug!("Fetching source distribution from Git: {url}");
        let git_dir = self.build_context.cache().bucket(CacheBucket::Git);

        // Avoid races between different processes, too.
        let locks_dir = git_dir.join("locks");
        fs::create_dir_all(&locks_dir).await?;
        let _lockfile = LockedFile::new(locks_dir.join(digest(&CanonicalUrl::new(url))))?;

        let DirectGitUrl { url, subdirectory } =
            DirectGitUrl::try_from(url).map_err(SourceDistError::Git)?;

        let source = if let Some(reporter) = &self.reporter {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter.clone()))
        } else {
            GitSource::new(url, git_dir)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(SourceDistError::Git)?;
        Ok((fetch, subdirectory))
    }

    /// Build a source distribution, storing the built wheel in the cache.
    ///
    /// Returns the un-normalized disk filename, the parsed, normalized filename and the metadata
    async fn build_source_dist(
        &self,
        dist: &SourceDist,
        source_dist: &Path,
        subdirectory: Option<&Path>,
        cache_entry: &CacheEntry,
    ) -> Result<(String, WheelFilename, Metadata21), SourceDistError> {
        debug!("Building: {dist}");

        if self.build_context.no_build() {
            return Err(SourceDistError::NoBuild);
        }

        // Build the wheel.
        fs::create_dir_all(&cache_entry.dir).await?;
        let disk_filename = self
            .build_context
            .setup_build(source_dist, subdirectory, &dist.to_string())
            .await
            .map_err(|err| SourceDistError::Build(Box::new(dist.clone()), err))?
            .wheel(&cache_entry.dir)
            .await
            .map_err(|err| SourceDistError::Build(Box::new(dist.clone()), err))?;

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_metadata(&filename, cache_entry.dir.join(&disk_filename))?;

        debug!("Finished building: {dist}");
        Ok((disk_filename, filename, metadata))
    }

    /// Read an existing cache entry, if it exists and is up-to-date.
    async fn read_fresh_metadata(
        cache_entry: &CacheEntry,
        modified: std::time::SystemTime,
    ) -> Result<Option<Manifest>, SourceDistError> {
        match fs::read(&cache_entry.path()).await {
            Ok(cached) => {
                let cached = rmp_serde::from_slice::<CachedByTimestamp<Manifest>>(&cached)?;
                if cached.timestamp == modified {
                    Ok(Some(cached.data))
                } else {
                    debug!(
                        "Removing stale built wheels for: {}",
                        cache_entry.path().display()
                    );
                    if let Err(err) = fs::remove_dir_all(&cache_entry.dir).await {
                        warn!("Failed to remove stale built wheel cache directory: {err}");
                    }
                    Ok(None)
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Read an existing cache entry, if it exists.
    async fn read_metadata(cache_entry: &CacheEntry) -> Result<Option<Manifest>, SourceDistError> {
        match fs::read(&cache_entry.path()).await {
            Ok(cached) => Ok(Some(rmp_serde::from_slice::<Manifest>(&cached)?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

/// Read the [`Metadata21`] from a built wheel.
fn read_metadata(
    filename: &WheelFilename,
    wheel: impl Into<PathBuf>,
) -> Result<Metadata21, SourceDistError> {
    let mut archive = ZipArchive::new(fs_err::File::open(wheel)?)?;
    let dist_info = read_dist_info(filename, &mut archive)?;
    Ok(Metadata21::parse(&dist_info)?)
}

trait SourceDistReporter: Send + Sync {
    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to [`puffin_git::Reporter`].
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
