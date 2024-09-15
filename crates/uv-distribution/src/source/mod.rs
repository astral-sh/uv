//! Fetch and build source distributions from remote sources.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use crate::distribution_database::ManagedClient;
use crate::error::Error;
use crate::metadata::{ArchiveMetadata, Metadata};
use crate::reporter::Facade;
use crate::source::built_wheel_metadata::BuiltWheelMetadata;
use crate::source::revision::Revision;
use crate::{Reporter, RequiresDist};
use distribution_filename::{SourceDistExtension, WheelFilename};
use distribution_types::{
    BuildableSource, DirectorySourceUrl, FileLocation, GitSourceUrl, HashPolicy, Hashed,
    PathSourceUrl, RemoteSource, SourceDist, SourceUrl,
};
use fs_err::tokio as fs;
use futures::{FutureExt, TryStreamExt};
use platform_tags::Tags;
use pypi_types::{HashDigest, Metadata12, Metadata23, RequiresTxt};
use reqwest::Response;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, Instrument};
use url::Url;
use uv_cache::{Cache, CacheBucket, CacheEntry, CacheShard, Removal, WheelCache};
use uv_cache_info::CacheInfo;
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_configuration::{BuildKind, BuildOutput};
use uv_extract::hash::Hasher;
use uv_fs::{rename_with_retry, write_atomic, LockedFile};
use uv_metadata::read_archive_metadata;
use uv_types::{BuildContext, SourceBuildTrait};
use zip::ZipArchive;

mod built_wheel_metadata;
mod revision;

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub(crate) struct SourceDistributionBuilder<'a, T: BuildContext> {
    build_context: &'a T,
    reporter: Option<Arc<dyn Reporter>>,
}

/// The name of the file that contains the revision ID for a remote distribution, encoded via `MsgPack`.
pub(crate) const HTTP_REVISION: &str = "revision.http";

/// The name of the file that contains the revision ID for a local distribution, encoded via `MsgPack`.
pub(crate) const LOCAL_REVISION: &str = "revision.rev";

/// The name of the file that contains the cached distribution metadata, encoded via `MsgPack`.
pub(crate) const METADATA: &str = "metadata.msgpack";

impl<'a, T: BuildContext> SourceDistributionBuilder<'a, T> {
    /// Initialize a [`SourceDistributionBuilder`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub(crate) fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Download and build a [`SourceDist`].
    pub(crate) async fn download_and_build(
        &self,
        source: &BuildableSource<'_>,
        tags: &Tags,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        let built_wheel_metadata = match &source {
            BuildableSource::Dist(SourceDist::Registry(dist)) => {
                // For registry source distributions, shard by package, then version, for
                // convenience in debugging.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Index(&dist.index)
                        .wheel_dir(dist.name.as_ref())
                        .join(dist.version.to_string()),
                );

                let url = match &dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => url.to_url(),
                };

                // If the URL is a file URL, use the local path directly.
                if url.scheme() == "file" {
                    let path = url
                        .to_file_path()
                        .map_err(|()| Error::NonFileUrl(url.clone()))?;
                    return self
                        .archive(
                            source,
                            &PathSourceUrl {
                                url: &url,
                                path: Cow::Owned(path),
                                ext: dist.ext,
                            },
                            &cache_shard,
                            tags,
                            hashes,
                        )
                        .boxed_local()
                        .await;
                }

                self.url(
                    source,
                    &dist.file.filename,
                    &url,
                    &cache_shard,
                    None,
                    dist.ext,
                    tags,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => {
                let filename = dist.filename().expect("Distribution must have a filename");

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(&dist.url).root(),
                );

                self.url(
                    source,
                    &filename,
                    &dist.url,
                    &cache_shard,
                    dist.subdirectory.as_deref(),
                    dist.ext,
                    tags,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Dist(SourceDist::Git(dist)) => {
                self.git(source, &GitSourceUrl::from(dist), tags, hashes, client)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Directory(dist)) => {
                self.source_tree(source, &DirectorySourceUrl::from(dist), tags, hashes)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Path(dist)) => {
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Path(&dist.url).root(),
                );
                self.archive(
                    source,
                    &PathSourceUrl::from(dist),
                    &cache_shard,
                    tags,
                    hashes,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Url(SourceUrl::Direct(resource)) => {
                let filename = resource
                    .url
                    .filename()
                    .expect("Distribution must have a filename");

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(resource.url).root(),
                );

                self.url(
                    source,
                    &filename,
                    resource.url,
                    &cache_shard,
                    resource.subdirectory,
                    resource.ext,
                    tags,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Url(SourceUrl::Git(resource)) => {
                self.git(source, resource, tags, hashes, client)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Directory(resource)) => {
                self.source_tree(source, resource, tags, hashes)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Path(resource)) => {
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Path(resource.url).root(),
                );
                self.archive(source, resource, &cache_shard, tags, hashes)
                    .boxed_local()
                    .await?
            }
        };

        Ok(built_wheel_metadata)
    }

    /// Download a [`SourceDist`] and determine its metadata. This typically involves building the
    /// source distribution into a wheel; however, some build backends support determining the
    /// metadata without building the source distribution.
    pub(crate) async fn download_and_build_metadata(
        &self,
        source: &BuildableSource<'_>,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        let metadata = match &source {
            BuildableSource::Dist(SourceDist::Registry(dist)) => {
                // For registry source distributions, shard by package, then version.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Index(&dist.index)
                        .wheel_dir(dist.name.as_ref())
                        .join(dist.version.to_string()),
                );

                let url = match &dist.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => url.to_url(),
                };

                // If the URL is a file URL, use the local path directly.
                if url.scheme() == "file" {
                    let path = url
                        .to_file_path()
                        .map_err(|()| Error::NonFileUrl(url.clone()))?;
                    return self
                        .archive_metadata(
                            source,
                            &PathSourceUrl {
                                url: &url,
                                path: Cow::Owned(path),
                                ext: dist.ext,
                            },
                            &cache_shard,
                            hashes,
                        )
                        .boxed_local()
                        .await;
                }

                self.url_metadata(
                    source,
                    &dist.file.filename,
                    &url,
                    &cache_shard,
                    None,
                    dist.ext,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => {
                let filename = dist.filename().expect("Distribution must have a filename");

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(&dist.url).root(),
                );

                self.url_metadata(
                    source,
                    &filename,
                    &dist.url,
                    &cache_shard,
                    dist.subdirectory.as_deref(),
                    dist.ext,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Dist(SourceDist::Git(dist)) => {
                self.git_metadata(source, &GitSourceUrl::from(dist), hashes, client)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Directory(dist)) => {
                self.source_tree_metadata(source, &DirectorySourceUrl::from(dist), hashes)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Dist(SourceDist::Path(dist)) => {
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Path(&dist.url).root(),
                );
                self.archive_metadata(source, &PathSourceUrl::from(dist), &cache_shard, hashes)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Direct(resource)) => {
                let filename = resource
                    .url
                    .filename()
                    .expect("Distribution must have a filename");

                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(resource.url).root(),
                );

                self.url_metadata(
                    source,
                    &filename,
                    resource.url,
                    &cache_shard,
                    resource.subdirectory,
                    resource.ext,
                    hashes,
                    client,
                )
                .boxed_local()
                .await?
            }
            BuildableSource::Url(SourceUrl::Git(resource)) => {
                self.git_metadata(source, resource, hashes, client)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Directory(resource)) => {
                self.source_tree_metadata(source, resource, hashes)
                    .boxed_local()
                    .await?
            }
            BuildableSource::Url(SourceUrl::Path(resource)) => {
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Path(resource.url).root(),
                );
                self.archive_metadata(source, resource, &cache_shard, hashes)
                    .boxed_local()
                    .await?
            }
        };

        Ok(metadata)
    }

    /// Return the [`RequiresDist`] from a `pyproject.toml`, if it can be statically extracted.
    pub(crate) async fn requires_dist(&self, project_root: &Path) -> Result<RequiresDist, Error> {
        let requires_dist = read_requires_dist(project_root).await?;
        let requires_dist = RequiresDist::from_project_maybe_workspace(
            requires_dist,
            project_root,
            self.build_context.sources(),
        )
        .await?;
        Ok(requires_dist)
    }

    /// Build a source distribution from a remote URL.
    async fn url<'data>(
        &self,
        source: &BuildableSource<'data>,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
        ext: SourceDistExtension,
        tags: &Tags,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        let _lock = lock_shard(cache_shard).await?;

        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, filename, ext, url, cache_shard, hashes, client)
            .await?;

        // Before running the build, check that the hashes match.
        if !revision.satisfies(hashes) {
            return Err(Error::hash_mismatch(
                source.to_string(),
                hashes.digests(),
                revision.hashes(),
            ));
        }

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel.with_hashes(revision.into_hashes()));
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
            hashes: revision.into_hashes(),
            cache_info: CacheInfo::default(),
        })
    }

    /// Build the source distribution's metadata from a local path.
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn url_metadata<'data>(
        &self,
        source: &BuildableSource<'data>,
        filename: &'data str,
        url: &'data Url,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
        ext: SourceDistExtension,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        let _lock = lock_shard(cache_shard).await?;

        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, filename, ext, url, cache_shard, hashes, client)
            .await?;

        // Before running the build, check that the hashes match.
        if !revision.satisfies(hashes) {
            return Err(Error::hash_mismatch(
                source.to_string(),
                hashes.digests(),
                revision.hashes(),
            ));
        }

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());
        let source_dist_entry = cache_shard.entry(filename);

        // If the metadata is static, return it.
        if let Some(metadata) =
            Self::read_static_metadata(source, source_dist_entry.path(), subdirectory).await?
        {
            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
        }

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
            debug!("Using cached metadata for: {source}");
            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
        }

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

        // Otherwise, we either need to build the metadata.
        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_dist_entry.path(), subdirectory)
            .boxed_local()
            .await?
        {
            // Store the metadata.
            fs::create_dir_all(metadata_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
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
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        Ok(ArchiveMetadata {
            metadata: Metadata::from_metadata23(metadata),
            hashes: revision.into_hashes(),
        })
    }

    /// Return the [`Revision`] for a remote URL, refreshing it if necessary.
    async fn url_revision(
        &self,
        source: &BuildableSource<'_>,
        filename: &str,
        ext: SourceDistExtension,
        url: &Url,
        cache_shard: &CacheShard,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<Revision, Error> {
        let cache_entry = cache_shard.entry(HTTP_REVISION);
        let cache_control = match client.unmanaged.connectivity() {
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
                let entry = cache_shard.shard(revision.id()).entry(filename);
                let hashes = self
                    .download_archive(response, source, filename, ext, entry.path(), hashes)
                    .await?;

                Ok(revision.with_hashes(hashes))
            }
            .boxed_local()
            .instrument(info_span!("download", source_dist = %source))
        };
        let req = Self::request(url.clone(), client.unmanaged)?;
        let revision = client
            .managed(|client| {
                client
                    .cached_client()
                    .get_serde(req, &cache_entry, cache_control, download)
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, force a refresh.
        if revision.has_digests(hashes) {
            Ok(revision)
        } else {
            client
                .managed(|client| async move {
                    client
                        .cached_client()
                        .skip_cache(Self::request(url.clone(), client)?, &cache_entry, download)
                        .await
                        .map_err(|err| match err {
                            CachedClientError::Callback(err) => err,
                            CachedClientError::Client(err) => Error::Client(err),
                        })
                })
                .await
        }
    }

    /// Build a source distribution from a local archive (e.g., `.tar.gz` or `.zip`).
    async fn archive(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        cache_shard: &CacheShard,
        tags: &Tags,
        hashes: HashPolicy<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        let _lock = lock_shard(cache_shard).await?;

        // Fetch the revision for the source distribution.
        let LocalRevisionPointer {
            cache_info,
            revision,
        } = self
            .archive_revision(source, resource, cache_shard, hashes)
            .await?;

        // Before running the build, check that the hashes match.
        if !revision.satisfies(hashes) {
            return Err(Error::hash_mismatch(
                source.to_string(),
                hashes.digests(),
                revision.hashes(),
            ));
        }

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

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
            hashes: revision.into_hashes(),
            cache_info,
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
        cache_shard: &CacheShard,
        hashes: HashPolicy<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        let _lock = lock_shard(cache_shard).await?;

        // Fetch the revision for the source distribution.
        let LocalRevisionPointer { revision, .. } = self
            .archive_revision(source, resource, cache_shard, hashes)
            .await?;

        // Before running the build, check that the hashes match.
        if !revision.satisfies(hashes) {
            return Err(Error::hash_mismatch(
                source.to_string(),
                hashes.digests(),
                revision.hashes(),
            ));
        }

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());
        let source_entry = cache_shard.entry("source");

        // If the metadata is static, return it.
        if let Some(metadata) =
            Self::read_static_metadata(source, source_entry.path(), None).await?
        {
            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
        }

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
            debug!("Using cached metadata for: {source}");
            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_entry.path(), None)
            .boxed_local()
            .await?
        {
            // Store the metadata.
            fs::create_dir_all(metadata_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes: revision.into_hashes(),
            });
        }

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

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
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(ArchiveMetadata {
            metadata: Metadata::from_metadata23(metadata),
            hashes: revision.into_hashes(),
        })
    }

    /// Return the [`Revision`] for a local archive, refreshing it if necessary.
    async fn archive_revision(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        cache_shard: &CacheShard,
        hashes: HashPolicy<'_>,
    ) -> Result<LocalRevisionPointer, Error> {
        // Verify that the archive exists.
        if !resource.path.is_file() {
            return Err(Error::NotFound(resource.url.clone()));
        }

        // Determine the last-modified time of the source distribution.
        let cache_info = CacheInfo::from_file(&resource.path).map_err(Error::CacheRead)?;

        // Read the existing metadata from the cache.
        let revision_entry = cache_shard.entry(LOCAL_REVISION);

        // If the revision already exists, return it. There's no need to check for freshness, since
        // we use an exact timestamp.
        if let Some(pointer) = LocalRevisionPointer::read_from(&revision_entry)? {
            if *pointer.cache_info() == cache_info {
                if pointer.revision().has_digests(hashes) {
                    return Ok(pointer);
                }
            }
        }

        // Otherwise, we need to create a new revision.
        let revision = Revision::new();

        // Unzip the archive to a temporary directory.
        debug!("Unpacking source distribution: {source}");
        let entry = cache_shard.shard(revision.id()).entry("source");
        let hashes = self
            .persist_archive(&resource.path, resource.ext, entry.path(), hashes)
            .await?;

        // Include the hashes and cache info in the revision.
        let revision = revision.with_hashes(hashes);

        // Persist the revision.
        let pointer = LocalRevisionPointer {
            cache_info,
            revision,
        };
        pointer.write_to(&revision_entry).await?;

        Ok(pointer)
    }

    /// Build a source distribution from a local source tree (i.e., directory), either editable or
    /// non-editable.
    async fn source_tree(
        &self,
        source: &BuildableSource<'_>,
        resource: &DirectorySourceUrl<'_>,
        tags: &Tags,
        hashes: HashPolicy<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        // Before running the build, check that the hashes match.
        if hashes.is_validate() {
            return Err(Error::HashesNotSupportedSourceTree(source.to_string()));
        }

        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            if resource.editable {
                WheelCache::Editable(resource.url).root()
            } else {
                WheelCache::Path(resource.url).root()
            },
        );

        let _lock = lock_shard(&cache_shard).await?;

        // Fetch the revision for the source distribution.
        let LocalRevisionPointer {
            cache_info,
            revision,
        } = self
            .source_tree_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

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
            .build_distribution(source, &resource.install_path, None, &cache_shard)
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
            hashes: revision.into_hashes(),
            cache_info,
        })
    }

    /// Build the source distribution's metadata from a local source tree (i.e., a directory),
    /// either editable or non-editable.
    ///
    /// If the build backend supports `prepare_metadata_for_build_wheel`, this method will avoid
    /// building the wheel.
    async fn source_tree_metadata(
        &self,
        source: &BuildableSource<'_>,
        resource: &DirectorySourceUrl<'_>,
        hashes: HashPolicy<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        // Before running the build, check that the hashes match.
        if hashes.is_validate() {
            return Err(Error::HashesNotSupportedSourceTree(source.to_string()));
        }

        if let Some(metadata) =
            Self::read_static_metadata(source, &resource.install_path, None).await?
        {
            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(
                    metadata,
                    resource.install_path.as_ref(),
                    self.build_context.sources(),
                )
                .await?,
            ));
        }

        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            if resource.editable {
                WheelCache::Editable(resource.url).root()
            } else {
                WheelCache::Path(resource.url).root()
            },
        );

        let _lock = lock_shard(&cache_shard).await?;

        // Fetch the revision for the source distribution.
        let LocalRevisionPointer { revision, .. } = self
            .source_tree_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
            debug!("Using cached metadata for: {source}");
            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(
                    metadata,
                    resource.install_path.as_ref(),
                    self.build_context.sources(),
                )
                .await?,
            ));
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, &resource.install_path, None)
            .boxed_local()
            .await?
        {
            // Store the metadata.
            fs::create_dir_all(metadata_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(
                    metadata,
                    resource.install_path.as_ref(),
                    self.build_context.sources(),
                )
                .await?,
            ));
        }

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(source, &resource.install_path, None, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // Store the metadata.
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(ArchiveMetadata::from(
            Metadata::from_workspace(
                metadata,
                resource.install_path.as_ref(),
                self.build_context.sources(),
            )
            .await?,
        ))
    }

    /// Return the [`Revision`] for a local source tree, refreshing it if necessary.
    async fn source_tree_revision(
        &self,
        source: &BuildableSource<'_>,
        resource: &DirectorySourceUrl<'_>,
        cache_shard: &CacheShard,
    ) -> Result<LocalRevisionPointer, Error> {
        // Verify that the source tree exists.
        if !resource.install_path.is_dir() {
            return Err(Error::NotFound(resource.url.clone()));
        }

        // Determine the last-modified time of the source distribution.
        let cache_info = CacheInfo::from_directory(&resource.install_path)?;

        // Read the existing metadata from the cache.
        let entry = cache_shard.entry(LOCAL_REVISION);

        // If the revision is fresh, return it.
        if self
            .build_context
            .cache()
            .freshness(&entry, source.name())
            .map_err(Error::CacheRead)?
            .is_fresh()
        {
            if let Some(pointer) = LocalRevisionPointer::read_from(&entry)? {
                if *pointer.cache_info() == cache_info {
                    return Ok(pointer);
                }
            }
        }

        // Otherwise, we need to create a new revision.
        let revision = Revision::new();
        let pointer = LocalRevisionPointer {
            cache_info,
            revision,
        };
        pointer.write_to(&entry).await?;

        Ok(pointer)
    }

    /// Build a source distribution from a Git repository.
    async fn git(
        &self,
        source: &BuildableSource<'_>,
        resource: &GitSourceUrl<'_>,
        tags: &Tags,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        // Before running the build, check that the hashes match.
        if hashes.is_validate() {
            return Err(Error::HashesNotSupportedGit(source.to_string()));
        }

        // Fetch the Git repository.
        let fetch = self
            .build_context
            .git()
            .fetch(
                resource.git,
                client.unmanaged.uncached_client(resource.url).clone(),
                self.build_context.cache().bucket(CacheBucket::Git),
                self.reporter.clone().map(Facade::from),
            )
            .await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            WheelCache::Git(resource.url, &git_sha.to_short_string()).root(),
        );
        let metadata_entry = cache_shard.entry(METADATA);

        let _lock = lock_shard(&cache_shard).await?;

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard) {
            return Ok(built_wheel);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(source, fetch.path(), resource.subdirectory, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // Store the metadata.
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(BuiltWheelMetadata {
            path: cache_shard.join(&disk_filename),
            target: cache_shard.join(filename.stem()),
            filename,
            hashes: vec![],
            cache_info: CacheInfo::default(),
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
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        // Before running the build, check that the hashes match.
        if hashes.is_validate() {
            return Err(Error::HashesNotSupportedGit(source.to_string()));
        }

        // Fetch the Git repository.
        let fetch = self
            .build_context
            .git()
            .fetch(
                resource.git,
                client.unmanaged.uncached_client(resource.url).clone(),
                self.build_context.cache().bucket(CacheBucket::Git),
                self.reporter.clone().map(Facade::from),
            )
            .await?;

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            WheelCache::Git(resource.url, &git_sha.to_short_string()).root(),
        );
        let metadata_entry = cache_shard.entry(METADATA);

        let _lock = lock_shard(&cache_shard).await?;

        let path = if let Some(subdirectory) = resource.subdirectory {
            Cow::Owned(fetch.path().join(subdirectory))
        } else {
            Cow::Borrowed(fetch.path())
        };

        if let Some(metadata) =
            Self::read_static_metadata(source, fetch.path(), resource.subdirectory).await?
        {
            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(metadata, &path, self.build_context.sources()).await?,
            ));
        }

        // If the cache contains compatible metadata, return it.
        if self
            .build_context
            .cache()
            .freshness(&metadata_entry, source.name())
            .map_err(Error::CacheRead)?
            .is_fresh()
        {
            if let Some(metadata) = read_cached_metadata(&metadata_entry).await? {
                let path = if let Some(subdirectory) = resource.subdirectory {
                    Cow::Owned(fetch.path().join(subdirectory))
                } else {
                    Cow::Borrowed(fetch.path())
                };

                debug!("Using cached metadata for: {source}");
                return Ok(ArchiveMetadata::from(
                    Metadata::from_workspace(metadata, &path, self.build_context.sources()).await?,
                ));
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, fetch.path(), resource.subdirectory)
            .boxed_local()
            .await?
        {
            // Store the metadata.
            fs::create_dir_all(metadata_entry.dir())
                .await
                .map_err(Error::CacheWrite)?;
            write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
                .await
                .map_err(Error::CacheWrite)?;

            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(metadata, &path, self.build_context.sources()).await?,
            ));
        }

        // If there are build settings, we need to scope to a cache shard.
        let config_settings = self.build_context.config_settings();
        let cache_shard = if config_settings.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_key::cache_digest(config_settings))
        };

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(source, fetch.path(), resource.subdirectory, &cache_shard)
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // Store the metadata.
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(ArchiveMetadata::from(
            Metadata::from_workspace(metadata, fetch.path(), self.build_context.sources()).await?,
        ))
    }

    /// Download and unzip a source distribution into the cache from an HTTP response.
    async fn download_archive(
        &self,
        response: Response,
        source: &BuildableSource<'_>,
        filename: &str,
        ext: SourceDistExtension,
        target: &Path,
        hashes: HashPolicy<'_>,
    ) -> Result<Vec<HashDigest>, Error> {
        let temp_dir = tempfile::tempdir_in(
            self.build_context
                .cache()
                .bucket(CacheBucket::SourceDistributions),
        )
        .map_err(Error::CacheWrite)?;
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();

        // Create a hasher for each hash algorithm.
        let algorithms = hashes.algorithms();
        let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

        // Download and unzip the source distribution into a temporary directory.
        let span = info_span!("download_source_dist", filename = filename, source_dist = %source);
        uv_extract::stream::archive(&mut hasher, ext, temp_dir.path()).await?;
        drop(span);

        // If necessary, exhaust the reader to compute the hash.
        if !hashes.is_none() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }

        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.into_path(),
            Err(err) => return Err(err.into()),
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        rename_with_retry(extracted, target)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(hashes)
    }

    /// Extract a local archive, and store it at the given [`CacheEntry`].
    async fn persist_archive(
        &self,
        path: &Path,
        ext: SourceDistExtension,
        target: &Path,
        hashes: HashPolicy<'_>,
    ) -> Result<Vec<HashDigest>, Error> {
        debug!("Unpacking for build: {}", path.display());

        let temp_dir = tempfile::tempdir_in(
            self.build_context
                .cache()
                .bucket(CacheBucket::SourceDistributions),
        )
        .map_err(Error::CacheWrite)?;
        let reader = fs_err::tokio::File::open(&path)
            .await
            .map_err(Error::CacheRead)?;

        // Create a hasher for each hash algorithm.
        let algorithms = hashes.algorithms();
        let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        // Unzip the archive into a temporary directory.
        uv_extract::stream::archive(&mut hasher, ext, &temp_dir.path()).await?;

        // If necessary, exhaust the reader to compute the hash.
        if !hashes.is_none() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }

        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        // Extract the top-level directory from the archive.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
            Err(err) => return Err(err.into()),
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        rename_with_retry(extracted, &target)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(hashes)
    }

    /// Build a source distribution, storing the built wheel in the cache.
    ///
    /// Returns the un-normalized disk filename, the parsed, normalized filename and the metadata
    #[instrument(skip_all, fields(dist = %source))]
    async fn build_distribution(
        &self,
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
        cache_shard: &CacheShard,
    ) -> Result<(String, WheelFilename, Metadata23), Error> {
        debug!("Building: {source}");

        // Guard against build of source distributions when disabled.
        if self
            .build_context
            .build_options()
            .no_build_requirement(source.name())
        {
            if source.is_editable() {
                debug!("Allowing build for editable source distribution: {source}");
            } else {
                return Err(Error::NoBuild);
            }
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
                Some(source.to_string()),
                source.as_dist(),
                if source.is_editable() {
                    BuildKind::Editable
                } else {
                    BuildKind::Wheel
                },
                BuildOutput::Debug,
            )
            .await
            .map_err(Error::Build)?
            .wheel(cache_shard)
            .await
            .map_err(Error::Build)?;

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_wheel_metadata(&filename, &cache_shard.join(&disk_filename))?;

        // Validate the metadata.
        validate(source, &metadata)?;

        debug!("Finished building: {source}");
        Ok((disk_filename, filename, metadata))
    }

    /// Build the metadata for a source distribution.
    #[instrument(skip_all, fields(dist = %source))]
    async fn build_metadata(
        &self,
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
    ) -> Result<Option<Metadata23>, Error> {
        debug!("Preparing metadata for: {source}");

        // Set up the builder.
        let mut builder = self
            .build_context
            .setup_build(
                source_root,
                subdirectory,
                Some(source.to_string()),
                source.as_dist(),
                if source.is_editable() {
                    BuildKind::Editable
                } else {
                    BuildKind::Wheel
                },
                BuildOutput::Debug,
            )
            .await
            .map_err(Error::Build)?;

        // Build the metadata.
        let dist_info = builder.metadata().await.map_err(Error::Build)?;
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
        validate(source, &metadata)?;

        Ok(Some(metadata))
    }

    async fn read_static_metadata(
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
    ) -> Result<Option<Metadata23>, Error> {
        // Attempt to read static metadata from the `pyproject.toml`.
        match read_pyproject_toml(source_root, subdirectory).await {
            Ok(metadata) => {
                debug!("Found static `pyproject.toml` for: {source}");

                // Validate the metadata.
                validate(source, &metadata)?;

                return Ok(Some(metadata));
            }
            Err(
                err @ (Error::MissingPyprojectToml
                | Error::PyprojectToml(
                    pypi_types::MetadataError::Pep508Error(_)
                    | pypi_types::MetadataError::DynamicField(_)
                    | pypi_types::MetadataError::FieldNotFound(_)
                    | pypi_types::MetadataError::PoetrySyntax,
                )),
            ) => {
                debug!("No static `pyproject.toml` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        // If the source distribution is a source tree, avoid reading `PKG-INFO` or `egg-info`,
        // since they could be out-of-date.
        if source.is_source_tree() {
            return Ok(None);
        }

        // Attempt to read static metadata from the `PKG-INFO` file.
        match read_pkg_info(source_root, subdirectory).await {
            Ok(metadata) => {
                debug!("Found static `PKG-INFO` for: {source}");

                // Validate the metadata.
                validate(source, &metadata)?;

                return Ok(Some(metadata));
            }
            Err(
                err @ (Error::MissingPkgInfo
                | Error::PkgInfo(
                    pypi_types::MetadataError::Pep508Error(_)
                    | pypi_types::MetadataError::DynamicField(_)
                    | pypi_types::MetadataError::FieldNotFound(_)
                    | pypi_types::MetadataError::UnsupportedMetadataVersion(_),
                )),
            ) => {
                debug!("No static `PKG-INFO` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        // Attempt to read static metadata from the `egg-info` directory.
        match read_egg_info(source_root, subdirectory).await {
            Ok(metadata) => {
                debug!("Found static `egg-info` for: {source}");

                // Validate the metadata.
                validate(source, &metadata)?;

                return Ok(Some(metadata));
            }
            Err(
                err @ (Error::MissingEggInfo
                | Error::MissingRequiresTxt
                | Error::MissingPkgInfo
                | Error::RequiresTxt(
                    pypi_types::MetadataError::Pep508Error(_)
                    | pypi_types::MetadataError::RequiresTxtContents(_),
                )
                | Error::PkgInfo(
                    pypi_types::MetadataError::Pep508Error(_)
                    | pypi_types::MetadataError::DynamicField(_)
                    | pypi_types::MetadataError::FieldNotFound(_)
                    | pypi_types::MetadataError::UnsupportedMetadataVersion(_),
                )),
            ) => {
                debug!("No static `egg-info` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        Ok(None)
    }

    /// Returns a GET [`reqwest::Request`] for the given URL.
    fn request(url: Url, client: &RegistryClient) -> Result<reqwest::Request, reqwest::Error> {
        client
            .uncached_client(&url)
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

/// Prune any unused source distributions from the cache.
pub fn prune(cache: &Cache) -> Result<Removal, Error> {
    let mut removal = Removal::default();

    let bucket = cache.bucket(CacheBucket::SourceDistributions);
    if bucket.is_dir() {
        for entry in walkdir::WalkDir::new(bucket) {
            let entry = entry.map_err(Error::CacheWalk)?;

            if !entry.file_type().is_dir() {
                continue;
            }

            // If we find a `revision.http` file, read the pointer, and remove any extraneous
            // directories.
            let revision = entry.path().join("revision.http");
            if revision.is_file() {
                let pointer = HttpRevisionPointer::read_from(revision)?;
                if let Some(pointer) = pointer {
                    // Remove all sibling directories that are not referenced by the pointer.
                    for sibling in entry.path().read_dir().map_err(Error::CacheRead)? {
                        let sibling = sibling.map_err(Error::CacheRead)?;
                        if sibling.file_type().map_err(Error::CacheRead)?.is_dir() {
                            let sibling_name = sibling.file_name();
                            if sibling_name != pointer.revision.id().as_str() {
                                debug!(
                                    "Removing dangling source revision: {}",
                                    sibling.path().display()
                                );
                                removal +=
                                    uv_cache::rm_rf(sibling.path()).map_err(Error::CacheWrite)?;
                            }
                        }
                    }
                }

                continue;
            }

            // If we find a `revision.rev` file, read the pointer, and remove any extraneous
            // directories.
            let revision = entry.path().join("revision.rev");
            if revision.is_file() {
                let pointer = LocalRevisionPointer::read_from(revision)?;
                if let Some(pointer) = pointer {
                    // Remove all sibling directories that are not referenced by the pointer.
                    for sibling in entry.path().read_dir().map_err(Error::CacheRead)? {
                        let sibling = sibling.map_err(Error::CacheRead)?;
                        if sibling.file_type().map_err(Error::CacheRead)?.is_dir() {
                            let sibling_name = sibling.file_name();
                            if sibling_name != pointer.revision.id().as_str() {
                                debug!(
                                    "Removing dangling source revision: {}",
                                    sibling.path().display()
                                );
                                removal +=
                                    uv_cache::rm_rf(sibling.path()).map_err(Error::CacheWrite)?;
                            }
                        }
                    }
                }

                continue;
            }
        }
    }

    Ok(removal)
}

/// Validate that the source distribution matches the built metadata.
fn validate(source: &BuildableSource<'_>, metadata: &Metadata23) -> Result<(), Error> {
    if let Some(name) = source.name() {
        if metadata.name != *name {
            return Err(Error::NameMismatch {
                metadata: metadata.name.clone(),
                given: name.clone(),
            });
        }
    }

    if let Some(version) = source.version() {
        if metadata.version != *version {
            return Err(Error::VersionMismatch {
                metadata: metadata.version.clone(),
                given: version.clone(),
            });
        }
    }

    Ok(())
}

/// A pointer to a source distribution revision in the cache, fetched from an HTTP archive.
///
/// Encoded with `MsgPack`, and represented on disk by a `.http` file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct HttpRevisionPointer {
    revision: Revision,
}

impl HttpRevisionPointer {
    /// Read an [`HttpRevisionPointer`] from the cache.
    pub(crate) fn read_from(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        match fs_err::File::open(path.as_ref()) {
            Ok(file) => {
                let data = DataWithCachePolicy::from_reader(file)?.data;
                let revision = rmp_serde::from_slice::<Revision>(&data)?;
                Ok(Some(Self { revision }))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Return the [`Revision`] from the pointer.
    pub(crate) fn into_revision(self) -> Revision {
        self.revision
    }
}

/// A pointer to a source distribution revision in the cache, fetched from a local path.
///
/// Encoded with `MsgPack`, and represented on disk by a `.rev` file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct LocalRevisionPointer {
    cache_info: CacheInfo,
    revision: Revision,
}

impl LocalRevisionPointer {
    /// Read an [`LocalRevisionPointer`] from the cache.
    pub(crate) fn read_from(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        match fs_err::read(path) {
            Ok(cached) => Ok(Some(rmp_serde::from_slice::<LocalRevisionPointer>(
                &cached,
            )?)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Write an [`LocalRevisionPointer`] to the cache.
    async fn write_to(&self, entry: &CacheEntry) -> Result<(), Error> {
        fs::create_dir_all(&entry.dir())
            .await
            .map_err(Error::CacheWrite)?;
        write_atomic(entry.path(), rmp_serde::to_vec(&self)?)
            .await
            .map_err(Error::CacheWrite)
    }

    /// Return the [`CacheInfo`] for the pointer.
    pub(crate) fn cache_info(&self) -> &CacheInfo {
        &self.cache_info
    }

    /// Return the [`Revision`] for the pointer.
    pub(crate) fn revision(&self) -> &Revision {
        &self.revision
    }

    /// Return the [`Revision`] for the pointer.
    pub(crate) fn into_revision(self) -> Revision {
        self.revision
    }
}

/// Read the [`Metadata23`] by combining a source distribution's `PKG-INFO` file with a
/// `requires.txt`.
///
/// `requires.txt` is a legacy concept from setuptools. For example, here's
/// `Flask.egg-info/requires.txt` from Flask's 1.0 release:
///
/// ```txt
/// Werkzeug>=0.14
/// Jinja2>=2.10
/// itsdangerous>=0.24
/// click>=5.1
///
/// [dev]
/// pytest>=3
/// coverage
/// tox
/// sphinx
/// pallets-sphinx-themes
/// sphinxcontrib-log-cabinet
///
/// [docs]
/// sphinx
/// pallets-sphinx-themes
/// sphinxcontrib-log-cabinet
///
/// [dotenv]
/// python-dotenv
/// ```
///
/// See: <https://setuptools.pypa.io/en/latest/deprecated/python_eggs.html#dependency-metadata>
async fn read_egg_info(
    source_tree: &Path,
    subdirectory: Option<&Path>,
) -> Result<Metadata23, Error> {
    fn find_egg_info(source_tree: &Path) -> std::io::Result<Option<PathBuf>> {
        for entry in fs_err::read_dir(source_tree)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("egg-info"))
                {
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    let directory = match subdirectory {
        Some(subdirectory) => Cow::Owned(source_tree.join(subdirectory)),
        None => Cow::Borrowed(source_tree),
    };

    // Locate the `egg-info` directory.
    let egg_info = match find_egg_info(directory.as_ref()) {
        Ok(Some(path)) => path,
        Ok(None) => return Err(Error::MissingEggInfo),
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Read the `requires.txt`.
    let requires_txt = egg_info.join("requires.txt");
    let content = match fs::read(requires_txt).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingRequiresTxt);
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Parse the `requires.txt.
    let requires_txt = RequiresTxt::parse(&content).map_err(Error::RequiresTxt)?;

    // Read the `PKG-INFO` file.
    let pkg_info = egg_info.join("PKG-INFO");
    let content = match fs::read(pkg_info).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingPkgInfo);
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Parse the metadata.
    let metadata = Metadata12::parse_metadata(&content).map_err(Error::PkgInfo)?;

    // Combine the sources.
    Ok(Metadata23 {
        name: metadata.name,
        version: metadata.version,
        requires_python: metadata.requires_python,
        requires_dist: requires_txt.requires_dist,
        provides_extras: requires_txt.provides_extras,
    })
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
    let metadata = Metadata23::parse_pkg_info(&content).map_err(Error::PkgInfo)?;

    Ok(metadata)
}

/// Read the [`Metadata23`] from a source distribution's `pyproject.toml` file, if it defines static
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
    let metadata = Metadata23::parse_pyproject_toml(&content).map_err(Error::PyprojectToml)?;

    Ok(metadata)
}

/// Return the [`pypi_types::RequiresDist`] from a `pyproject.toml`, if it can be statically extracted.
async fn read_requires_dist(project_root: &Path) -> Result<pypi_types::RequiresDist, Error> {
    // Read the `pyproject.toml` file.
    let pyproject_toml = project_root.join("pyproject.toml");
    let content = match fs::read_to_string(pyproject_toml).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::MissingPyprojectToml);
        }
        Err(err) => return Err(Error::CacheRead(err)),
    };

    // Parse the metadata.
    let requires_dist =
        pypi_types::RequiresDist::parse_pyproject_toml(&content).map_err(Error::PyprojectToml)?;

    Ok(requires_dist)
}

/// Read an existing cached [`Metadata23`], if it exists.
async fn read_cached_metadata(cache_entry: &CacheEntry) -> Result<Option<Metadata23>, Error> {
    match fs::read(&cache_entry.path()).await {
        Ok(cached) => Ok(Some(rmp_serde::from_slice(&cached)?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::CacheRead(err)),
    }
}

/// Read the [`Metadata23`] from a built wheel.
fn read_wheel_metadata(filename: &WheelFilename, wheel: &Path) -> Result<Metadata23, Error> {
    let file = fs_err::File::open(wheel).map_err(Error::CacheRead)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;
    let dist_info = read_archive_metadata(filename, &mut archive)
        .map_err(|err| Error::WheelMetadata(wheel.to_path_buf(), Box::new(err)))?;
    Ok(Metadata23::parse_metadata(&dist_info)?)
}

/// Apply an advisory lock to a [`CacheShard`] to prevent concurrent builds.
async fn lock_shard(cache_shard: &CacheShard) -> Result<LockedFile, Error> {
    let root = cache_shard.as_ref();

    fs_err::create_dir_all(root).map_err(Error::CacheWrite)?;

    let lock = LockedFile::acquire(root.join(".lock"), root.display())
        .await
        .map_err(Error::CacheWrite)?;

    Ok(lock)
}
