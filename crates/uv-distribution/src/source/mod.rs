//! Fetch and build source distributions from remote sources.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use fs_err::tokio as fs;
use futures::{FutureExt, TryStreamExt};
use reqwest::Response;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, Instrument};
use url::Url;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::{
    DirectArchiveUrl, DirectGitUrl, Dist, FileLocation, GitSourceDist, LocalEditable, Name,
    PathSourceDist, RemoteSource, SourceDist,
};
use install_wheel_rs::read_dist_info;
use pep508_rs::VerbatimUrl;
use platform_tags::Tags;
use pypi_types::Metadata21;
use uv_cache::{
    ArchiveTimestamp, CacheBucket, CacheEntry, CacheShard, CachedByTimestamp, Freshness, WheelCache,
};
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_fs::{write_atomic, LockedFile};
use uv_git::{Fetch, GitSource};
use uv_traits::{BuildContext, BuildKind, NoBuild, SourceBuildTrait};

use crate::error::Error;
use crate::reporter::Facade;
use crate::source::built_wheel_metadata::BuiltWheelMetadata;
use crate::source::manifest::Manifest;
use crate::Reporter;

mod built_wheel_metadata;
mod manifest;

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub struct SourceDistCachedBuilder<'a, T: BuildContext> {
    build_context: &'a T,
    client: &'a RegistryClient,
    reporter: Option<Arc<dyn Reporter>>,
    tags: &'a Tags,
}

/// The name of the file that contains the cached manifest, encoded via `MsgPack`.
pub(crate) const MANIFEST: &str = "manifest.msgpack";

/// The name of the file that contains the cached distribution metadata, encoded via `MsgPack`.
pub(crate) const METADATA: &str = "metadata.msgpack";

impl<'a, T: BuildContext> SourceDistCachedBuilder<'a, T> {
    /// Initialize a [`SourceDistCachedBuilder`] from a [`BuildContext`].
    pub fn new(build_context: &'a T, client: &'a RegistryClient, tags: &'a Tags) -> Self {
        Self {
            build_context,
            reporter: None,
            client,
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

    /// Download and build a [`SourceDist`].
    pub async fn download_and_build(
        &self,
        source_dist: &SourceDist,
    ) -> Result<BuiltWheelMetadata, Error> {
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
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                )
                .boxed()
                .await?
            }
            SourceDist::Registry(registry_source_dist) => {
                let url = match &registry_source_dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let path_source_dist = PathSourceDist {
                            name: registry_source_dist.filename.name.clone(),
                            url: VerbatimUrl::unknown(
                                Url::from_file_path(path).expect("path is absolute"),
                            ),
                            path: path.clone(),
                            editable: false,
                        };
                        return self.path(source_dist, &path_source_dist).boxed().await;
                    }
                };

                // For registry source distributions, shard by package, then version.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Index(&registry_source_dist.index)
                        .remote_wheel_dir(registry_source_dist.filename.name.as_ref())
                        .join(registry_source_dist.filename.version.to_string()),
                );

                self.url(
                    source_dist,
                    &registry_source_dist.file.filename,
                    &url,
                    &cache_shard,
                    None,
                )
                .boxed()
                .await?
            }
            SourceDist::Git(git_source_dist) => {
                self.git(source_dist, git_source_dist).boxed().await?
            }
            SourceDist::Path(path_source_dist) => {
                self.path(source_dist, path_source_dist).boxed().await?
            }
        };

        Ok(built_wheel_metadata)
    }

    /// Download a [`SourceDist`] and determine its metadata. This typically involves building the
    /// source distribution into a wheel; however, some build backends support determining the
    /// metadata without building the source distribution.
    pub async fn download_and_build_metadata(
        &self,
        source_dist: &SourceDist,
    ) -> Result<Metadata21, Error> {
        let metadata = match &source_dist {
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

                self.url_metadata(
                    source_dist,
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                )
                .boxed()
                .await?
            }
            SourceDist::Registry(registry_source_dist) => {
                let url = match &registry_source_dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let path_source_dist = PathSourceDist {
                            name: registry_source_dist.filename.name.clone(),
                            url: VerbatimUrl::unknown(
                                Url::from_file_path(path).expect("path is absolute"),
                            ),
                            path: path.clone(),
                            editable: false,
                        };
                        return self
                            .path_metadata(source_dist, &path_source_dist)
                            .boxed()
                            .await;
                    }
                };

                // For registry source distributions, shard by package, then version.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Index(&registry_source_dist.index)
                        .remote_wheel_dir(registry_source_dist.filename.name.as_ref())
                        .join(registry_source_dist.filename.version.to_string()),
                );

                self.url_metadata(
                    source_dist,
                    &registry_source_dist.file.filename,
                    &url,
                    &cache_shard,
                    None,
                )
                .boxed()
                .await?
            }
            SourceDist::Git(git_source_dist) => {
                self.git_metadata(source_dist, git_source_dist)
                    .boxed()
                    .await?
            }
            SourceDist::Path(path_source_dist) => {
                self.path_metadata(source_dist, path_source_dist)
                    .boxed()
                    .await?
            }
        };

        Ok(metadata)
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
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_entry = cache_shard.entry(MANIFEST);
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&cache_entry, Some(source_dist.name()))
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let download = |response| {
            async {
                // At this point, we're seeing a new or updated source distribution. Initialize a
                // new manifest, to collect the source and built artifacts.
                let manifest = Manifest::new();

                // Download the source distribution.
                debug!("Downloading source distribution: {source_dist}");
                let source_dist_entry = cache_shard.shard(manifest.id()).entry(filename);
                self.persist_source_dist_url(response, source_dist, filename, &source_dist_entry)
                    .await?;

                Ok(manifest)
            }
            .boxed()
            .instrument(info_span!("download", source_dist = %source_dist))
        };
        let req = self
            .client
            .cached_client()
            .uncached()
            .get(url.clone())
            .build()?;
        let manifest = self
            .client
            .cached_client()
            .get_serde(req, &cache_entry, cache_control, download)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        // From here on, scope all operations to the current build. Within the manifest shard,
        // there's no need to check for freshness, since entries have to be fresher than the
        // manifest itself. There's also no need to lock, since we never replace entries within the
        // shard.
        let cache_shard = cache_shard.shard(manifest.id());

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(self.tags, &cache_shard) {
            return Ok(built_wheel);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        // Build the source distribution.
        let source_dist_entry = cache_shard.entry(filename);
        let (disk_filename, wheel_filename, metadata) = self
            .build_source_dist(
                source_dist,
                source_dist_entry.path(),
                subdirectory,
                &cache_shard,
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Store the metadata.
        let metadata_entry = cache_shard.entry(METADATA);
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(BuiltWheelMetadata {
            path: cache_shard.join(&disk_filename),
            target: cache_shard.join(wheel_filename.stem()),
            filename: wheel_filename,
        })
    }

    /// Build the source distribution's metadata from a local path.
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    #[allow(clippy::too_many_arguments)]
    async fn url_metadata<'data>(
        &self,
        source_dist: &'data SourceDist,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
    ) -> Result<Metadata21, Error> {
        let cache_entry = cache_shard.entry(MANIFEST);
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&cache_entry, Some(source_dist.name()))
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let download = |response| {
            async {
                // At this point, we're seeing a new or updated source distribution. Initialize a
                // new manifest, to collect the source and built artifacts.
                let manifest = Manifest::new();

                // Download the source distribution.
                debug!("Downloading source distribution: {source_dist}");
                let source_dist_entry = cache_shard.shard(manifest.id()).entry(filename);
                self.persist_source_dist_url(response, source_dist, filename, &source_dist_entry)
                    .await?;

                Ok(manifest)
            }
            .boxed()
            .instrument(info_span!("download", source_dist = %source_dist))
        };
        let req = self
            .client
            .cached_client()
            .uncached()
            .get(url.clone())
            .build()?;
        let manifest = self
            .client
            .cached_client()
            .get_serde(req, &cache_entry, cache_control, download)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        // From here on, scope all operations to the current build. Within the manifest shard,
        // there's no need to check for freshness, since entries have to be fresher than the
        // manifest itself. There's also no need to lock, since we never replace entries within the
        // shard.
        let cache_shard = cache_shard.shard(manifest.id());

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
            debug!("Using cached metadata for {source_dist}");
            return Ok(metadata.clone());
        }

        // Otherwise, we either need to build the metadata or the wheel.
        let source_dist_entry = cache_shard.entry(filename);

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_source_dist_metadata(source_dist, source_dist_entry.path(), subdirectory)
            .boxed()
            .await?
        {
            // Store the metadata.
            let cache_entry = cache_shard.entry(METADATA);
            fs::create_dir_all(cache_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(metadata);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        // Build the source distribution.
        let (_disk_filename, _wheel_filename, metadata) = self
            .build_source_dist(
                source_dist,
                source_dist_entry.path(),
                subdirectory,
                &cache_shard,
            )
            .await?;

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        Ok(metadata)
    }

    /// Build a source distribution from a local path.
    async fn path(
        &self,
        source_dist: &SourceDist,
        path_source_dist: &PathSourceDist,
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(&path_source_dist.url)
                .remote_wheel_dir(path_source_dist.name().as_ref()),
        );

        // Determine the last-modified time of the source distribution.
        let Some(modified) =
            ArchiveTimestamp::from_path(&path_source_dist.path).map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the existing metadata from the cache.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let manifest_freshness = self
            .build_context
            .cache()
            .freshness(&manifest_entry, Some(source_dist.name()))
            .map_err(Error::CacheRead)?;
        let manifest =
            refresh_timestamp_manifest(&manifest_entry, manifest_freshness, modified).await?;

        // From here on, scope all operations to the current build. Within the manifest shard,
        // there's no need to check for freshness, since entries have to be fresher than the
        // manifest itself. There's also no need to lock, since we never replace entries within the
        // shard.
        let cache_shard = cache_shard.shard(manifest.id());

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(self.tags, &cache_shard) {
            return Ok(built_wheel);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        let (disk_filename, filename, metadata) = self
            .build_source_dist(source_dist, &path_source_dist.path, None, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(BuiltWheelMetadata {
            path: cache_shard.join(&disk_filename),
            target: cache_shard.join(filename.stem()),
            filename,
        })
    }

    /// Build the source distribution's metadata from a local path.
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn path_metadata(
        &self,
        source_dist: &SourceDist,
        path_source_dist: &PathSourceDist,
    ) -> Result<Metadata21, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(&path_source_dist.url)
                .remote_wheel_dir(path_source_dist.name().as_ref()),
        );

        // Determine the last-modified time of the source distribution.
        let Some(modified) =
            ArchiveTimestamp::from_path(&path_source_dist.path).map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the existing metadata from the cache, to clear stale entries.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let manifest_freshness = self
            .build_context
            .cache()
            .freshness(&manifest_entry, Some(source_dist.name()))
            .map_err(Error::CacheRead)?;
        let manifest =
            refresh_timestamp_manifest(&manifest_entry, manifest_freshness, modified).await?;

        // From here on, scope all operations to the current build. Within the manifest shard,
        // there's no need to check for freshness, since entries have to be fresher than the
        // manifest itself. There's also no need to lock, since we never replace entries within the
        // shard.
        let cache_shard = cache_shard.shard(manifest.id());

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if self
            .build_context
            .cache()
            .freshness(&metadata_entry, Some(source_dist.name()))
            .is_ok_and(Freshness::is_fresh)
        {
            if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
                debug!("Using cached metadata for {source_dist}");
                return Ok(metadata.clone());
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_source_dist_metadata(source_dist, &path_source_dist.path, None)
            .boxed()
            .await?
        {
            // Store the metadata.
            let cache_entry = cache_shard.entry(METADATA);
            fs::create_dir_all(cache_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(metadata);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        let (_disk_filename, _filename, metadata) = self
            .build_source_dist(source_dist, &path_source_dist.path, None, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(metadata)
    }

    /// Build a source distribution from a Git repository.
    async fn git(
        &self,
        source_dist: &SourceDist,
        git_source_dist: &GitSourceDist,
    ) -> Result<BuiltWheelMetadata, Error> {
        let (fetch, subdirectory) = self.download_source_dist_git(&git_source_dist.url).await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&git_source_dist.url, &git_sha.to_short_string())
                .remote_wheel_dir(git_source_dist.name().as_ref()),
        );

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(self.tags, &cache_shard) {
            return Ok(built_wheel);
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
                &cache_shard,
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(BuiltWheelMetadata {
            path: cache_shard.join(&disk_filename),
            target: cache_shard.join(filename.stem()),
            filename,
        })
    }

    /// Build the source distribution's metadata from a Git repository.
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn git_metadata(
        &self,
        source_dist: &SourceDist,
        git_source_dist: &GitSourceDist,
    ) -> Result<Metadata21, Error> {
        let (fetch, subdirectory) = self.download_source_dist_git(&git_source_dist.url).await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&git_source_dist.url, &git_sha.to_short_string())
                .remote_wheel_dir(git_source_dist.name().as_ref()),
        );

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if self
            .build_context
            .cache()
            .freshness(&metadata_entry, Some(source_dist.name()))
            .is_ok_and(Freshness::is_fresh)
        {
            if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
                debug!("Using cached metadata for {source_dist}");
                return Ok(metadata.clone());
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_source_dist_metadata(source_dist, fetch.path(), subdirectory.as_deref())
            .boxed()
            .await?
        {
            // Store the metadata.
            let cache_entry = cache_shard.entry(METADATA);
            fs::create_dir_all(cache_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(metadata);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        let (_disk_filename, _filename, metadata) = self
            .build_source_dist(
                source_dist,
                fetch.path(),
                subdirectory.as_deref(),
                &cache_shard,
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(metadata)
    }

    /// Download and unzip a source distribution into the cache from an HTTP response.
    async fn persist_source_dist_url<'data>(
        &self,
        response: Response,
        source_dist: &SourceDist,
        filename: &str,
        cache_entry: &'data CacheEntry,
    ) -> Result<&'data Path, Error> {
        let cache_path = cache_entry.path();
        if cache_path.is_dir() {
            debug!("Distribution is already cached: {source_dist}");
            return Ok(cache_path);
        }

        // Download and unzip the source distribution into a temporary directory.
        let span =
            info_span!("download_source_dist", filename = filename, source_dist = %source_dist);
        let temp_dir =
            tempfile::tempdir_in(self.build_context.cache().root()).map_err(Error::CacheWrite)?;
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();
        uv_extract::stream::archive(reader.compat(), filename, temp_dir.path()).await?;
        drop(span);

        // Extract the top-level directory.
        let extracted = uv_extract::strip_component(temp_dir.path())?;

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(cache_path.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        fs_err::tokio::rename(extracted, &cache_path)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(cache_path)
    }

    /// Download a source distribution from a Git repository.
    async fn download_source_dist_git(&self, url: &Url) -> Result<(Fetch, Option<PathBuf>), Error> {
        debug!("Fetching source distribution from Git: {url}");
        let git_dir = self.build_context.cache().bucket(CacheBucket::Git);

        // Avoid races between different processes, too.
        let lock_dir = git_dir.join("locks");
        fs::create_dir_all(&lock_dir)
            .await
            .map_err(Error::CacheWrite)?;
        let canonical_url = cache_key::CanonicalUrl::new(url);
        let _lock = LockedFile::acquire(
            lock_dir.join(cache_key::digest(&canonical_url)),
            &canonical_url,
        )
        .map_err(Error::CacheWrite)?;

        let DirectGitUrl { url, subdirectory } = DirectGitUrl::try_from(url).map_err(Error::Git)?;

        let source = if let Some(reporter) = &self.reporter {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter.clone()))
        } else {
            GitSource::new(url, git_dir)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(Error::Git)?;
        Ok((fetch, subdirectory))
    }

    /// Build a source distribution, storing the built wheel in the cache.
    ///
    /// Returns the un-normalized disk filename, the parsed, normalized filename and the metadata
    #[instrument(skip_all, fields(dist))]
    async fn build_source_dist(
        &self,
        dist: &SourceDist,
        source_dist: &Path,
        subdirectory: Option<&Path>,
        cache_shard: &CacheShard,
    ) -> Result<(String, WheelFilename, Metadata21), Error> {
        debug!("Building: {dist}");

        // Guard against build of source distributions when disabled
        let no_build = match self.build_context.no_build() {
            NoBuild::All => true,
            NoBuild::None => false,
            NoBuild::Packages(packages) => packages.contains(dist.name()),
        };
        if no_build {
            return Err(Error::NoBuild);
        }

        // Build the wheel.
        fs::create_dir_all(&cache_shard)
            .await
            .map_err(Error::CacheWrite)?;
        let disk_filename = self
            .build_context
            .setup_build(
                source_dist,
                subdirectory,
                &dist.to_string(),
                Some(dist),
                BuildKind::Wheel,
            )
            .await
            .map_err(|err| Error::Build(dist.to_string(), err))?
            .wheel(cache_shard)
            .await
            .map_err(|err| Error::Build(dist.to_string(), err))?;

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_wheel_metadata(&filename, cache_shard.join(&disk_filename))?;

        // Validate the metadata.
        if &metadata.name != dist.name() {
            return Err(Error::NameMismatch {
                metadata: metadata.name,
                given: dist.name().clone(),
            });
        }

        debug!("Finished building: {dist}");
        Ok((disk_filename, filename, metadata))
    }

    /// Build the metadata for a source distribution.
    #[instrument(skip_all, fields(dist))]
    async fn build_source_dist_metadata(
        &self,
        dist: &SourceDist,
        source_dist: &Path,
        subdirectory: Option<&Path>,
    ) -> Result<Option<Metadata21>, Error> {
        debug!("Preparing metadata for: {dist}");

        // Setup the builder.
        let mut builder = self
            .build_context
            .setup_build(
                source_dist,
                subdirectory,
                &dist.to_string(),
                Some(dist),
                BuildKind::Wheel,
            )
            .await
            .map_err(|err| Error::Build(dist.to_string(), err))?;

        // Build the metadata.
        let dist_info = builder
            .metadata()
            .await
            .map_err(|err| Error::Build(dist.to_string(), err))?;
        let Some(dist_info) = dist_info else {
            return Ok(None);
        };

        // Read the metadata from disk.
        debug!("Prepared metadata for: {dist}");
        let content = fs::read(dist_info.join("METADATA"))
            .await
            .map_err(Error::CacheRead)?;
        let metadata = Metadata21::parse(&content)?;

        // Validate the metadata.
        if &metadata.name != dist.name() {
            return Err(Error::NameMismatch {
                metadata: metadata.name,
                given: dist.name().clone(),
            });
        }

        Ok(Some(metadata))
    }

    /// Build a single directory into an editable wheel
    pub async fn build_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(Dist, String, WheelFilename, Metadata21), Error> {
        debug!("Building (editable) {editable}");
        let disk_filename = self
            .build_context
            .setup_build(
                &editable.path,
                None,
                &editable.to_string(),
                None,
                BuildKind::Editable,
            )
            .await
            .map_err(|err| Error::BuildEditable(editable.to_string(), err))?
            .wheel(editable_wheel_dir)
            .await
            .map_err(|err| Error::BuildEditable(editable.to_string(), err))?;
        let filename = WheelFilename::from_str(&disk_filename)?;
        // We finally have the name of the package and can construct the dist.
        let dist = Dist::Source(SourceDist::Path(PathSourceDist {
            name: filename.name.clone(),
            url: editable.url().clone(),
            path: editable.path.clone(),
            editable: true,
        }));
        let metadata = read_wheel_metadata(&filename, editable_wheel_dir.join(&disk_filename))?;

        debug!("Finished building (editable): {dist}");
        Ok((dist, disk_filename, filename, metadata))
    }
}

/// Read an existing HTTP-cached [`Manifest`], if it exists.
pub(crate) fn read_http_manifest(cache_entry: &CacheEntry) -> Result<Option<Manifest>, Error> {
    match std::fs::File::open(cache_entry.path()) {
        Ok(file) => {
            let data = DataWithCachePolicy::from_reader(file)?.data;
            Ok(Some(rmp_serde::from_slice::<Manifest>(&data)?))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::CacheRead(err)),
    }
}

/// Read an existing timestamped [`Manifest`], if it exists and is up-to-date.
///
/// If the cache entry is stale, a new entry will be created.
pub(crate) fn read_timestamp_manifest(
    cache_entry: &CacheEntry,
    modified: ArchiveTimestamp,
) -> Result<Option<Manifest>, Error> {
    // If the cache entry is up-to-date, return it.
    match std::fs::read(cache_entry.path()) {
        Ok(cached) => {
            let cached = rmp_serde::from_slice::<CachedByTimestamp<Manifest>>(&cached)?;
            if cached.timestamp == modified.timestamp() {
                return Ok(Some(cached.data));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(Error::CacheRead(err)),
    }
    Ok(None)
}

/// Read an existing timestamped [`Manifest`], if it exists and is up-to-date.
///
/// If the cache entry is stale, a new entry will be created.
pub(crate) async fn refresh_timestamp_manifest(
    cache_entry: &CacheEntry,
    freshness: Freshness,
    modified: ArchiveTimestamp,
) -> Result<Manifest, Error> {
    // If we know the exact modification time, we don't need to force a revalidate.
    if matches!(modified, ArchiveTimestamp::Exact(_)) || freshness.is_fresh() {
        if let Some(manifest) = read_timestamp_manifest(cache_entry, modified)? {
            return Ok(manifest);
        }
    }

    // Otherwise, create a new manifest.
    let manifest = Manifest::new();
    fs::create_dir_all(&cache_entry.dir())
        .await
        .map_err(Error::CacheWrite)?;
    write_atomic(
        cache_entry.path(),
        rmp_serde::to_vec(&CachedByTimestamp {
            timestamp: modified.timestamp(),
            data: manifest.clone(),
        })?,
    )
    .await
    .map_err(Error::CacheWrite)?;
    Ok(manifest)
}

/// Read an existing cached [`Metadata21`], if it exists.
pub(crate) async fn read_cached_metadata(
    cache_entry: &CacheEntry,
) -> Result<Option<Metadata21>, Error> {
    match fs::read(&cache_entry.path()).await {
        Ok(cached) => Ok(Some(rmp_serde::from_slice::<Metadata21>(&cached)?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::CacheRead(err)),
    }
}

/// Read the [`Metadata21`] from a built wheel.
fn read_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl Into<PathBuf>,
) -> Result<Metadata21, Error> {
    let file = fs_err::File::open(wheel).map_err(Error::CacheRead)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;
    let dist_info = read_dist_info(filename, &mut archive)?;
    Ok(Metadata21::parse(&dist_info)?)
}
