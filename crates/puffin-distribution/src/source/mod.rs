//! Fetch and build source distributions from remote sources.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use fs_err::tokio as fs;
use futures::TryStreamExt;
use reqwest::Response;
use tempfile::TempDir;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, warn, Instrument};
use url::Url;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::{
    DirectArchiveUrl, DirectGitUrl, Dist, GitSourceDist, LocalEditable, Name, PathSourceDist,
    RemoteSource, SourceDist,
};
use install_wheel_rs::read_dist_info;
use platform_tags::Tags;
use puffin_cache::{CacheBucket, CacheEntry, CacheShard, CachedByTimestamp, WheelCache};
use puffin_client::{CachedClient, CachedClientError, DataWithCachePolicy};
use puffin_fs::{write_atomic, LockedFile};
use puffin_git::{Fetch, GitSource};
use puffin_traits::{BuildContext, BuildKind, SourceBuildTrait};
use pypi_types::Metadata21;

use crate::reporter::Facade;
use crate::source::built_wheel_metadata::BuiltWheelMetadata;
pub use crate::source::error::SourceDistError;
use crate::source::manifest::{DiskFilenameAndMetadata, Manifest};
use crate::Reporter;

mod built_wheel_metadata;
mod error;
mod manifest;

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
                let url = registry_source_dist
                    .base
                    .join_relative(&registry_source_dist.file.url)
                    .map_err(|err| {
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
        let cache_entry = cache_shard.entry(METADATA);

        let download_and_build = |response| {
            async {
                // At this point, we're seeing a new or updated source distribution; delete all
                // wheels, and rebuild.
                match fs::remove_dir_all(&cache_entry.dir()).await {
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
                let source_dist_entry = cache_shard.entry(filename);
                let cache_dir = self
                    .persist_source_dist_url(response, source_dist, filename, &source_dist_entry)
                    .await?;

                // Build the source distribution.
                let (disk_filename, wheel_filename, metadata) = self
                    .build_source_dist(source_dist, cache_dir, subdirectory, &cache_entry)
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
            }
            .instrument(info_span!("download_and_build", source_dist = %source_dist))
        };
        let req = self.cached_client.uncached().get(url.clone()).build()?;
        let manifest = self
            .cached_client
            .get_cached_with_callback(req, &cache_entry, download_and_build)
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

        let source_dist_entry = cache_shard.entry(filename);
        let cache_dir = self
            .persist_source_dist_url(response, source_dist, filename, &source_dist_entry)
            .await?;

        // Build the source distribution.
        let (disk_filename, wheel_filename, metadata) = self
            .build_source_dist(source_dist, cache_dir, subdirectory, &cache_entry)
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
            METADATA,
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

        let path = cache_entry.dir().join(&disk_filename);
        let target = cache_entry.dir().join(filename.stem());

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
            METADATA,
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

        let path = cache_entry.dir().join(&disk_filename);
        let target = cache_entry.dir().join(filename.stem());

        Ok(BuiltWheelMetadata {
            path,
            target,
            filename,
            metadata,
        })
    }

    /// Download and unzip a source distribution into the cache from an HTTP response.
    async fn persist_source_dist_url<'data>(
        &self,
        response: Response,
        source_dist: &SourceDist,
        filename: &str,
        cache_entry: &'data CacheEntry,
    ) -> Result<&'data Path, SourceDistError> {
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
        fs::create_dir_all(&cache_entry.dir()).await?;
        if let Err(err) = fs_err::rename(&source_dist_dir, cache_path) {
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
        let lock_dir = git_dir.join("locks");
        fs::create_dir_all(&lock_dir).await?;
        let canonical_url = cache_key::CanonicalUrl::new(url);
        let _lock = LockedFile::acquire(
            lock_dir.join(cache_key::digest(&canonical_url)),
            &canonical_url,
        )?;

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
    #[instrument(skip_all, fields(dist = %dist))]
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
        fs::create_dir_all(&cache_entry.dir()).await?;
        let disk_filename = self
            .build_context
            .setup_build(
                source_dist,
                subdirectory,
                &dist.to_string(),
                BuildKind::Wheel,
            )
            .await
            .map_err(|err| SourceDistError::Build(dist.to_string(), err))?
            .wheel(cache_entry.dir())
            .await
            .map_err(|err| SourceDistError::Build(dist.to_string(), err))?;

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_metadata(&filename, cache_entry.dir().join(&disk_filename))?;

        debug!("Finished building: {dist}");
        Ok((disk_filename, filename, metadata))
    }

    /// Build a single directory into an editable wheel
    pub async fn build_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(Dist, String, WheelFilename, Metadata21), SourceDistError> {
        debug!("Building (editable) {editable}");
        let disk_filename = self
            .build_context
            .setup_build(
                &editable.path,
                None,
                &editable.to_string(),
                BuildKind::Editable,
            )
            .await
            .map_err(|err| SourceDistError::Build(editable.to_string(), err))?
            .wheel(editable_wheel_dir)
            .await
            .map_err(|err| SourceDistError::Build(editable.to_string(), err))?;
        let filename = WheelFilename::from_str(&disk_filename)?;
        // We finally have the name of the package and can construct the dist
        let dist = Dist::Source(SourceDist::Path(PathSourceDist {
            name: filename.name.clone(),
            url: editable.url().clone(),
            path: editable.path.clone(),
            editable: true,
        }));
        let metadata = read_metadata(&filename, editable_wheel_dir.join(&disk_filename))?;

        debug!("Finished building (editable): {dist}");
        Ok((dist, disk_filename, filename, metadata))
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
                    if let Err(err) = fs::remove_dir_all(&cache_entry.dir()).await {
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
