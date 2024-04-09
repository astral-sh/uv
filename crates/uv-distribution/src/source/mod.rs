//! Fetch and build source distributions from remote sources.

use std::borrow::Cow;
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
    BuildableSource, DirectArchiveUrl, Dist, FileLocation, GitSourceUrl, LocalEditable,
    PathSourceDist, PathSourceUrl, RemoteSource, SourceDist, SourceUrl,
};
use install_wheel_rs::metadata::read_archive_metadata;
use platform_tags::Tags;
use pypi_types::Metadata23;
use uv_cache::{
    ArchiveTimestamp, CacheBucket, CacheEntry, CacheShard, CachedByTimestamp, Freshness, WheelCache,
};
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_fs::write_atomic;
use uv_types::{BuildContext, BuildKind, NoBuild, SourceBuildTrait};

use crate::error::Error;
use crate::git::{fetch_git_archive, resolve_precise};
use crate::source::built_wheel_metadata::BuiltWheelMetadata;
use crate::source::revision::Revision;
use crate::Reporter;

mod built_wheel_metadata;
mod revision;

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub struct SourceDistributionBuilder<'a, T: BuildContext> {
    client: &'a RegistryClient,
    build_context: &'a T,
    reporter: Option<Arc<dyn Reporter>>,
}

/// The name of the file that contains the revision ID, encoded via `MsgPack`.
///
/// TODO(charlie): Update the filename whenever we bump the cache version.
pub(crate) const REVISION: &str = "manifest.msgpack";

/// The name of the file that contains the cached distribution metadata, encoded via `MsgPack`.
pub(crate) const METADATA: &str = "metadata.msgpack";

impl<'a, T: BuildContext> SourceDistributionBuilder<'a, T> {
    /// Initialize a [`SourceDistributionBuilder`] from a [`BuildContext`].
    pub fn new(client: &'a RegistryClient, build_context: &'a T) -> Self {
        Self {
            client,
            build_context,
            reporter: None,
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
                        return self
                            .archive(
                                source,
                                &PathSourceUrl {
                                    url: &url,
                                    path: Cow::Borrowed(path),
                                },
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
                if dist.path.is_dir() {
                    self.source_tree(source, &PathSourceUrl::from(dist), tags)
                        .boxed()
                        .await?
                } else {
                    self.archive(source, &PathSourceUrl::from(dist), tags)
                        .boxed()
                        .await?
                }
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
                if resource.path.is_dir() {
                    self.source_tree(source, resource, tags).boxed().await?
                } else {
                    self.archive(source, resource, tags).boxed().await?
                }
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
                        return self
                            .archive_metadata(
                                source,
                                &PathSourceUrl {
                                    url: &url,
                                    path: Cow::Borrowed(path),
                                },
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
                if dist.path.is_dir() {
                    self.source_tree_metadata(source, &PathSourceUrl::from(dist))
                        .boxed()
                        .await?
                } else {
                    self.archive_metadata(source, &PathSourceUrl::from(dist))
                        .boxed()
                        .await?
                }
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
                if resource.path.is_dir() {
                    self.source_tree_metadata(source, resource).boxed().await?
                } else {
                    self.archive_metadata(source, resource).boxed().await?
                }
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
        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, filename, url, cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

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
        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, filename, url, cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

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

    /// Return the [`Revision`] for a remote URL, refreshing it if necessary.
    async fn url_revision(
        &self,
        source: &BuildableSource<'_>,
        filename: &str,
        url: &Url,
        cache_shard: &CacheShard,
    ) -> Result<Revision, Error> {
        let cache_entry = cache_shard.entry(REVISION);
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
                // new revision, to collect the source and built artifacts.
                let revision = Revision::new();

                // Download the source distribution.
                debug!("Downloading source distribution: {source}");
                let source_dist_entry = cache_shard.shard(revision.id()).entry(filename);
                self.persist_url(response, source, filename, &source_dist_entry)
                    .await?;

                Ok(revision)
            }
            .boxed()
            .instrument(info_span!("download", source_dist = %source))
        };
        let req = self.request(url.clone())?;
        self.client
            .cached_client()
            .get_serde(req, &cache_entry, cache_control, download)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })
    }

    /// Build a source distribution from a local archive (e.g., `.tar.gz` or `.zip`).
    async fn archive(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Fetch the revision for the source distribution.
        let revision = self
            .archive_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel);
        }

        let source_entry = cache_shard.entry("source");

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(source, source_entry.path(), None, &cache_shard)
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

    /// Build the source distribution's metadata from a local archive (e.g., `.tar.gz` or `.zip`).
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn archive_metadata(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
    ) -> Result<Metadata23, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Fetch the revision for the source distribution.
        let revision = self
            .archive_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
            debug!("Using cached metadata for: {source}");
            return Ok(metadata);
        }

        let source_entry = cache_shard.entry("source");

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_entry.path(), None)
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
            .build_distribution(source, source_entry.path(), None, &cache_shard)
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

    /// Return the [`Revision`] for a local archive, refreshing it if necessary.
    async fn archive_revision(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        cache_shard: &CacheShard,
    ) -> Result<Revision, Error> {
        // Determine the last-modified time of the source distribution.
        let modified = ArchiveTimestamp::from_file(&resource.path).map_err(Error::CacheRead)?;

        // Read the existing metadata from the cache.
        let revision_entry = cache_shard.entry(REVISION);

        // If the revision already exists, return it. There's no need to check for freshness, since
        // we use an exact timestamp.
        if let Some(revision) = read_timestamped_revision(&revision_entry, modified)? {
            return Ok(revision);
        }

        // Otherwise, we need to create a new revision.
        let revision = Revision::new();

        // Unzip the archive to a temporary directory.
        debug!("Unpacking source distribution: {source}");
        let entry = cache_shard.shard(revision.id()).entry("source");
        self.persist_archive(&resource.path, source, &entry).await?;

        // Persist the revision.
        write_atomic(
            revision_entry.path(),
            rmp_serde::to_vec(&CachedByTimestamp {
                timestamp: modified.timestamp(),
                data: revision.clone(),
            })?,
        )
        .await
        .map_err(Error::CacheWrite)?;

        Ok(revision)
    }

    /// Build a source distribution from a local source tree (i.e., directory).
    async fn source_tree(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Fetch the revision for the source distribution.
        let revision = self
            .source_tree_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

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
            .build_distribution(source, &resource.path, None, &cache_shard)
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

    /// Build the source distribution's metadata from a local source tree (i.e., a directory).
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn source_tree_metadata(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
    ) -> Result<Metadata23, Error> {
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(resource.url).root(),
        );

        // Fetch the revision for the source distribution.
        let revision = self
            .source_tree_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

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
            .build_metadata(source, &resource.path, None)
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
            .build_distribution(source, &resource.path, None, &cache_shard)
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

    /// Return the [`Revision`] for a local source tree, refreshing it if necessary.
    async fn source_tree_revision(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        cache_shard: &CacheShard,
    ) -> Result<Revision, Error> {
        // Determine the last-modified time of the source distribution.
        let Some(modified) =
            ArchiveTimestamp::from_path(&resource.path).map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the existing metadata from the cache.
        let revision_entry = cache_shard.entry(REVISION);
        let revision_freshness = self
            .build_context
            .cache()
            .freshness(&revision_entry, source.name())
            .map_err(Error::CacheRead)?;

        refresh_timestamped_revision(&revision_entry, revision_freshness, modified).await
    }

    /// Build a source distribution from a Git repository.
    async fn git(
        &self,
        source: &BuildableSource<'_>,
        resource: &GitSourceUrl<'_>,
        tags: &Tags,
    ) -> Result<BuiltWheelMetadata, Error> {
        // Resolve to a precise Git SHA.
        let url = if let Some(url) = resolve_precise(
            resource.url,
            self.build_context.cache(),
            self.reporter.as_ref(),
        )
        .await?
        {
            Cow::Owned(url)
        } else {
            Cow::Borrowed(resource.url)
        };

        // Fetch the Git repository.
        let (fetch, subdirectory) =
            fetch_git_archive(&url, self.build_context.cache(), self.reporter.as_ref()).await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&url, &git_sha.to_short_string()).root(),
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
        // Resolve to a precise Git SHA.
        let url = if let Some(url) = resolve_precise(
            resource.url,
            self.build_context.cache(),
            self.reporter.as_ref(),
        )
        .await?
        {
            Cow::Owned(url)
        } else {
            Cow::Borrowed(resource.url)
        };

        // Fetch the Git repository.
        let (fetch, subdirectory) =
            fetch_git_archive(&url, self.build_context.cache(), self.reporter.as_ref()).await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&url, &git_sha.to_short_string()).root(),
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
    async fn persist_url(
        &self,
        response: Response,
        source: &BuildableSource<'_>,
        filename: &str,
        cache_entry: &CacheEntry,
    ) -> Result<(), Error> {
        let cache_path = cache_entry.path();
        if cache_path.is_dir() {
            debug!("Distribution is already cached: {source}");
            return Ok(());
        }

        // Download and unzip the source distribution into a temporary directory.
        let span = info_span!("persist_url", filename = filename, source_dist = %source);
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

        Ok(())
    }

    /// Extract a local archive, and store it at the given [`CacheEntry`].
    async fn persist_archive(
        &self,
        path: &Path,
        source: &BuildableSource<'_>,
        cache_entry: &CacheEntry,
    ) -> Result<(), Error> {
        let cache_path = cache_entry.path();
        if cache_path.is_dir() {
            debug!("Distribution is already cached: {source}");
            return Ok(());
        }

        debug!("Unpacking for build: {}", path.display());

        // Unzip the archive into a temporary directory.
        let temp_dir =
            tempfile::tempdir_in(self.build_context.cache().bucket(CacheBucket::BuiltWheels))
                .map_err(Error::CacheWrite)?;
        let reader = fs_err::tokio::File::open(&path)
            .await
            .map_err(Error::CacheRead)?;
        uv_extract::seek::archive(reader, path, &temp_dir.path()).await?;

        // Extract the top-level directory from the archive.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
            Err(err) => return Err(err.into()),
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(cache_path.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        fs_err::tokio::rename(extracted, &cache_path)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(())
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
        match read_pkg_info(source_root, subdirectory).await {
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
        match read_pyproject_toml(source_root, subdirectory).await {
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

    /// Returns a GET [`reqwest::Request`] for the given URL.
    fn request(&self, url: Url) -> Result<reqwest::Request, reqwest::Error> {
        self.client
            .uncached_client()
            .get(url)
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
            .build()
    }
}

/// Read an existing HTTP-cached [`Revision`], if it exists.
pub(crate) fn read_http_revision(cache_entry: &CacheEntry) -> Result<Option<Revision>, Error> {
    match fs_err::File::open(cache_entry.path()) {
        Ok(file) => {
            let data = DataWithCachePolicy::from_reader(file)?.data;
            Ok(Some(rmp_serde::from_slice::<Revision>(&data)?))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::CacheRead(err)),
    }
}

/// Read an existing timestamped [`Revision`], if it exists and is up-to-date.
///
/// If the cache entry is stale, a new entry will be created.
pub(crate) fn read_timestamped_revision(
    cache_entry: &CacheEntry,
    modified: ArchiveTimestamp,
) -> Result<Option<Revision>, Error> {
    // If the cache entry is up-to-date, return it.
    match fs_err::read(cache_entry.path()) {
        Ok(cached) => {
            let cached = rmp_serde::from_slice::<CachedByTimestamp<Revision>>(&cached)?;
            if cached.timestamp == modified.timestamp() {
                return Ok(Some(cached.data));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(Error::CacheRead(err)),
    }
    Ok(None)
}

/// Read the [`Metadata23`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
/// or later _and_ none of the required fields (`Requires-Python`, `Requires-Dist`, and
/// `Provides-Extra`) are marked as dynamic.
async fn read_pkg_info(
    source_tree: &Path,
    subdirectory: Option<&Path>,
) -> Result<Metadata23, Error> {
    // Read the `PKG-INFO` file.
    let pkg_info = match subdirectory {
        Some(subdirectory) => source_tree.join(subdirectory).join("PKG-INFO"),
        None => source_tree.join("PKG-INFO"),
    };
    let content = match fs::read(pkg_info).await {
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
async fn read_pyproject_toml(
    source_tree: &Path,
    subdirectory: Option<&Path>,
) -> Result<Metadata23, Error> {
    // Read the `pyproject.toml` file.
    let pyproject_toml = match subdirectory {
        Some(subdirectory) => source_tree.join(subdirectory).join("pyproject.toml"),
        None => source_tree.join("pyproject.toml"),
    };
    let content = match fs::read_to_string(pyproject_toml).await {
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

/// Read an existing timestamped [`Manifest`], if it exists and is up-to-date.
///
/// If the cache entry is stale, a new entry will be created.
async fn refresh_timestamped_revision(
    cache_entry: &CacheEntry,
    freshness: Freshness,
    modified: ArchiveTimestamp,
) -> Result<Revision, Error> {
    // If we know the exact modification time, we don't need to force a revalidate.
    if matches!(modified, ArchiveTimestamp::Exact(_)) || freshness.is_fresh() {
        if let Some(revision) = read_timestamped_revision(cache_entry, modified)? {
            return Ok(revision);
        }
    }

    // Otherwise, create a new revision.
    let revision = Revision::new();
    fs::create_dir_all(&cache_entry.dir())
        .await
        .map_err(Error::CacheWrite)?;
    write_atomic(
        cache_entry.path(),
        rmp_serde::to_vec(&CachedByTimestamp {
            timestamp: modified.timestamp(),
            data: revision.clone(),
        })?,
    )
    .await
    .map_err(Error::CacheWrite)?;
    Ok(revision)
}

/// Read an existing cached [`Metadata23`], if it exists.
async fn read_cached_metadata(cache_entry: &CacheEntry) -> Result<Option<Metadata23>, Error> {
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
