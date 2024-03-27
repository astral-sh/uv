//! Fetch and build source distributions from remote sources.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use fs_err::tokio as fs;
use futures::{FutureExt, TryStreamExt};
use reqwest::Response;
use tempfile::TempDir;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, Instrument};
use url::Url;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::{
    BuildableSource, DirectArchiveUrl, Dist, FileLocation, GitSourceUrl, LocalEditable,
    PathSourceDist, PathSourceUrl, RemoteSource, SourceDist, SourceUrl,
};
use install_wheel_rs::metadata::read_archive_metadata;
use pep508_rs::Scheme;
use platform_tags::Tags;
use pypi_types::Metadata23;
use uv_cache::{
    ArchiveTimestamp, Cache, CacheBucket, CacheEntry, CacheShard, CachedByTimestamp, Freshness,
    WheelCache,
};
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_fs::write_atomic;
use uv_types::{BuildContext, BuildKind, NoBuild, SourceBuildTrait};

use crate::error::Error;
use crate::git::fetch_git_archive;
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
}

/// The name of the file that contains the cached manifest, encoded via `MsgPack`.
pub(crate) const MANIFEST: &str = "manifest.msgpack";

/// The name of the file that contains the cached distribution metadata, encoded via `MsgPack`.
pub(crate) const METADATA: &str = "metadata.msgpack";

impl<'a, T: BuildContext> SourceDistCachedBuilder<'a, T> {
    /// Initialize a [`SourceDistCachedBuilder`] from a [`BuildContext`].
    pub fn new(build_context: &'a T, client: &'a RegistryClient) -> Self {
        Self {
            build_context,
            reporter: None,
            client,
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
        source: &BuildableSource<'_>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let built_wheel_metadata = match &source {
            BuildableSource::Dist(SourceDist::Registry(dist)) => {
                let url = match &dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let url = Url::from_file_path(path).expect("path is absolute");

                        // If necessary, extract the archive.
                        let extracted = extract_archive(path, self.build_context.cache()).await?;

                        return self
                            .path(
                                source,
                                &PathSourceUrl {
                                    url: &url,
                                    path: Cow::Borrowed(path),
                                },
                                extracted.path(),
                                tags,
                            )
                            .boxed()
                            .await;
                    }
                };

                // For registry source distributions, shard by package, then version, for
                // convenience in debugging.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Index(&dist.index)
                        .wheel_dir(dist.filename.name.as_ref())
                        .join(dist.filename.version.to_string()),
                );

                self.url(source, &dist.file.filename, &url, &cache_shard, None, tags)
                    .boxed()
                    .await?
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => {
                let filename = dist.filename().expect("Distribution must have a filename");
                let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(dist.url.raw());

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self
                    .build_context
                    .cache()
                    .shard(CacheBucket::BuiltWheels, WheelCache::Url(&url).root());

                self.url(
                    source,
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                    tags,
                )
                .boxed()
                .await?
            }
            BuildableSource::Dist(SourceDist::Git(dist)) => {
                self.git(source, &GitSourceUrl::from(dist), tags)
                    .boxed()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Path(dist)) => {
                // If necessary, extract the archive.
                let extracted = extract_archive(&dist.path, self.build_context.cache()).await?;

                self.path(source, &PathSourceUrl::from(dist), extracted.path(), tags)
                    .boxed()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Direct(resource)) => {
                let filename = resource
                    .url
                    .filename()
                    .expect("Distribution must have a filename");
                let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(resource.url);

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self
                    .build_context
                    .cache()
                    .shard(CacheBucket::BuiltWheels, WheelCache::Url(&url).root());

                self.url(
                    source,
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                    tags,
                )
                .boxed()
                .await?
            }
            BuildableSource::Url(SourceUrl::Git(resource)) => {
                self.git(source, resource, tags).boxed().await?
            }
            BuildableSource::Url(SourceUrl::Path(resource)) => {
                // If necessary, extract the archive.
                let extracted = extract_archive(&resource.path, self.build_context.cache()).await?;

                self.path(source, resource, extracted.path(), tags)
                    .boxed()
                    .await?
            }
        };

        Ok(built_wheel_metadata)
    }

    /// Download a [`SourceDist`] and determine its metadata. This typically involves building the
    /// source distribution into a wheel; however, some build backends support determining the
    /// metadata without building the source distribution.
    pub async fn download_and_build_metadata(
        &self,
        source: &BuildableSource<'_>,
    ) -> Result<Metadata23, Error> {
        let metadata = match &source {
            BuildableSource::Dist(SourceDist::Registry(dist)) => {
                let url = match &dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let url = Url::from_file_path(path).expect("path is absolute");

                        // If necessary, extract the archive.
                        let extracted = extract_archive(path, self.build_context.cache()).await?;

                        return self
                            .path_metadata(
                                source,
                                &PathSourceUrl {
                                    url: &url,
                                    path: Cow::Borrowed(path),
                                },
                                extracted.path(),
                            )
                            .boxed()
                            .await;
                    }
                };

                // For registry source distributions, shard by package, then version.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::BuiltWheels,
                    WheelCache::Index(&dist.index)
                        .wheel_dir(dist.filename.name.as_ref())
                        .join(dist.filename.version.to_string()),
                );

                self.url_metadata(source, &dist.file.filename, &url, &cache_shard, None)
                    .boxed()
                    .await?
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => {
                let filename = dist.filename().expect("Distribution must have a filename");
                let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(dist.url.raw());

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self
                    .build_context
                    .cache()
                    .shard(CacheBucket::BuiltWheels, WheelCache::Url(&url).root());

                self.url_metadata(
                    source,
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                )
                .boxed()
                .await?
            }
            BuildableSource::Dist(SourceDist::Git(dist)) => {
                self.git_metadata(source, &GitSourceUrl::from(dist))
                    .boxed()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Path(dist)) => {
                // If necessary, extract the archive.
                let extracted = extract_archive(&dist.path, self.build_context.cache()).await?;

                self.path_metadata(source, &PathSourceUrl::from(dist), extracted.path())
                    .boxed()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Direct(resource)) => {
                let filename = resource
                    .url
                    .filename()
                    .expect("Distribution must have a filename");
                let DirectArchiveUrl { url, subdirectory } = DirectArchiveUrl::from(resource.url);

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self
                    .build_context
                    .cache()
                    .shard(CacheBucket::BuiltWheels, WheelCache::Url(&url).root());

                self.url_metadata(
                    source,
                    &filename,
                    &url,
                    &cache_shard,
                    subdirectory.as_deref(),
                )
                .boxed()
                .await?
            }
            BuildableSource::Url(SourceUrl::Git(resource)) => {
                self.git_metadata(source, resource).boxed().await?
            }
            BuildableSource::Url(SourceUrl::Path(resource)) => {
                // If necessary, extract the archive.
                let extracted = extract_archive(&resource.path, self.build_context.cache()).await?;

                self.path_metadata(source, resource, extracted.path())
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
        source: &BuildableSource<'data>,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_entry = cache_shard.entry(MANIFEST);
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&cache_entry, source.name())
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
                debug!("Downloading source distribution: {source}");
                let source_dist_entry = cache_shard.shard(manifest.id()).entry(filename);
                self.persist_url(response, source, filename, &source_dist_entry)
                    .await?;

                Ok(manifest)
            }
            .boxed()
            .instrument(info_span!("download", source_dist = %source))
        };
        let req = self
            .client
            .uncached_client()
            .get(url.clone())
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
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
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        // Build the source distribution.
        let source_dist_entry = cache_shard.entry(filename);
        let (disk_filename, wheel_filename, metadata) = self
            .build_distribution(source, source_dist_entry.path(), subdirectory, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
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
        source: &BuildableSource<'data>,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
    ) -> Result<Metadata23, Error> {
        let cache_entry = cache_shard.entry(MANIFEST);
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&cache_entry, source.name())
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
                debug!("Downloading source distribution: {source}");
                let source_dist_entry = cache_shard.shard(manifest.id()).entry(filename);
                self.persist_url(response, source, filename, &source_dist_entry)
                    .await?;

                Ok(manifest)
            }
            .boxed()
            .instrument(info_span!("download", source_dist = %source))
        };
        let req = self
            .client
            .uncached_client()
            .get(url.clone())
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
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
            debug!("Using cached metadata for: {source}");
            return Ok(metadata);
        }

        // Otherwise, we either need to build the metadata or the wheel.
        let source_dist_entry = cache_shard.entry(filename);

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_dist_entry.path(), subdirectory)
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
            .map(|reporter| reporter.on_build_start(source));

        // Build the source distribution.
        let (_disk_filename, _wheel_filename, metadata) = self
            .build_distribution(source, source_dist_entry.path(), subdirectory, &cache_shard)
            .await?;

        // Store the metadata.
        let cache_entry = cache_shard.entry(METADATA);
        write_atomic(cache_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        Ok(metadata)
    }

    /// Build a source distribution from a local path.
    async fn path(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        source_root: &Path,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Determine the last-modified time of the source distribution.
        let Some(modified) =
            ArchiveTimestamp::from_path(&resource.path).map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the existing metadata from the cache.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let manifest_freshness = self
            .build_context
            .cache()
            .freshness(&manifest_entry, source.name())
            .map_err(Error::CacheRead)?;
        let manifest =
            refresh_timestamp_manifest(&manifest_entry, manifest_freshness, modified).await?;

        // From here on, scope all operations to the current build. Within the manifest shard,
        // there's no need to check for freshness, since entries have to be fresher than the
        // manifest itself. There's also no need to lock, since we never replace entries within the
        // shard.
        let cache_shard = cache_shard.shard(manifest.id());

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(source, source_root, None, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // Store the metadata.
        let metadata_entry = cache_shard.entry(METADATA);
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
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
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        source_root: &Path,
    ) -> Result<Metadata23, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Determine the last-modified time of the source distribution.
        let Some(modified) =
            ArchiveTimestamp::from_path(&resource.path).map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the existing metadata from the cache, to clear stale entries.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let manifest_freshness = self
            .build_context
            .cache()
            .freshness(&manifest_entry, source.name())
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
            .freshness(&metadata_entry, source.name())
            .is_ok_and(Freshness::is_fresh)
        {
            if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
                debug!("Using cached metadata for: {source}");
                return Ok(metadata);
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_root, None)
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
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(source, source_root, None, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // Store the metadata.
        let metadata_entry = cache_shard.entry(METADATA);
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(metadata)
    }

    /// Build a source distribution from a Git repository.
    async fn git(
        &self,
        source: &BuildableSource<'_>,
        resource: &GitSourceUrl<'_>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let (fetch, subdirectory) = fetch_git_archive(
            resource.url,
            self.build_context.cache(),
            self.reporter.as_ref(),
        )
        .await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(resource.url, &git_sha.to_short_string()).root(),
        );

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(source, fetch.path(), subdirectory.as_deref(), &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
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
        source: &BuildableSource<'_>,
        resource: &GitSourceUrl<'_>,
    ) -> Result<Metadata23, Error> {
        let (fetch, subdirectory) = fetch_git_archive(
            resource.url,
            self.build_context.cache(),
            self.reporter.as_ref(),
        )
        .await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(resource.url, &git_sha.to_short_string()).root(),
        );

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if self
            .build_context
            .cache()
            .freshness(&metadata_entry, source.name())
            .is_ok_and(Freshness::is_fresh)
        {
            if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
                debug!("Using cached metadata for: {source}");
                return Ok(metadata);
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, fetch.path(), subdirectory.as_deref())
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
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(source, fetch.path(), subdirectory.as_deref(), &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
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
    async fn persist_url<'data>(
        &self,
        response: Response,
        source: &BuildableSource<'_>,
        filename: &str,
        cache_entry: &'data CacheEntry,
    ) -> Result<&'data Path, Error> {
        let cache_path = cache_entry.path();
        if cache_path.is_dir() {
            debug!("Distribution is already cached: {source}");
            return Ok(cache_path);
        }

        // Download and unzip the source distribution into a temporary directory.
        let span = info_span!("download_source_dist", filename = filename, source_dist = %source);
        let temp_dir =
            tempfile::tempdir_in(self.build_context.cache().bucket(CacheBucket::BuiltWheels))
                .map_err(Error::CacheWrite)?;
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();
        uv_extract::stream::archive(reader.compat(), filename, temp_dir.path()).await?;
        drop(span);

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.into_path(),
            Err(err) => return Err(err.into()),
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(cache_path.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        fs_err::tokio::rename(extracted, &cache_path)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(cache_path)
    }

    /// Build a source distribution, storing the built wheel in the cache.
    ///
    /// Returns the un-normalized disk filename, the parsed, normalized filename and the metadata
    #[instrument(skip_all, fields(dist))]
    async fn build_distribution(
        &self,
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
        cache_shard: &CacheShard,
    ) -> Result<(String, WheelFilename, Metadata23), Error> {
        debug!("Building: {source}");

        // Guard against build of source distributions when disabled.
        let no_build = match self.build_context.no_build() {
            NoBuild::All => true,
            NoBuild::None => false,
            NoBuild::Packages(packages) => {
                source.name().is_some_and(|name| packages.contains(name))
            }
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
                source_root,
                subdirectory,
                &source.to_string(),
                source.as_dist(),
                BuildKind::Wheel,
            )
            .await
            .map_err(|err| Error::Build(source.to_string(), err))?
            .wheel(cache_shard)
            .await
            .map_err(|err| Error::Build(source.to_string(), err))?;

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_wheel_metadata(&filename, cache_shard.join(&disk_filename))?;

        // Validate the metadata.
        if let Some(name) = source.name() {
            if metadata.name != *name {
                return Err(Error::NameMismatch {
                    metadata: metadata.name,
                    given: name.clone(),
                });
            }
        }

        debug!("Finished building: {source}");
        Ok((disk_filename, filename, metadata))
    }

    /// Build the metadata for a source distribution.
    #[instrument(skip_all, fields(dist))]
    async fn build_metadata(
        &self,
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
    ) -> Result<Option<Metadata23>, Error> {
        debug!("Preparing metadata for: {source}");

        // Attempt to read static metadata from the `PKG-INFO` file.
        match read_pkg_info(source_root).await {
            Ok(metadata) => {
                debug!("Found static `PKG-INFO` for: {source}");

                // Validate the metadata.
                if let Some(name) = source.name() {
                    if metadata.name != *name {
                        return Err(Error::NameMismatch {
                            metadata: metadata.name,
                            given: name.clone(),
                        });
                    }
                }

                return Ok(Some(metadata));
            }
            Err(err @ (Error::MissingPkgInfo | Error::DynamicPkgInfo(_))) => {
                debug!("No static `PKG-INFO` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        // Attempt to read static metadata from the `pyproject.toml`.
        match read_pyproject_toml(source_root).await {
            Ok(metadata) => {
                debug!("Found static `pyproject.toml` for: {source}");

                // Validate the metadata.
                if let Some(name) = source.name() {
                    if metadata.name != *name {
                        return Err(Error::NameMismatch {
                            metadata: metadata.name,
                            given: name.clone(),
                        });
                    }
                }

                return Ok(Some(metadata));
            }
            Err(err @ (Error::MissingPyprojectToml | Error::DynamicPyprojectToml(_))) => {
                debug!("No static `pyproject.toml` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        // Setup the builder.
        let mut builder = self
            .build_context
            .setup_build(
                source_root,
                subdirectory,
                &source.to_string(),
                source.as_dist(),
                BuildKind::Wheel,
            )
            .await
            .map_err(|err| Error::Build(source.to_string(), err))?;

        // Build the metadata.
        let dist_info = builder
            .metadata()
            .await
            .map_err(|err| Error::Build(source.to_string(), err))?;
        let Some(dist_info) = dist_info else {
            return Ok(None);
        };

        // Read the metadata from disk.
        debug!("Prepared metadata for: {source}");
        let content = fs::read(dist_info.join("METADATA"))
            .await
            .map_err(Error::CacheRead)?;
        let metadata = Metadata23::parse_metadata(&content)?;

        // Validate the metadata.
        if let Some(name) = source.name() {
            if metadata.name != *name {
                return Err(Error::NameMismatch {
                    metadata: metadata.name,
                    given: name.clone(),
                });
            }
        }

        Ok(Some(metadata))
    }

    /// Build a single directory into an editable wheel
    pub async fn build_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(Dist, String, WheelFilename, Metadata23), Error> {
        debug!("Building (editable) {editable}");

        // Verify that the editable exists.
        if !editable.path.exists() {
            return Err(Error::NotFound(editable.path.clone()));
        }

        // Build the wheel.
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

#[derive(Debug)]
pub enum ExtractedSource {
    /// The source distribution was passed in as a directory, and so doesn't need to be extracted.
    Directory(PathBuf),
    /// The source distribution was passed in as an archive, and was extracted into a temporary
    /// directory.
    ///
    /// The extracted archive and temporary directory will be deleted when the `ExtractedSource` is
    /// dropped.
    #[allow(dead_code)]
    Archive(PathBuf, TempDir),
}

impl ExtractedSource {
    /// Return the [`Path`] to the extracted source root.
    pub fn path(&self) -> &Path {
        match self {
            ExtractedSource::Directory(path) => path,
            ExtractedSource::Archive(path, _) => path,
        }
    }
}

/// Read the [`Metadata23`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
/// or later _and_ none of the required fields (`Requires-Python`, `Requires-Dist`, and
/// `Provides-Extra`) are marked as dynamic.
pub(crate) async fn read_pkg_info(source_tree: &Path) -> Result<Metadata23, Error> {
    // Read the `PKG-INFO` file.
    let content = match fs::read(source_tree.join("PKG-INFO")).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingPkgInfo);
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Parse the metadata.
    let metadata = Metadata23::parse_pkg_info(&content).map_err(Error::DynamicPkgInfo)?;

    Ok(metadata)
}

/// Read the [`Metadata23`] from a source distribution's `pyproject.tom` file, if it defines static
/// metadata consistent with PEP 621.
pub(crate) async fn read_pyproject_toml(source_tree: &Path) -> Result<Metadata23, Error> {
    // Read the `pyproject.toml` file.
    let content = match fs::read_to_string(source_tree.join("pyproject.toml")).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingPyprojectToml);
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Parse the metadata.
    let metadata =
        Metadata23::parse_pyproject_toml(&content).map_err(Error::DynamicPyprojectToml)?;

    Ok(metadata)
}

/// Read an existing HTTP-cached [`Manifest`], if it exists.
pub(crate) fn read_http_manifest(cache_entry: &CacheEntry) -> Result<Option<Manifest>, Error> {
    match fs_err::File::open(cache_entry.path()) {
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
    match fs_err::read(cache_entry.path()) {
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

/// Read an existing cached [`Metadata23`], if it exists.
pub(crate) async fn read_cached_metadata(
    cache_entry: &CacheEntry,
) -> Result<Option<Metadata23>, Error> {
    match fs::read(&cache_entry.path()).await {
        Ok(cached) => Ok(Some(rmp_serde::from_slice::<Metadata23>(&cached)?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::CacheRead(err)),
    }
}

/// Read the [`Metadata23`] from a built wheel.
fn read_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl Into<PathBuf>,
) -> Result<Metadata23, Error> {
    let file = fs_err::File::open(wheel).map_err(Error::CacheRead)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;
    let dist_info = read_archive_metadata(filename, &mut archive)?;
    Ok(Metadata23::parse_metadata(&dist_info)?)
}

/// Extract a local source distribution, if it's stored as a `.tar.gz` or `.zip` archive.
///
/// TODO(charlie): Consider storing the extracted source in the cache, to avoid re-extracting
/// on every invocation.
async fn extract_archive(path: &Path, cache: &Cache) -> Result<ExtractedSource, Error> {
    let metadata = match fs::metadata(&path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotFound(path.to_path_buf()));
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    if metadata.is_dir() {
        Ok(ExtractedSource::Directory(path.to_path_buf()))
    } else {
        debug!("Unpacking for build: {}", path.display());

        let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::BuiltWheels))
            .map_err(Error::CacheWrite)?;

        // Unzip the archive into the temporary directory.
        let reader = fs_err::tokio::File::open(&path)
            .await
            .map_err(Error::CacheRead)?;
        uv_extract::seek::archive(tokio::io::BufReader::new(reader), path, &temp_dir.path())
            .await?;

        // Extract the top-level directory from the archive.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
            Err(err) => return Err(err.into()),
        };

        Ok(ExtractedSource::Archive(extracted, temp_dir))
    }
}

/// Download and extract a source distribution from a URL.
///
/// This function will download the source distribution from the given URL, and extract it into a
/// directory.
///
/// For VCS distributions, this method will checkout the URL into the shared Git cache.
///
/// For local archives, this method will extract the archive into a temporary directory.
///
/// For HTTP distributions, this method will download the archive and extract it into a temporary
/// directory.
pub async fn download_and_extract_archive(
    url: &Url,
    cache: &Cache,
    client: &RegistryClient,
) -> Result<ExtractedSource, Error> {
    match Scheme::parse(url.scheme()) {
        // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
        Some(Scheme::File) => {
            let path = url.to_file_path().expect("URL to be a file path");
            extract_archive(&path, cache).await
        }
        // Ex) `git+https://github.com/pallets/flask`
        Some(Scheme::GitSsh | Scheme::GitHttps) => {
            // Download the source distribution from the Git repository.
            let (fetch, subdirectory) = fetch_git_archive(url, cache, None).await?;
            let path = if let Some(subdirectory) = subdirectory {
                fetch.path().join(subdirectory)
            } else {
                fetch.path().to_path_buf()
            };
            Ok(ExtractedSource::Directory(path))
        }
        // Ex) `https://download.pytorch.org/whl/torch_stable.html`
        Some(Scheme::Http | Scheme::Https) => {
            let filename = url.filename().expect("Distribution must have a filename");

            // Build a request to download the source distribution.
            let req = client
                .uncached_client()
                .get(url.clone())
                .header(
                    // `reqwest` defaults to accepting compressed responses.
                    // Specify identity encoding to get consistent .whl downloading
                    // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                    "accept-encoding",
                    reqwest::header::HeaderValue::from_static("identity"),
                )
                .build()?;

            // Execute the request over the network.
            let response = client
                .uncached_client()
                .execute(req)
                .await?
                .error_for_status()?;

            // Download and unzip the source distribution into a temporary directory.
            let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::BuiltWheels))
                .map_err(Error::CacheWrite)?;
            let reader = response
                .bytes_stream()
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                .into_async_read();
            uv_extract::stream::archive(reader.compat(), filename.as_ref(), temp_dir.path())
                .await?;

            // Extract the top-level directory.
            let extracted = match uv_extract::strip_component(temp_dir.path()) {
                Ok(top_level) => top_level,
                Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
                Err(err) => return Err(err.into()),
            };

            Ok(ExtractedSource::Archive(extracted, temp_dir))
        }
        // Ex) `../editable/`
        None => {
            let path = url.to_file_path().expect("URL to be a file path");
            extract_archive(&path, cache).await
        }
        // Ex) `bzr+https://launchpad.net/bzr/+download/...`
        Some(scheme) => Err(Error::UnsupportedScheme(scheme.to_string())),
    }
}
