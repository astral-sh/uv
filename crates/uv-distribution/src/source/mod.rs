//! Fetch and build source distributions from remote sources.

// This is to squash warnings about `|r| r.into_git_reporter()`. Clippy wants
// me to eta-reduce that and write it as
// `<(dyn reporter::Reporter + 'static)>::into_git_reporter`
// instead. But that's a monster. On the other hand, applying this suppression
// instruction more granularly is annoying. So we just slap it on the module
// for now. ---AG
#![allow(clippy::redundant_closure_for_method_calls)]

use std::borrow::Cow;
use std::ops::Bound;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use fs_err::tokio as fs;
use futures::{FutureExt, TryStreamExt};
use reqwest::{Response, StatusCode};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{Instrument, debug, info_span, instrument, warn};
use url::Url;
use uv_redacted::DisplaySafeUrl;
use zip::ZipArchive;

use uv_cache::{Cache, CacheBucket, CacheEntry, CacheShard, Removal, WheelCache};
use uv_cache_info::CacheInfo;
use uv_cache_key::cache_digest;
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_configuration::{BuildKind, BuildOutput, ConfigSettings, SourceStrategy};
use uv_distribution_filename::{SourceDistExtension, WheelFilename};
use uv_distribution_types::{
    BuildableSource, DirectorySourceUrl, ExtraBuildRequirement, GitSourceUrl, HashPolicy, Hashed,
    IndexUrl, PathSourceUrl, SourceDist, SourceUrl,
};
use uv_extract::hash::Hasher;
use uv_fs::{rename_with_retry, write_atomic};
use uv_git_types::{GitHubRepository, GitOid};
use uv_metadata::read_archive_metadata;
use uv_normalize::PackageName;
use uv_pep440::{Version, release_specifiers_to_ranges};
use uv_platform_tags::Tags;
use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests, PyProjectToml, ResolutionMetadata};
use uv_types::{BuildContext, BuildKey, BuildStack, SourceBuildTrait};
use uv_workspace::pyproject::ToolUvSources;

use crate::distribution_database::ManagedClient;
use crate::error::Error;
use crate::metadata::{ArchiveMetadata, GitWorkspaceMember, Metadata};
use crate::source::built_wheel_metadata::BuiltWheelMetadata;
use crate::source::revision::Revision;
use crate::{Reporter, RequiresDist};

mod built_wheel_metadata;
mod revision;

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub(crate) struct SourceDistributionBuilder<'a, T: BuildContext> {
    build_context: &'a T,
    build_stack: Option<&'a BuildStack>,
    reporter: Option<Arc<dyn Reporter>>,
}

/// The name of the file that contains the revision ID for a remote distribution, encoded via `MsgPack`.
pub(crate) const HTTP_REVISION: &str = "revision.http";

/// The name of the file that contains the revision ID for a local distribution, encoded via `MsgPack`.
pub(crate) const LOCAL_REVISION: &str = "revision.rev";

/// The name of the file that contains the cached distribution metadata, encoded via `MsgPack`.
pub(crate) const METADATA: &str = "metadata.msgpack";

/// The directory within each entry under which to store the unpacked source distribution.
pub(crate) const SOURCE: &str = "src";

impl<'a, T: BuildContext> SourceDistributionBuilder<'a, T> {
    /// Initialize a [`SourceDistributionBuilder`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            build_stack: None,
            reporter: None,
        }
    }

    /// Set the [`BuildStack`] to use for the [`SourceDistributionBuilder`].
    #[must_use]
    pub(crate) fn with_build_stack(self, build_stack: &'a BuildStack) -> Self {
        Self {
            build_stack: Some(build_stack),
            ..self
        }
    }

    /// Set the [`Reporter`] to use for the [`SourceDistributionBuilder`].
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

                let url = dist.file.url.to_url()?;

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
                    &url,
                    Some(&dist.index),
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
                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(&dist.url).root(),
                );

                self.url(
                    source,
                    &dist.url,
                    None,
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
                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(resource.url).root(),
                );

                self.url(
                    source,
                    resource.url,
                    None,
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

                let url = dist.file.url.to_url()?;

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
                    &url,
                    Some(&dist.index),
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
                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(&dist.url).root(),
                );

                self.url_metadata(
                    source,
                    &dist.url,
                    None,
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
                // For direct URLs, cache directly under the hash of the URL itself.
                let cache_shard = self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Url(resource.url).root(),
                );

                self.url_metadata(
                    source,
                    resource.url,
                    None,
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

    /// Determine the [`ConfigSettings`] for the given package name.
    fn config_settings_for(&self, name: Option<&PackageName>) -> Cow<'_, ConfigSettings> {
        if let Some(name) = name {
            if let Some(package_settings) = self.build_context.config_settings_package().get(name) {
                Cow::Owned(
                    package_settings
                        .clone()
                        .merge(self.build_context.config_settings().clone()),
                )
            } else {
                Cow::Borrowed(self.build_context.config_settings())
            }
        } else {
            Cow::Borrowed(self.build_context.config_settings())
        }
    }

    /// Determine the extra build dependencies for the given package name.
    fn extra_build_dependencies_for(&self, name: Option<&PackageName>) -> &[ExtraBuildRequirement] {
        name.and_then(|name| {
            self.build_context
                .extra_build_requires()
                .get(name)
                .map(Vec::as_slice)
        })
        .unwrap_or(&[])
    }

    /// Build a source distribution from a remote URL.
    async fn url<'data>(
        &self,
        source: &BuildableSource<'data>,
        url: &'data DisplaySafeUrl,
        index: Option<&'data IndexUrl>,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
        ext: SourceDistExtension,
        tags: &Tags,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<BuiltWheelMetadata, Error> {
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, ext, url, index, cache_shard, hashes, client)
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
        let source_dist_entry = cache_shard.entry(SOURCE);

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard)
            .ok()
            .flatten()
            .filter(|built_wheel| built_wheel.matches(source.name(), source.version()))
        {
            return Ok(built_wheel.with_hashes(revision.into_hashes()));
        }

        // Otherwise, we need to build a wheel. Before building, ensure that the source is present.
        let revision = if source_dist_entry.path().is_dir() {
            revision
        } else {
            self.heal_url_revision(
                source,
                ext,
                url,
                index,
                &source_dist_entry,
                revision,
                hashes,
                client,
            )
            .await?
        };

        // Validate that the subdirectory exists.
        if let Some(subdirectory) = subdirectory {
            if !source_dist_entry.path().join(subdirectory).is_dir() {
                return Err(Error::MissingSubdirectory(
                    url.clone(),
                    subdirectory.to_path_buf(),
                ));
            }
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        // Build the source distribution.
        let (disk_filename, wheel_filename, metadata) = self
            .build_distribution(
                source,
                source_dist_entry.path(),
                subdirectory,
                &cache_shard,
                SourceStrategy::Disabled,
            )
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
            path: cache_shard.join(&disk_filename).into_boxed_path(),
            target: cache_shard.join(wheel_filename.stem()).into_boxed_path(),
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
        url: &'data Url,
        index: Option<&'data IndexUrl>,
        cache_shard: &CacheShard,
        subdirectory: Option<&'data Path>,
        ext: SourceDistExtension,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

        // Fetch the revision for the source distribution.
        let revision = self
            .url_revision(source, ext, url, index, cache_shard, hashes, client)
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
        let source_dist_entry = cache_shard.entry(SOURCE);

        // If the metadata is static, return it.
        let dynamic =
            match StaticMetadata::read(source, source_dist_entry.path(), subdirectory).await? {
                StaticMetadata::Some(metadata) => {
                    return Ok(ArchiveMetadata {
                        metadata: Metadata::from_metadata23(metadata),
                        hashes: revision.into_hashes(),
                    });
                }
                StaticMetadata::Dynamic => true,
                StaticMetadata::None => false,
            };

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        match CachedMetadata::read(&metadata_entry).await {
            Ok(Some(metadata)) => {
                if metadata.matches(source.name(), source.version()) {
                    debug!("Using cached metadata for: {source}");
                    return Ok(ArchiveMetadata {
                        metadata: Metadata::from_metadata23(metadata.into()),
                        hashes: revision.into_hashes(),
                    });
                }
                debug!("Cached metadata does not match expected name and version for: {source}");
            }
            Ok(None) => {}
            Err(err) => {
                debug!("Failed to deserialize cached metadata for: {source} ({err})");
            }
        }

        // Otherwise, we need a wheel.
        let revision = if source_dist_entry.path().is_dir() {
            revision
        } else {
            self.heal_url_revision(
                source,
                ext,
                url,
                index,
                &source_dist_entry,
                revision,
                hashes,
                client,
            )
            .await?
        };

        // Validate that the subdirectory exists.
        if let Some(subdirectory) = subdirectory {
            if !source_dist_entry.path().join(subdirectory).is_dir() {
                return Err(Error::MissingSubdirectory(
                    DisplaySafeUrl::from(url.clone()),
                    subdirectory.to_path_buf(),
                ));
            }
        }

        // Otherwise, we either need to build the metadata.
        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(
                source,
                source_dist_entry.path(),
                subdirectory,
                SourceStrategy::Disabled,
            )
            .boxed_local()
            .await?
        {
            // If necessary, mark the metadata as dynamic.
            let metadata = if dynamic {
                ResolutionMetadata {
                    dynamic: true,
                    ..metadata
                }
            } else {
                metadata
            };

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

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        // Build the source distribution.
        let (_disk_filename, _wheel_filename, metadata) = self
            .build_distribution(
                source,
                source_dist_entry.path(),
                subdirectory,
                &cache_shard,
                SourceStrategy::Disabled,
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // If necessary, mark the metadata as dynamic.
        let metadata = if dynamic {
            ResolutionMetadata {
                dynamic: true,
                ..metadata
            }
        } else {
            metadata
        };

        // Store the metadata.
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(ArchiveMetadata {
            metadata: Metadata::from_metadata23(metadata),
            hashes: revision.into_hashes(),
        })
    }

    /// Return the [`Revision`] for a remote URL, refreshing it if necessary.
    async fn url_revision(
        &self,
        source: &BuildableSource<'_>,
        ext: SourceDistExtension,
        url: &Url,
        index: Option<&IndexUrl>,
        cache_shard: &CacheShard,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<Revision, Error> {
        let cache_entry = cache_shard.entry(HTTP_REVISION);

        // Determine the cache control policy for the request.
        let cache_control = match client.unmanaged.connectivity() {
            Connectivity::Online => {
                if let Some(header) = index.and_then(|index| {
                    self.build_context
                        .locations()
                        .artifact_cache_control_for(index)
                }) {
                    CacheControl::Override(header)
                } else {
                    CacheControl::from(
                        self.build_context
                            .cache()
                            .freshness(&cache_entry, source.name(), source.source_tree())
                            .map_err(Error::CacheRead)?,
                    )
                }
            }
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let download = |response| {
            async {
                // At this point, we're seeing a new or updated source distribution. Initialize a
                // new revision, to collect the source and built artifacts.
                let revision = Revision::new();

                // Download the source distribution.
                debug!("Downloading source distribution: {source}");
                let entry = cache_shard.shard(revision.id()).entry(SOURCE);
                let algorithms = hashes.algorithms();
                let hashes = self
                    .download_archive(response, source, ext, entry.path(), &algorithms)
                    .await?;

                Ok(revision.with_hashes(HashDigests::from(hashes)))
            }
            .boxed_local()
            .instrument(info_span!("download", source_dist = %source))
        };
        let req = Self::request(DisplaySafeUrl::from(url.clone()), client.unmanaged)?;
        let revision = client
            .managed(|client| {
                client.cached_client().get_serde_with_retry(
                    req,
                    &cache_entry,
                    cache_control,
                    download,
                )
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback { err, .. } => err,
                CachedClientError::Client { err, .. } => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, force a refresh.
        if revision.has_digests(hashes) {
            Ok(revision)
        } else {
            client
                .managed(async |client| {
                    client
                        .cached_client()
                        .skip_cache_with_retry(
                            Self::request(DisplaySafeUrl::from(url.clone()), client)?,
                            &cache_entry,
                            cache_control,
                            download,
                        )
                        .await
                        .map_err(|err| match err {
                            CachedClientError::Callback { err, .. } => err,
                            CachedClientError::Client { err, .. } => Error::Client(err),
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
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

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
        let source_entry = cache_shard.entry(SOURCE);

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard)
            .ok()
            .flatten()
            .filter(|built_wheel| built_wheel.matches(source.name(), source.version()))
        {
            return Ok(built_wheel);
        }

        // Otherwise, we need to build a wheel, which requires a source distribution.
        let revision = if source_entry.path().is_dir() {
            revision
        } else {
            self.heal_archive_revision(source, resource, &source_entry, revision, hashes)
                .await?
        };

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(
                source,
                source_entry.path(),
                None,
                &cache_shard,
                SourceStrategy::Disabled,
            )
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
            path: cache_shard.join(&disk_filename).into_boxed_path(),
            target: cache_shard.join(filename.stem()).into_boxed_path(),
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
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

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
        let source_entry = cache_shard.entry(SOURCE);

        // If the metadata is static, return it.
        let dynamic = match StaticMetadata::read(source, source_entry.path(), None).await? {
            StaticMetadata::Some(metadata) => {
                return Ok(ArchiveMetadata {
                    metadata: Metadata::from_metadata23(metadata),
                    hashes: revision.into_hashes(),
                });
            }
            StaticMetadata::Dynamic => true,
            StaticMetadata::None => false,
        };

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        match CachedMetadata::read(&metadata_entry).await {
            Ok(Some(metadata)) => {
                if metadata.matches(source.name(), source.version()) {
                    debug!("Using cached metadata for: {source}");
                    return Ok(ArchiveMetadata {
                        metadata: Metadata::from_metadata23(metadata.into()),
                        hashes: revision.into_hashes(),
                    });
                }
                debug!("Cached metadata does not match expected name and version for: {source}");
            }
            Ok(None) => {}
            Err(err) => {
                debug!("Failed to deserialize cached metadata for: {source} ({err})");
            }
        }

        // Otherwise, we need a source distribution.
        let revision = if source_entry.path().is_dir() {
            revision
        } else {
            self.heal_archive_revision(source, resource, &source_entry, revision, hashes)
                .await?
        };

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(source, source_entry.path(), None, SourceStrategy::Disabled)
            .boxed_local()
            .await?
        {
            // If necessary, mark the metadata as dynamic.
            let metadata = if dynamic {
                ResolutionMetadata {
                    dynamic: true,
                    ..metadata
                }
            } else {
                metadata
            };

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

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(
                source,
                source_entry.path(),
                None,
                &cache_shard,
                SourceStrategy::Disabled,
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // If necessary, mark the metadata as dynamic.
        let metadata = if dynamic {
            ResolutionMetadata {
                dynamic: true,
                ..metadata
            }
        } else {
            metadata
        };

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
        let entry = cache_shard.shard(revision.id()).entry(SOURCE);
        let algorithms = hashes.algorithms();
        let hashes = self
            .persist_archive(&resource.path, resource.ext, entry.path(), &algorithms)
            .await?;

        // Include the hashes and cache info in the revision.
        let revision = revision.with_hashes(HashDigests::from(hashes));

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
            if resource.editable.unwrap_or(false) {
                WheelCache::Editable(resource.url).root()
            } else {
                WheelCache::Path(resource.url).root()
            },
        );

        // Acquire the advisory lock.
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

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

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard)
            .ok()
            .flatten()
            .filter(|built_wheel| built_wheel.matches(source.name(), source.version()))
        {
            return Ok(built_wheel);
        }

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(
                source,
                &resource.install_path,
                None,
                &cache_shard,
                self.build_context.sources(),
            )
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
            path: cache_shard.join(&disk_filename).into_boxed_path(),
            target: cache_shard.join(filename.stem()).into_boxed_path(),
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

        // If the metadata is static, return it.
        let dynamic = match StaticMetadata::read(source, &resource.install_path, None).await? {
            StaticMetadata::Some(metadata) => {
                return Ok(ArchiveMetadata::from(
                    Metadata::from_workspace(
                        metadata,
                        resource.install_path.as_ref(),
                        None,
                        self.build_context.locations(),
                        self.build_context.sources(),
                        self.build_context.workspace_cache(),
                    )
                    .await?,
                ));
            }
            StaticMetadata::Dynamic => true,
            StaticMetadata::None => false,
        };

        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            if resource.editable.unwrap_or(false) {
                WheelCache::Editable(resource.url).root()
            } else {
                WheelCache::Path(resource.url).root()
            },
        );

        // Acquire the advisory lock.
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

        // Fetch the revision for the source distribution.
        let LocalRevisionPointer { revision, .. } = self
            .source_tree_revision(source, resource, &cache_shard)
            .await?;

        // Scope all operations to the revision. Within the revision, there's no need to check for
        // freshness, since entries have to be fresher than the revision itself.
        let cache_shard = cache_shard.shard(revision.id());

        // If the cache contains compatible metadata, return it.
        let metadata_entry = cache_shard.entry(METADATA);
        match CachedMetadata::read(&metadata_entry).await {
            Ok(Some(metadata)) => {
                if metadata.matches(source.name(), source.version()) {
                    debug!("Using cached metadata for: {source}");

                    // If necessary, mark the metadata as dynamic.
                    let metadata = if dynamic {
                        ResolutionMetadata {
                            dynamic: true,
                            ..metadata.into()
                        }
                    } else {
                        metadata.into()
                    };
                    return Ok(ArchiveMetadata::from(
                        Metadata::from_workspace(
                            metadata,
                            resource.install_path.as_ref(),
                            None,
                            self.build_context.locations(),
                            self.build_context.sources(),
                            self.build_context.workspace_cache(),
                        )
                        .await?,
                    ));
                }
                debug!("Cached metadata does not match expected name and version for: {source}");
            }
            Ok(None) => {}
            Err(err) => {
                debug!("Failed to deserialize cached metadata for: {source} ({err})");
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(
                source,
                &resource.install_path,
                None,
                self.build_context.sources(),
            )
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

            // If necessary, mark the metadata as dynamic.
            let metadata = if dynamic {
                ResolutionMetadata {
                    dynamic: true,
                    ..metadata
                }
            } else {
                metadata
            };

            return Ok(ArchiveMetadata::from(
                Metadata::from_workspace(
                    metadata,
                    resource.install_path.as_ref(),
                    None,
                    self.build_context.locations(),
                    self.build_context.sources(),
                    self.build_context.workspace_cache(),
                )
                .await?,
            ));
        }

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(
                source,
                &resource.install_path,
                None,
                &cache_shard,
                self.build_context.sources(),
            )
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

        // If necessary, mark the metadata as dynamic.
        let metadata = if dynamic {
            ResolutionMetadata {
                dynamic: true,
                ..metadata
            }
        } else {
            metadata
        };

        Ok(ArchiveMetadata::from(
            Metadata::from_workspace(
                metadata,
                resource.install_path.as_ref(),
                None,
                self.build_context.locations(),
                self.build_context.sources(),
                self.build_context.workspace_cache(),
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
            .freshness(&entry, source.name(), source.source_tree())
            .map_err(Error::CacheRead)?
            .is_fresh()
        {
            match LocalRevisionPointer::read_from(&entry) {
                Ok(Some(pointer)) => {
                    if *pointer.cache_info() == cache_info {
                        return Ok(pointer);
                    }

                    debug!("Cached revision does not match expected cache info for: {source}");
                }
                Ok(None) => {}
                Err(err) => {
                    debug!("Failed to deserialize cached revision for: {source} ({err})",);
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

    /// Return the [`RequiresDist`] from a `pyproject.toml`, if it can be statically extracted.
    pub(crate) async fn source_tree_requires_dist(
        &self,
        source_tree: &Path,
    ) -> Result<Option<RequiresDist>, Error> {
        // Attempt to read static metadata from the `pyproject.toml`.
        match read_requires_dist(source_tree).await {
            Ok(requires_dist) => {
                debug!(
                    "Found static `requires-dist` for: {}",
                    source_tree.display()
                );
                let requires_dist = RequiresDist::from_project_maybe_workspace(
                    requires_dist,
                    source_tree,
                    None,
                    self.build_context.locations(),
                    self.build_context.sources(),
                    self.build_context.workspace_cache(),
                )
                .await?;
                Ok(Some(requires_dist))
            }
            Err(
                err @ (Error::MissingPyprojectToml
                | Error::PyprojectToml(
                    uv_pypi_types::MetadataError::Pep508Error(_)
                    | uv_pypi_types::MetadataError::DynamicField(_)
                    | uv_pypi_types::MetadataError::FieldNotFound(_)
                    | uv_pypi_types::MetadataError::PoetrySyntax,
                )),
            ) => {
                debug!(
                    "No static `requires-dist` available for: {} ({err:?})",
                    source_tree.display()
                );
                Ok(None)
            }
            Err(err) => Err(err),
        }
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
                client
                    .unmanaged
                    .uncached_client(resource.git.repository())
                    .clone(),
                client.unmanaged.disable_ssl(resource.git.repository()),
                client.unmanaged.connectivity() == Connectivity::Offline,
                self.build_context.cache().bucket(CacheBucket::Git),
                self.reporter
                    .clone()
                    .map(|reporter| reporter.into_git_reporter()),
            )
            .await?;

        // Validate that the subdirectory exists.
        if let Some(subdirectory) = resource.subdirectory {
            if !fetch.path().join(subdirectory).is_dir() {
                return Err(Error::MissingSubdirectory(
                    resource.url.to_url(),
                    subdirectory.to_path_buf(),
                ));
            }
        }

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            WheelCache::Git(resource.url, git_sha.as_short_str()).root(),
        );
        let metadata_entry = cache_shard.entry(METADATA);

        // Acquire the advisory lock.
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // If the cache contains a compatible wheel, return it.
        if let Some(built_wheel) = BuiltWheelMetadata::find_in_cache(tags, &cache_shard)
            .ok()
            .flatten()
            .filter(|built_wheel| built_wheel.matches(source.name(), source.version()))
        {
            return Ok(built_wheel);
        }

        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (disk_filename, filename, metadata) = self
            .build_distribution(
                source,
                fetch.path(),
                resource.subdirectory,
                &cache_shard,
                self.build_context.sources(),
            )
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
            path: cache_shard.join(&disk_filename).into_boxed_path(),
            target: cache_shard.join(filename.stem()).into_boxed_path(),
            filename,
            hashes: HashDigests::empty(),
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

        // If the reference appears to be a commit, and we've already checked it out, avoid taking
        // the GitHub fast path.
        let cache_shard = resource
            .git
            .reference()
            .as_str()
            .and_then(|reference| GitOid::from_str(reference).ok())
            .map(|oid| {
                self.build_context.cache().shard(
                    CacheBucket::SourceDistributions,
                    WheelCache::Git(resource.url, oid.as_short_str()).root(),
                )
            });
        if cache_shard
            .as_ref()
            .is_some_and(|cache_shard| cache_shard.is_dir())
        {
            debug!("Skipping GitHub fast path for: {source} (shard exists)");
        } else {
            debug!("Attempting GitHub fast path for: {source}");

            // If this is GitHub URL, attempt to resolve to a precise commit using the GitHub API.
            match self
                .build_context
                .git()
                .github_fast_path(
                    resource.git,
                    client
                        .unmanaged
                        .uncached_client(resource.git.repository())
                        .raw_client(),
                )
                .await
            {
                Ok(Some(precise)) => {
                    // There's no need to check the cache, since we can't use cached metadata if there are
                    // sources, and we can't know if there are sources without fetching the
                    // `pyproject.toml`.
                    //
                    // For the same reason, there's no need to write to the cache, since we won't be able to
                    // use it on subsequent runs.
                    match self
                        .github_metadata(precise, source, resource, client)
                        .await
                    {
                        Ok(Some(metadata)) => {
                            // Validate the metadata, but ignore it if the metadata doesn't match.
                            match validate_metadata(source, &metadata) {
                                Ok(()) => {
                                    debug!(
                                        "Found static metadata via GitHub fast path for: {source}"
                                    );
                                    return Ok(ArchiveMetadata {
                                        metadata: Metadata::from_metadata23(metadata),
                                        hashes: HashDigests::empty(),
                                    });
                                }
                                Err(err) => {
                                    debug!(
                                        "Ignoring `pyproject.toml` from GitHub for {source}: {err}"
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            // Nothing to do.
                        }
                        Err(err) => {
                            debug!(
                                "Failed to fetch `pyproject.toml` via GitHub fast path for: {source} ({err})"
                            );
                        }
                    }
                }
                Ok(None) => {
                    // Nothing to do.
                }
                Err(err) => {
                    debug!("Failed to resolve commit via GitHub fast path for: {source} ({err})");
                }
            }
        }

        // Fetch the Git repository.
        let fetch = self
            .build_context
            .git()
            .fetch(
                resource.git,
                client
                    .unmanaged
                    .uncached_client(resource.git.repository())
                    .clone(),
                client.unmanaged.disable_ssl(resource.git.repository()),
                client.unmanaged.connectivity() == Connectivity::Offline,
                self.build_context.cache().bucket(CacheBucket::Git),
                self.reporter
                    .clone()
                    .map(|reporter| reporter.into_git_reporter()),
            )
            .await?;

        // Validate that the subdirectory exists.
        if let Some(subdirectory) = resource.subdirectory {
            if !fetch.path().join(subdirectory).is_dir() {
                return Err(Error::MissingSubdirectory(
                    resource.url.to_url(),
                    subdirectory.to_path_buf(),
                ));
            }
        }

        let git_sha = fetch.git().precise().expect("Exact commit after checkout");
        let cache_shard = self.build_context.cache().shard(
            CacheBucket::SourceDistributions,
            WheelCache::Git(resource.url, git_sha.as_short_str()).root(),
        );
        let metadata_entry = cache_shard.entry(METADATA);

        // Acquire the advisory lock.
        let _lock = cache_shard.lock().await.map_err(Error::CacheWrite)?;

        let path = if let Some(subdirectory) = resource.subdirectory {
            Cow::Owned(fetch.path().join(subdirectory))
        } else {
            Cow::Borrowed(fetch.path())
        };

        let git_member = GitWorkspaceMember {
            fetch_root: fetch.path(),
            git_source: resource,
        };

        // If the metadata is static, return it.
        let dynamic =
            match StaticMetadata::read(source, fetch.path(), resource.subdirectory).await? {
                StaticMetadata::Some(metadata) => {
                    return Ok(ArchiveMetadata::from(
                        Metadata::from_workspace(
                            metadata,
                            &path,
                            Some(&git_member),
                            self.build_context.locations(),
                            self.build_context.sources(),
                            self.build_context.workspace_cache(),
                        )
                        .await?,
                    ));
                }
                StaticMetadata::Dynamic => true,
                StaticMetadata::None => false,
            };

        // If the cache contains compatible metadata, return it.
        if self
            .build_context
            .cache()
            .freshness(&metadata_entry, source.name(), source.source_tree())
            .map_err(Error::CacheRead)?
            .is_fresh()
        {
            match CachedMetadata::read(&metadata_entry).await {
                Ok(Some(metadata)) => {
                    if metadata.matches(source.name(), source.version()) {
                        debug!("Using cached metadata for: {source}");

                        let git_member = GitWorkspaceMember {
                            fetch_root: fetch.path(),
                            git_source: resource,
                        };
                        return Ok(ArchiveMetadata::from(
                            Metadata::from_workspace(
                                metadata.into(),
                                &path,
                                Some(&git_member),
                                self.build_context.locations(),
                                self.build_context.sources(),
                                self.build_context.workspace_cache(),
                            )
                            .await?,
                        ));
                    }
                    debug!(
                        "Cached metadata does not match expected name and version for: {source}"
                    );
                }
                Ok(None) => {}
                Err(err) => {
                    debug!("Failed to deserialize cached metadata for: {source} ({err})");
                }
            }
        }

        // If the backend supports `prepare_metadata_for_build_wheel`, use it.
        if let Some(metadata) = self
            .build_metadata(
                source,
                fetch.path(),
                resource.subdirectory,
                self.build_context.sources(),
            )
            .boxed_local()
            .await?
        {
            // If necessary, mark the metadata as dynamic.
            let metadata = if dynamic {
                ResolutionMetadata {
                    dynamic: true,
                    ..metadata
                }
            } else {
                metadata
            };

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
                    &path,
                    Some(&git_member),
                    self.build_context.locations(),
                    self.build_context.sources(),
                    self.build_context.workspace_cache(),
                )
                .await?,
            ));
        }

        // If there are build settings or extra build dependencies, we need to scope to a cache shard.
        let config_settings = self.config_settings_for(source.name());
        let extra_build_deps = self.extra_build_dependencies_for(source.name());
        let cache_shard = if config_settings.is_empty() && extra_build_deps.is_empty() {
            cache_shard
        } else {
            cache_shard.shard(cache_digest(&(&config_settings, extra_build_deps)))
        };

        // Otherwise, we need to build a wheel.
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source));

        let (_disk_filename, _filename, metadata) = self
            .build_distribution(
                source,
                fetch.path(),
                resource.subdirectory,
                &cache_shard,
                self.build_context.sources(),
            )
            .await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source, task);
            }
        }

        // If necessary, mark the metadata as dynamic.
        let metadata = if dynamic {
            ResolutionMetadata {
                dynamic: true,
                ..metadata
            }
        } else {
            metadata
        };

        // Store the metadata.
        write_atomic(metadata_entry.path(), rmp_serde::to_vec(&metadata)?)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(ArchiveMetadata::from(
            Metadata::from_workspace(
                metadata,
                fetch.path(),
                Some(&git_member),
                self.build_context.locations(),
                self.build_context.sources(),
                self.build_context.workspace_cache(),
            )
            .await?,
        ))
    }

    /// Resolve a source to a specific revision.
    pub(crate) async fn resolve_revision(
        &self,
        source: &BuildableSource<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<(), Error> {
        let git = match source {
            BuildableSource::Dist(SourceDist::Git(source)) => &*source.git,
            BuildableSource::Url(SourceUrl::Git(source)) => source.git,
            _ => {
                return Ok(());
            }
        };

        // If the URL is already precise, return it.
        if self.build_context.git().get_precise(git).is_some() {
            debug!("Precise commit already known: {source}");
            return Ok(());
        }

        // If this is GitHub URL, attempt to resolve to a precise commit using the GitHub API.
        if self
            .build_context
            .git()
            .github_fast_path(
                git,
                client
                    .unmanaged
                    .uncached_client(git.repository())
                    .raw_client(),
            )
            .await?
            .is_some()
        {
            debug!("Resolved to precise commit via GitHub fast path: {source}");
            return Ok(());
        }

        // Otherwise, fetch the Git repository.
        self.build_context
            .git()
            .fetch(
                git,
                client.unmanaged.uncached_client(git.repository()).clone(),
                client.unmanaged.disable_ssl(git.repository()),
                client.unmanaged.connectivity() == Connectivity::Offline,
                self.build_context.cache().bucket(CacheBucket::Git),
                self.reporter
                    .clone()
                    .map(|reporter| reporter.into_git_reporter()),
            )
            .await?;

        Ok(())
    }

    /// Fetch static [`ResolutionMetadata`] from a GitHub repository, if possible.
    ///
    /// Attempts to fetch the `pyproject.toml` from the resolved commit using the GitHub API.
    async fn github_metadata(
        &self,
        commit: GitOid,
        source: &BuildableSource<'_>,
        resource: &GitSourceUrl<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<Option<ResolutionMetadata>, Error> {
        let GitSourceUrl {
            git, subdirectory, ..
        } = resource;

        // The fast path isn't available for subdirectories. If a `pyproject.toml` is in a
        // subdirectory, it could be part of a workspace; and if it's part of a workspace, it could
        // have `tool.uv.sources` entries that it inherits from the workspace root.
        if subdirectory.is_some() {
            return Ok(None);
        }

        let Some(GitHubRepository { owner, repo }) = GitHubRepository::parse(git.repository())
        else {
            return Ok(None);
        };

        // Fetch the `pyproject.toml` from the resolved commit.
        let url =
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{commit}/pyproject.toml");

        debug!("Attempting to fetch `pyproject.toml` from: {url}");

        let content = client
            .managed(async |client| {
                let response = client
                    .uncached_client(git.repository())
                    .get(&url)
                    .send()
                    .await?;

                // If the `pyproject.toml` does not exist, the GitHub API will return a 404.
                if response.status() == StatusCode::NOT_FOUND {
                    return Ok::<Option<String>, Error>(None);
                }
                response.error_for_status_ref()?;

                let content = response.text().await?;
                Ok::<Option<String>, Error>(Some(content))
            })
            .await?;

        let Some(content) = content else {
            debug!("GitHub API returned a 404 for: {url}");
            return Ok(None);
        };

        // Parse the `pyproject.toml`.
        let pyproject_toml = match PyProjectToml::from_toml(&content) {
            Ok(metadata) => metadata,
            Err(
                uv_pypi_types::MetadataError::InvalidPyprojectTomlSyntax(..)
                | uv_pypi_types::MetadataError::InvalidPyprojectTomlSchema(..),
            ) => {
                debug!("Failed to read `pyproject.toml` from GitHub API for: {url}");
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };

        // Parse the metadata.
        let metadata =
            match ResolutionMetadata::parse_pyproject_toml(pyproject_toml, source.version()) {
                Ok(metadata) => metadata,
                Err(
                    uv_pypi_types::MetadataError::Pep508Error(..)
                    | uv_pypi_types::MetadataError::DynamicField(..)
                    | uv_pypi_types::MetadataError::FieldNotFound(..)
                    | uv_pypi_types::MetadataError::PoetrySyntax,
                ) => {
                    debug!("Failed to extract static metadata from GitHub API for: {url}");
                    return Ok(None);
                }
                Err(err) => return Err(err.into()),
            };

        // Determine whether the project has `tool.uv.sources`. If the project has sources, it must
        // be lowered, which requires access to the workspace. For example, it could have workspace
        // members that need to be translated to concrete paths on disk.
        //
        // TODO(charlie): We could still use the `pyproject.toml` if the sources are all `git` or
        // `url` sources; this is only applicable to `workspace` and `path` sources. It's awkward,
        // though, because we'd need to pass a path into the lowering routine, and that path would
        // be incorrect (we'd just be relying on it not being used).
        match has_sources(&content) {
            Ok(false) => {}
            Ok(true) => {
                debug!("Skipping GitHub fast path; `pyproject.toml` has sources: {url}");
                return Ok(None);
            }
            Err(err) => {
                debug!("Failed to parse `tool.uv.sources` from GitHub API for: {url} ({err})");
                return Ok(None);
            }
        }

        Ok(Some(metadata))
    }

    /// Heal a [`Revision`] for a local archive.
    async fn heal_archive_revision(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        entry: &CacheEntry,
        revision: Revision,
        hashes: HashPolicy<'_>,
    ) -> Result<Revision, Error> {
        warn!("Re-extracting missing source distribution: {source}");

        // Take the union of the requested and existing hash algorithms.
        let algorithms = {
            let mut algorithms = hashes.algorithms();
            for digest in revision.hashes() {
                algorithms.push(digest.algorithm());
            }
            algorithms.sort();
            algorithms.dedup();
            algorithms
        };

        let hashes = self
            .persist_archive(&resource.path, resource.ext, entry.path(), &algorithms)
            .await?;
        for existing in revision.hashes() {
            if !hashes.contains(existing) {
                return Err(Error::CacheHeal(source.to_string(), existing.algorithm()));
            }
        }
        Ok(revision.with_hashes(HashDigests::from(hashes)))
    }

    /// Heal a [`Revision`] for a remote archive.
    async fn heal_url_revision(
        &self,
        source: &BuildableSource<'_>,
        ext: SourceDistExtension,
        url: &Url,
        index: Option<&IndexUrl>,
        entry: &CacheEntry,
        revision: Revision,
        hashes: HashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<Revision, Error> {
        warn!("Re-downloading missing source distribution: {source}");
        let cache_entry = entry.shard().entry(HTTP_REVISION);

        // Determine the cache control policy for the request.
        let cache_control = match client.unmanaged.connectivity() {
            Connectivity::Online => {
                if let Some(header) = index.and_then(|index| {
                    self.build_context
                        .locations()
                        .artifact_cache_control_for(index)
                }) {
                    CacheControl::Override(header)
                } else {
                    CacheControl::from(
                        self.build_context
                            .cache()
                            .freshness(&cache_entry, source.name(), source.source_tree())
                            .map_err(Error::CacheRead)?,
                    )
                }
            }
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let download = |response| {
            async {
                // Take the union of the requested and existing hash algorithms.
                let algorithms = {
                    let mut algorithms = hashes.algorithms();
                    for digest in revision.hashes() {
                        algorithms.push(digest.algorithm());
                    }
                    algorithms.sort();
                    algorithms.dedup();
                    algorithms
                };

                let hashes = self
                    .download_archive(response, source, ext, entry.path(), &algorithms)
                    .await?;
                for existing in revision.hashes() {
                    if !hashes.contains(existing) {
                        return Err(Error::CacheHeal(source.to_string(), existing.algorithm()));
                    }
                }
                Ok(revision.clone().with_hashes(HashDigests::from(hashes)))
            }
            .boxed_local()
            .instrument(info_span!("download", source_dist = %source))
        };
        client
            .managed(async |client| {
                client
                    .cached_client()
                    .skip_cache_with_retry(
                        Self::request(DisplaySafeUrl::from(url.clone()), client)?,
                        &cache_entry,
                        cache_control,
                        download,
                    )
                    .await
                    .map_err(|err| match err {
                        CachedClientError::Callback { err, .. } => err,
                        CachedClientError::Client { err, .. } => Error::Client(err),
                    })
            })
            .await
    }

    /// Download and unzip a source distribution into the cache from an HTTP response.
    async fn download_archive(
        &self,
        response: Response,
        source: &BuildableSource<'_>,
        ext: SourceDistExtension,
        target: &Path,
        algorithms: &[HashAlgorithm],
    ) -> Result<Vec<HashDigest>, Error> {
        let temp_dir = tempfile::tempdir_in(
            self.build_context
                .cache()
                .bucket(CacheBucket::SourceDistributions),
        )
        .map_err(Error::CacheWrite)?;
        let reader = response
            .bytes_stream()
            .map_err(std::io::Error::other)
            .into_async_read();

        // Create a hasher for each hash algorithm.
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

        // Download and unzip the source distribution into a temporary directory.
        let span = info_span!("download_source_dist", source_dist = %source);
        uv_extract::stream::archive(&mut hasher, ext, temp_dir.path())
            .await
            .map_err(|err| Error::Extract(source.to_string(), err))?;
        drop(span);

        // If necessary, exhaust the reader to compute the hash.
        if !algorithms.is_empty() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }

        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.keep(),
            Err(err) => {
                return Err(Error::Extract(
                    temp_dir.path().to_string_lossy().into_owned(),
                    err,
                ));
            }
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        if let Err(err) = rename_with_retry(extracted, target).await {
            // If the directory already exists, accept it.
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                warn!("Directory already exists: {}", target.display());
            } else {
                return Err(Error::CacheWrite(err));
            }
        }

        Ok(hashes)
    }

    /// Extract a local archive, and store it at the given [`CacheEntry`].
    async fn persist_archive(
        &self,
        path: &Path,
        ext: SourceDistExtension,
        target: &Path,
        algorithms: &[HashAlgorithm],
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
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        // Unzip the archive into a temporary directory.
        uv_extract::stream::archive(&mut hasher, ext, &temp_dir.path())
            .await
            .map_err(|err| Error::Extract(temp_dir.path().to_string_lossy().into_owned(), err))?;

        // If necessary, exhaust the reader to compute the hash.
        if !algorithms.is_empty() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }

        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        // Extract the top-level directory from the archive.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
            Err(err) => {
                return Err(Error::Extract(
                    temp_dir.path().to_string_lossy().into_owned(),
                    err,
                ));
            }
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        if let Err(err) = rename_with_retry(extracted, target).await {
            // If the directory already exists, accept it.
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                warn!("Directory already exists: {}", target.display());
            } else {
                return Err(Error::CacheWrite(err));
            }
        }

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
        source_strategy: SourceStrategy,
    ) -> Result<(String, WheelFilename, ResolutionMetadata), Error> {
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

        // Build into a temporary directory, to prevent partial builds.
        let temp_dir = self
            .build_context
            .cache()
            .build_dir()
            .map_err(Error::CacheWrite)?;

        // Build the wheel.
        fs::create_dir_all(&cache_shard)
            .await
            .map_err(Error::CacheWrite)?;

        // Try a direct build if that isn't disabled and the uv build backend is used.
        let disk_filename = if let Some(name) = self
            .build_context
            .direct_build(
                source_root,
                subdirectory,
                temp_dir.path(),
                if source.is_editable() {
                    BuildKind::Editable
                } else {
                    BuildKind::Wheel
                },
                Some(&source.to_string()),
            )
            .await
            .map_err(|err| Error::Build(err.into()))?
        {
            // In the uv build backend, the normalized filename and the disk filename are the same.
            name.to_string()
        } else {
            // Identify the base Python interpreter to use in the cache key.
            let base_python = if cfg!(unix) {
                self.build_context
                    .interpreter()
                    .await
                    .find_base_python()
                    .map_err(Error::BaseInterpreter)?
            } else {
                self.build_context
                    .interpreter()
                    .await
                    .to_base_python()
                    .map_err(Error::BaseInterpreter)?
            };

            let build_kind = if source.is_editable() {
                BuildKind::Editable
            } else {
                BuildKind::Wheel
            };

            let build_key = BuildKey {
                base_python: base_python.into_boxed_path(),
                source_root: source_root.to_path_buf().into_boxed_path(),
                subdirectory: subdirectory
                    .map(|subdirectory| subdirectory.to_path_buf().into_boxed_path()),
                source_strategy,
                build_kind,
            };

            if let Some(builder) = self.build_context.build_arena().remove(&build_key) {
                debug!("Creating build environment for: {source}");
                let wheel = builder.wheel(temp_dir.path()).await.map_err(Error::Build)?;

                // Store the build context.
                self.build_context.build_arena().insert(build_key, builder);

                wheel
            } else {
                debug!("Reusing existing build environment for: {source}");

                let builder = self
                    .build_context
                    .setup_build(
                        source_root,
                        subdirectory,
                        source_root,
                        Some(&source.to_string()),
                        source.as_dist(),
                        source_strategy,
                        if source.is_editable() {
                            BuildKind::Editable
                        } else {
                            BuildKind::Wheel
                        },
                        BuildOutput::Debug,
                        self.build_stack.cloned().unwrap_or_default(),
                    )
                    .await
                    .map_err(|err| Error::Build(err.into()))?;

                // Build the wheel.
                let wheel = builder.wheel(temp_dir.path()).await.map_err(Error::Build)?;

                // Store the build context.
                self.build_context.build_arena().insert(build_key, builder);

                wheel
            }
        };

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;
        let metadata = read_wheel_metadata(&filename, &temp_dir.path().join(&disk_filename))?;

        // Validate the metadata.
        validate_metadata(source, &metadata)?;
        validate_filename(&filename, &metadata)?;

        // Move the wheel to the cache.
        rename_with_retry(
            temp_dir.path().join(&disk_filename),
            cache_shard.join(&disk_filename),
        )
        .await
        .map_err(Error::CacheWrite)?;

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
        source_strategy: SourceStrategy,
    ) -> Result<Option<ResolutionMetadata>, Error> {
        debug!("Preparing metadata for: {source}");

        // Ensure that the _installed_ Python version is compatible with the `requires-python`
        // specifier.
        if let Some(requires_python) = source.requires_python() {
            let installed = self.build_context.interpreter().await.python_version();
            let target = release_specifiers_to_ranges(requires_python.clone())
                .bounding_range()
                .map(|bounding_range| bounding_range.0.cloned())
                .unwrap_or(Bound::Unbounded);
            let is_compatible = match target {
                Bound::Included(target) => *installed >= target,
                Bound::Excluded(target) => *installed > target,
                Bound::Unbounded => true,
            };
            if !is_compatible {
                return Err(Error::RequiresPython(
                    requires_python.clone(),
                    installed.clone(),
                ));
            }
        }

        // Identify the base Python interpreter to use in the cache key.
        let base_python = if cfg!(unix) {
            self.build_context
                .interpreter()
                .await
                .find_base_python()
                .map_err(Error::BaseInterpreter)?
        } else {
            self.build_context
                .interpreter()
                .await
                .to_base_python()
                .map_err(Error::BaseInterpreter)?
        };

        // Determine whether this is an editable or non-editable build.
        let build_kind = if source.is_editable() {
            BuildKind::Editable
        } else {
            BuildKind::Wheel
        };

        // Set up the builder.
        let mut builder = self
            .build_context
            .setup_build(
                source_root,
                subdirectory,
                source_root,
                Some(&source.to_string()),
                source.as_dist(),
                source_strategy,
                build_kind,
                BuildOutput::Debug,
                self.build_stack.cloned().unwrap_or_default(),
            )
            .await
            .map_err(|err| Error::Build(err.into()))?;

        // Build the metadata.
        let dist_info = builder.metadata().await.map_err(Error::Build)?;

        // Store the build context.
        self.build_context.build_arena().insert(
            BuildKey {
                base_python: base_python.into_boxed_path(),
                source_root: source_root.to_path_buf().into_boxed_path(),
                subdirectory: subdirectory
                    .map(|subdirectory| subdirectory.to_path_buf().into_boxed_path()),
                source_strategy,
                build_kind,
            },
            builder,
        );

        // Return the `.dist-info` directory, if it exists.
        let Some(dist_info) = dist_info else {
            return Ok(None);
        };

        // Read the metadata from disk.
        debug!("Prepared metadata for: {source}");
        let content = fs::read(dist_info.join("METADATA"))
            .await
            .map_err(Error::CacheRead)?;
        let metadata = ResolutionMetadata::parse_metadata(&content)?;

        // Validate the metadata.
        validate_metadata(source, &metadata)?;

        Ok(Some(metadata))
    }

    /// Returns a GET [`reqwest::Request`] for the given URL.
    fn request(
        url: DisplaySafeUrl,
        client: &RegistryClient,
    ) -> Result<reqwest::Request, reqwest::Error> {
        client
            .uncached_client(&url)
            .get(Url::from(url))
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
                if let Ok(Some(pointer)) = HttpRevisionPointer::read_from(revision) {
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
            }

            // If we find a `revision.rev` file, read the pointer, and remove any extraneous
            // directories.
            let revision = entry.path().join("revision.rev");
            if revision.is_file() {
                if let Ok(Some(pointer)) = LocalRevisionPointer::read_from(revision) {
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
            }
        }
    }

    Ok(removal)
}

/// The result of extracting statically available metadata from a source distribution.
#[derive(Debug)]
enum StaticMetadata {
    /// The metadata was found and successfully read.
    Some(ResolutionMetadata),
    /// The metadata was found, but it was ignored due to a dynamic version.
    Dynamic,
    /// The metadata was not found.
    None,
}

impl StaticMetadata {
    /// Read the [`ResolutionMetadata`] from a source distribution.
    async fn read(
        source: &BuildableSource<'_>,
        source_root: &Path,
        subdirectory: Option<&Path>,
    ) -> Result<Self, Error> {
        // Attempt to read the `pyproject.toml`.
        let pyproject_toml = match read_pyproject_toml(source_root, subdirectory).await {
            Ok(pyproject_toml) => Some(pyproject_toml),
            Err(Error::MissingPyprojectToml) => {
                debug!("No `pyproject.toml` available for: {source}");
                None
            }
            Err(err) => return Err(err),
        };

        // Determine whether the version is static or dynamic.
        let dynamic = pyproject_toml.as_ref().is_some_and(|pyproject_toml| {
            pyproject_toml.project.as_ref().is_some_and(|project| {
                project
                    .dynamic
                    .as_ref()
                    .is_some_and(|dynamic| dynamic.iter().any(|field| field == "version"))
            })
        });

        // Attempt to read static metadata from the `pyproject.toml`.
        if let Some(pyproject_toml) = pyproject_toml {
            match ResolutionMetadata::parse_pyproject_toml(pyproject_toml, source.version()) {
                Ok(metadata) => {
                    debug!("Found static `pyproject.toml` for: {source}");

                    // Validate the metadata, but ignore it if the metadata doesn't match.
                    match validate_metadata(source, &metadata) {
                        Ok(()) => {
                            return Ok(Self::Some(metadata));
                        }
                        Err(err) => {
                            debug!("Ignoring `pyproject.toml` for {source}: {err}");
                        }
                    }
                }
                Err(
                    err @ (uv_pypi_types::MetadataError::Pep508Error(_)
                    | uv_pypi_types::MetadataError::DynamicField(_)
                    | uv_pypi_types::MetadataError::FieldNotFound(_)
                    | uv_pypi_types::MetadataError::PoetrySyntax),
                ) => {
                    debug!("No static `pyproject.toml` available for: {source} ({err:?})");
                }
                Err(err) => return Err(Error::PyprojectToml(err)),
            }
        }

        // If the source distribution is a source tree, avoid reading `PKG-INFO`, since it could be
        // out-of-date.
        if source.is_source_tree() {
            return Ok(if dynamic { Self::Dynamic } else { Self::None });
        }

        // Attempt to read static metadata from the `PKG-INFO` file.
        match read_pkg_info(source_root, subdirectory).await {
            Ok(metadata) => {
                debug!("Found static `PKG-INFO` for: {source}");

                // Validate the metadata, but ignore it if the metadata doesn't match.
                match validate_metadata(source, &metadata) {
                    Ok(()) => {
                        // If necessary, mark the metadata as dynamic.
                        let metadata = if dynamic {
                            ResolutionMetadata {
                                dynamic: true,
                                ..metadata
                            }
                        } else {
                            metadata
                        };
                        return Ok(Self::Some(metadata));
                    }
                    Err(err) => {
                        debug!("Ignoring `PKG-INFO` for {source}: {err}");
                    }
                }
            }
            Err(
                err @ (Error::MissingPkgInfo
                | Error::PkgInfo(
                    uv_pypi_types::MetadataError::Pep508Error(_)
                    | uv_pypi_types::MetadataError::DynamicField(_)
                    | uv_pypi_types::MetadataError::FieldNotFound(_)
                    | uv_pypi_types::MetadataError::UnsupportedMetadataVersion(_),
                )),
            ) => {
                debug!("No static `PKG-INFO` available for: {source} ({err:?})");
            }
            Err(err) => return Err(err),
        }

        Ok(Self::None)
    }
}

/// Returns `true` if a `pyproject.toml` has `tool.uv.sources`.
fn has_sources(content: &str) -> Result<bool, toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct PyProjectToml {
        tool: Option<Tool>,
    }

    #[derive(serde::Deserialize)]
    struct Tool {
        uv: Option<ToolUv>,
    }

    #[derive(serde::Deserialize)]
    struct ToolUv {
        sources: Option<ToolUvSources>,
    }

    let PyProjectToml { tool } = toml::from_str(content)?;
    if let Some(tool) = tool {
        if let Some(uv) = tool.uv {
            if let Some(sources) = uv.sources {
                if !sources.inner().is_empty() {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Validate that the source distribution matches the built metadata.
fn validate_metadata(
    source: &BuildableSource<'_>,
    metadata: &ResolutionMetadata,
) -> Result<(), Error> {
    if let Some(name) = source.name() {
        if metadata.name != *name {
            return Err(Error::WheelMetadataNameMismatch {
                metadata: metadata.name.clone(),
                given: name.clone(),
            });
        }
    }

    if let Some(version) = source.version() {
        if *version != metadata.version && *version != metadata.version.clone().without_local() {
            return Err(Error::WheelMetadataVersionMismatch {
                metadata: metadata.version.clone(),
                given: version.clone(),
            });
        }
    }

    Ok(())
}

/// Validate that the source distribution matches the built filename.
fn validate_filename(filename: &WheelFilename, metadata: &ResolutionMetadata) -> Result<(), Error> {
    if metadata.name != filename.name {
        return Err(Error::WheelFilenameNameMismatch {
            metadata: metadata.name.clone(),
            filename: filename.name.clone(),
        });
    }

    if metadata.version != filename.version {
        return Err(Error::WheelFilenameVersionMismatch {
            metadata: metadata.version.clone(),
            filename: filename.version.clone(),
        });
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
            Ok(cached) => Ok(Some(rmp_serde::from_slice::<Self>(&cached)?)),
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

/// Read the [`ResolutionMetadata`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
/// or later _and_ none of the required fields (`Requires-Python`, `Requires-Dist`, and
/// `Provides-Extra`) are marked as dynamic.
async fn read_pkg_info(
    source_tree: &Path,
    subdirectory: Option<&Path>,
) -> Result<ResolutionMetadata, Error> {
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
    let metadata = ResolutionMetadata::parse_pkg_info(&content).map_err(Error::PkgInfo)?;

    Ok(metadata)
}

/// Read the [`ResolutionMetadata`] from a source distribution's `pyproject.toml` file, if it defines static
/// metadata consistent with PEP 621.
async fn read_pyproject_toml(
    source_tree: &Path,
    subdirectory: Option<&Path>,
) -> Result<PyProjectToml, Error> {
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

    let pyproject_toml = PyProjectToml::from_toml(&content)?;

    Ok(pyproject_toml)
}

/// Return the [`pypi_types::RequiresDist`] from a `pyproject.toml`, if it can be statically extracted.
async fn read_requires_dist(project_root: &Path) -> Result<uv_pypi_types::RequiresDist, Error> {
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
    let requires_dist = uv_pypi_types::RequiresDist::parse_pyproject_toml(&content)
        .map_err(Error::PyprojectToml)?;

    Ok(requires_dist)
}

/// Wheel metadata stored in the source distribution cache.
#[derive(Debug, Clone)]
struct CachedMetadata(ResolutionMetadata);

impl CachedMetadata {
    /// Read an existing cached [`ResolutionMetadata`], if it exists.
    async fn read(cache_entry: &CacheEntry) -> Result<Option<Self>, Error> {
        match fs::read(&cache_entry.path()).await {
            Ok(cached) => Ok(Some(Self(rmp_serde::from_slice(&cached)?))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Returns `true` if the metadata matches the given package name and version.
    fn matches(&self, name: Option<&PackageName>, version: Option<&Version>) -> bool {
        name.is_none_or(|name| self.0.name == *name)
            && version.is_none_or(|version| self.0.version == *version)
    }
}

impl From<CachedMetadata> for ResolutionMetadata {
    fn from(value: CachedMetadata) -> Self {
        value.0
    }
}

/// Read the [`ResolutionMetadata`] from a built wheel.
fn read_wheel_metadata(
    filename: &WheelFilename,
    wheel: &Path,
) -> Result<ResolutionMetadata, Error> {
    let file = fs_err::File::open(wheel).map_err(Error::CacheRead)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = ZipArchive::new(reader)?;
    let dist_info = read_archive_metadata(filename, &mut archive)
        .map_err(|err| Error::WheelMetadata(wheel.to_path_buf(), Box::new(err)))?;
    Ok(ResolutionMetadata::parse_metadata(&dist_info)?)
}
