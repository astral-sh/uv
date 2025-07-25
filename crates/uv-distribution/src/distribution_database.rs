use std::collections::BTreeSet;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use anyhow::anyhow;
use futures::{FutureExt, StreamExt, TryStreamExt};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncSeekExt, ReadBuf};
use tokio::sync::Semaphore;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{Instrument, debug, info_span, instrument, trace, warn};
use url::Url;

use uv_cache::{ArchiveId, CacheBucket, CacheEntry, WheelCache};
use uv_cache_info::{CacheInfo, Timestamp};
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_configuration::BuildOutput;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuildableSource, BuiltDist, Dist, DistributionId, HashPolicy, Hashed, Identifier, IndexUrl,
    InstalledDist, Name, Node, RegistryBuiltDist, RegistryVariantsJson, Resolution, ResolvedDist,
    ResourceId, SourceDist,
};
use uv_extract::hash::Hasher;
use uv_fs::write_atomic;
use uv_platform_tags::Tags;
use uv_pypi_types::{HashDigest, HashDigests};
use uv_redacted::DisplaySafeUrl;
use uv_types::{BuildContext, BuildStack, VariantsTrait};
use uv_variants::resolved_variants::ResolvedVariants;
use uv_variants::score_variant;
use uv_variants::variants_json::{VariantPropertyType, VariantsJsonContent};

use crate::archive::Archive;
use crate::metadata::{ArchiveMetadata, Metadata};
use crate::source::SourceDistributionBuilder;
use crate::{Error, LocalWheel, Reporter, RequiresDist};

/// A cached high-level interface to convert distributions (a requirement resolved to a location)
/// to a wheel or wheel metadata.
///
/// For wheel metadata, this happens by either fetching the metadata from the remote wheel or by
/// building the source distribution. For wheel files, either the wheel is downloaded or a source
/// distribution is downloaded, built and the new wheel gets returned.
///
/// All kinds of wheel sources (index, URL, path) and source distribution source (index, URL, path,
/// Git) are supported.
///
/// This struct also has the task of acquiring locks around source dist builds in general and git
/// operation especially, as well as respecting concurrency limits.
pub struct DistributionDatabase<'a, Context: BuildContext> {
    build_context: &'a Context,
    builder: SourceDistributionBuilder<'a, Context>,
    client: ManagedClient<'a>,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext> DistributionDatabase<'a, Context> {
    pub fn new(
        client: &'a RegistryClient,
        build_context: &'a Context,
        concurrent_downloads: usize,
    ) -> Self {
        Self {
            build_context,
            builder: SourceDistributionBuilder::new(build_context),
            client: ManagedClient::new(client, concurrent_downloads),
            reporter: None,
        }
    }

    /// Set the build stack to use for the [`DistributionDatabase`].
    #[must_use]
    pub fn with_build_stack(self, build_stack: &'a BuildStack) -> Self {
        Self {
            builder: self.builder.with_build_stack(build_stack),
            ..self
        }
    }

    /// Set the [`Reporter`] to use for the [`DistributionDatabase`].
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            builder: self.builder.with_reporter(reporter.clone()),
            reporter: Some(reporter),
            ..self
        }
    }

    /// Handle a specific `reqwest` error, and convert it to [`io::Error`].
    fn handle_response_errors(&self, err: reqwest::Error) -> io::Error {
        if err.is_timeout() {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",
                    self.client.unmanaged.timeout().as_secs()
                ),
            )
        } else {
            io::Error::other(err)
        }
    }

    /// Either fetch the wheel or fetch and build the source distribution
    ///
    /// Returns a wheel that's compliant with the given platform tags.
    ///
    /// While hashes will be generated in some cases, hash-checking is only enforced for source
    /// distributions, and should be enforced by the caller for wheels.
    #[instrument(skip_all, fields(%dist))]
    pub async fn get_or_build_wheel(
        &self,
        dist: &Dist,
        tags: &Tags,
        hashes: HashPolicy<'_>,
    ) -> Result<LocalWheel, Error> {
        match dist {
            Dist::Built(built) => self.get_wheel(built, hashes).await,
            Dist::Source(source) => self.build_wheel(source, tags, hashes).await,
        }
    }

    /// Either fetch the only wheel metadata (directly from the index or with range requests) or
    /// fetch and build the source distribution.
    ///
    /// While hashes will be generated in some cases, hash-checking is only enforced for source
    /// distributions, and should be enforced by the caller for wheels.
    #[instrument(skip_all, fields(%dist))]
    pub async fn get_installed_metadata(
        &self,
        dist: &InstalledDist,
    ) -> Result<ArchiveMetadata, Error> {
        // If the metadata was provided by the user directly, prefer it.
        if let Some(metadata) = self
            .build_context
            .dependency_metadata()
            .get(dist.name(), Some(dist.version()))
        {
            return Ok(ArchiveMetadata::from_metadata23(metadata.clone()));
        }

        let metadata = dist
            .metadata()
            .map_err(|err| Error::ReadInstalled(Box::new(dist.clone()), err))?;

        Ok(ArchiveMetadata::from_metadata23(metadata))
    }

    /// Either fetch the only wheel metadata (directly from the index or with range requests) or
    /// fetch and build the source distribution.
    ///
    /// While hashes will be generated in some cases, hash-checking is only enforced for source
    /// distributions, and should be enforced by the caller for wheels.
    #[instrument(skip_all, fields(%dist))]
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
        hashes: HashPolicy<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        match dist {
            Dist::Built(built) => self.get_wheel_metadata(built, hashes).await,
            Dist::Source(source) => {
                self.build_wheel_metadata(&BuildableSource::Dist(source), hashes)
                    .await
            }
        }
    }

    /// Fetch a wheel from the cache or download it from the index.
    ///
    /// While hashes will be generated in all cases, hash-checking is _not_ enforced and should
    /// instead be enforced by the caller.
    async fn get_wheel(
        &self,
        dist: &BuiltDist,
        hashes: HashPolicy<'_>,
    ) -> Result<LocalWheel, Error> {
        match dist {
            BuiltDist::Registry(wheels) => {
                let wheel = wheels.best_wheel();
                let url = wheel.file.url.to_url()?;

                // Create a cache entry for the wheel.
                let wheel_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).wheel_dir(wheel.name().as_ref()),
                    wheel.filename.cache_key(),
                );

                // If the URL is a file URL, load the wheel directly.
                if url.scheme() == "file" {
                    let path = url
                        .to_file_path()
                        .map_err(|()| Error::NonFileUrl(url.clone()))?;
                    return self
                        .load_wheel(&path, &wheel.filename, wheel_entry, dist, hashes)
                        .await;
                }

                // Download and unzip.
                match self
                    .stream_wheel(
                        url.clone(),
                        dist.index(),
                        &wheel.filename,
                        wheel.file.size,
                        &wheel_entry,
                        dist,
                        hashes,
                    )
                    .await
                {
                    Ok(archive) => Ok(LocalWheel {
                        dist: Dist::Built(dist.clone()),
                        archive: self
                            .build_context
                            .cache()
                            .archive(&archive.id)
                            .into_boxed_path(),
                        hashes: archive.hashes,
                        filename: wheel.filename.clone(),
                        cache: CacheInfo::default(),
                    }),
                    Err(Error::Extract(name, err)) => {
                        if err.is_http_streaming_unsupported() {
                            warn!(
                                "Streaming unsupported for {dist}; downloading wheel to disk ({err})"
                            );
                        } else if err.is_http_streaming_failed() {
                            warn!("Streaming failed for {dist}; downloading wheel to disk ({err})");
                        } else {
                            return Err(Error::Extract(name, err));
                        }

                        // If the request failed because streaming is unsupported, download the
                        // wheel directly.
                        let archive = self
                            .download_wheel(
                                url,
                                dist.index(),
                                &wheel.filename,
                                wheel.file.size,
                                &wheel_entry,
                                dist,
                                hashes,
                            )
                            .await?;

                        Ok(LocalWheel {
                            dist: Dist::Built(dist.clone()),
                            archive: self
                                .build_context
                                .cache()
                                .archive(&archive.id)
                                .into_boxed_path(),
                            hashes: archive.hashes,
                            filename: wheel.filename.clone(),
                            cache: CacheInfo::default(),
                        })
                    }
                    Err(err) => Err(err),
                }
            }

            BuiltDist::DirectUrl(wheel) => {
                // Create a cache entry for the wheel.
                let wheel_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).wheel_dir(wheel.name().as_ref()),
                    wheel.filename.cache_key(),
                );

                // Download and unzip.
                match self
                    .stream_wheel(
                        wheel.url.raw().clone(),
                        None,
                        &wheel.filename,
                        None,
                        &wheel_entry,
                        dist,
                        hashes,
                    )
                    .await
                {
                    Ok(archive) => Ok(LocalWheel {
                        dist: Dist::Built(dist.clone()),
                        archive: self
                            .build_context
                            .cache()
                            .archive(&archive.id)
                            .into_boxed_path(),
                        hashes: archive.hashes,
                        filename: wheel.filename.clone(),
                        cache: CacheInfo::default(),
                    }),
                    Err(Error::Client(err)) if err.is_http_streaming_unsupported() => {
                        warn!(
                            "Streaming unsupported for {dist}; downloading wheel to disk ({err})"
                        );

                        // If the request failed because streaming is unsupported, download the
                        // wheel directly.
                        let archive = self
                            .download_wheel(
                                wheel.url.raw().clone(),
                                None,
                                &wheel.filename,
                                None,
                                &wheel_entry,
                                dist,
                                hashes,
                            )
                            .await?;
                        Ok(LocalWheel {
                            dist: Dist::Built(dist.clone()),
                            archive: self
                                .build_context
                                .cache()
                                .archive(&archive.id)
                                .into_boxed_path(),
                            hashes: archive.hashes,
                            filename: wheel.filename.clone(),
                            cache: CacheInfo::default(),
                        })
                    }
                    Err(err) => Err(err),
                }
            }

            BuiltDist::Path(wheel) => {
                let cache_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).wheel_dir(wheel.name().as_ref()),
                    wheel.filename.cache_key(),
                );

                self.load_wheel(
                    &wheel.install_path,
                    &wheel.filename,
                    cache_entry,
                    dist,
                    hashes,
                )
                .await
            }
        }
    }

    /// Convert a source distribution into a wheel, fetching it from the cache or building it if
    /// necessary.
    ///
    /// The returned wheel is guaranteed to come from a distribution with a matching hash, and
    /// no build processes will be executed for distributions with mismatched hashes.
    async fn build_wheel(
        &self,
        dist: &SourceDist,
        tags: &Tags,
        hashes: HashPolicy<'_>,
    ) -> Result<LocalWheel, Error> {
        let built_wheel = self
            .builder
            .download_and_build(&BuildableSource::Dist(dist), tags, hashes, &self.client)
            .boxed_local()
            .await?;

        // Acquire the advisory lock.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = CacheEntry::new(
                built_wheel.target.parent().unwrap(),
                format!(
                    "{}.lock",
                    built_wheel.target.file_name().unwrap().to_str().unwrap()
                ),
            );
            lock_entry.lock().await.map_err(Error::CacheWrite)?
        };

        // If the wheel was unzipped previously, respect it. Source distributions are
        // cached under a unique revision ID, so unzipped directories are never stale.
        match self.build_context.cache().resolve_link(&built_wheel.target) {
            Ok(archive) => {
                return Ok(LocalWheel {
                    dist: Dist::Source(dist.clone()),
                    archive: archive.into_boxed_path(),
                    filename: built_wheel.filename,
                    hashes: built_wheel.hashes,
                    cache: built_wheel.cache_info,
                });
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(Error::CacheRead(err)),
        }

        // Otherwise, unzip the wheel.
        let id = self
            .unzip_wheel(&built_wheel.path, &built_wheel.target)
            .await?;

        Ok(LocalWheel {
            dist: Dist::Source(dist.clone()),
            archive: self.build_context.cache().archive(&id).into_boxed_path(),
            hashes: built_wheel.hashes,
            filename: built_wheel.filename,
            cache: built_wheel.cache_info,
        })
    }

    /// Fetch the wheel metadata from the index, or from the cache if possible.
    ///
    /// While hashes will be generated in some cases, hash-checking is _not_ enforced and should
    /// instead be enforced by the caller.
    async fn get_wheel_metadata(
        &self,
        dist: &BuiltDist,
        hashes: HashPolicy<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        // If hash generation is enabled, and the distribution isn't hosted on a registry, get the
        // entire wheel to ensure that the hashes are included in the response. If the distribution
        // is hosted on an index, the hashes will be included in the simple metadata response.
        // For hash _validation_, callers are expected to enforce the policy when retrieving the
        // wheel.
        //
        // Historically, for `uv pip compile --universal`, we also generate hashes for
        // registry-based distributions when the relevant registry doesn't provide them. This was
        // motivated by `--find-links`. We continue that behavior (under `HashGeneration::All`) for
        // backwards compatibility, but it's a little dubious, since we're only hashing _one_
        // distribution here (as opposed to hashing all distributions for the version), and it may
        // not even be a compatible distribution!
        //
        // TODO(charlie): Request the hashes via a separate method, to reduce the coupling in this API.
        if hashes.is_generate(dist) {
            let wheel = self.get_wheel(dist, hashes).await?;
            // If the metadata was provided by the user directly, prefer it.
            let metadata = if let Some(metadata) = self
                .build_context
                .dependency_metadata()
                .get(dist.name(), Some(dist.version()))
            {
                metadata.clone()
            } else {
                wheel.metadata()?
            };
            let hashes = wheel.hashes;
            return Ok(ArchiveMetadata {
                metadata: Metadata::from_metadata23(metadata),
                hashes,
            });
        }

        // If the metadata was provided by the user directly, prefer it.
        if let Some(metadata) = self
            .build_context
            .dependency_metadata()
            .get(dist.name(), Some(dist.version()))
        {
            return Ok(ArchiveMetadata::from_metadata23(metadata.clone()));
        }

        let result = self
            .client
            .managed(|client| {
                client
                    .wheel_metadata(dist, self.build_context.capabilities())
                    .boxed_local()
            })
            .await;

        match result {
            Ok(metadata) => {
                // Validate that the metadata is consistent with the distribution.
                Ok(ArchiveMetadata::from_metadata23(metadata))
            }
            Err(err) if err.is_http_streaming_unsupported() => {
                warn!(
                    "Streaming unsupported when fetching metadata for {dist}; downloading wheel directly ({err})"
                );

                // If the request failed due to an error that could be resolved by
                // downloading the wheel directly, try that.
                let wheel = self.get_wheel(dist, hashes).await?;
                let metadata = wheel.metadata()?;
                let hashes = wheel.hashes;
                Ok(ArchiveMetadata {
                    metadata: Metadata::from_metadata23(metadata),
                    hashes,
                })
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Build the wheel metadata for a source distribution, or fetch it from the cache if possible.
    ///
    /// The returned metadata is guaranteed to come from a distribution with a matching hash, and
    /// no build processes will be executed for distributions with mismatched hashes.
    pub async fn build_wheel_metadata(
        &self,
        source: &BuildableSource<'_>,
        hashes: HashPolicy<'_>,
    ) -> Result<ArchiveMetadata, Error> {
        // If the metadata was provided by the user directly, prefer it.
        if let Some(dist) = source.as_dist() {
            if let Some(metadata) = self
                .build_context
                .dependency_metadata()
                .get(dist.name(), dist.version())
            {
                // If we skipped the build, we should still resolve any Git dependencies to precise
                // commits.
                self.builder.resolve_revision(source, &self.client).await?;

                return Ok(ArchiveMetadata::from_metadata23(metadata.clone()));
            }
        }

        let metadata = self
            .builder
            .download_and_build_metadata(source, hashes, &self.client)
            .boxed_local()
            .await?;

        Ok(metadata)
    }

    /// Return the [`RequiresDist`] from a `pyproject.toml`, if it can be statically extracted.
    pub async fn requires_dist(
        &self,
        source_tree: impl AsRef<Path>,
    ) -> Result<Option<RequiresDist>, Error> {
        self.builder
            .source_tree_requires_dist(source_tree.as_ref())
            .await
    }

    #[instrument(skip_all, fields(variants_json = %variants_json.filename))]
    pub async fn fetch_and_query_variants(
        &self,
        variants_json: &RegistryVariantsJson,
    ) -> Result<ResolvedVariants, Error> {
        let variants_json = self.fetch_variants_json(variants_json).await?;
        let resolved_variants = self.query_variant_providers(variants_json).await?;
        Ok(resolved_variants)
    }

    /// Fetch the variants.json contents from a URL (cached) or from a path.
    async fn fetch_variants_json(
        &self,
        variants_json: &RegistryVariantsJson,
    ) -> Result<VariantsJsonContent, Error> {
        Ok(self
            .client
            .managed(|client| client.fetch_variants_json(variants_json))
            .await?)
    }

    async fn query_variant_providers(
        &self,
        variants_json: VariantsJsonContent,
    ) -> Result<ResolvedVariants, Error> {
        // Collect all known properties for dynamic providers.
        // TODO(konsti): We shouldn't need to do this conversion.
        let mut known_properties = BTreeSet::default();
        for variant in variants_json.variants.values() {
            for (namespace, features) in &**variant {
                for (feature, value) in features {
                    for value in value {
                        known_properties.insert(VariantPropertyType {
                            namespace: namespace.clone(),
                            feature: feature.clone(),
                            // TODO(konsti): Be consistent on value vs. property
                            value: value.clone(),
                        });
                    }
                }
            }
        }
        let known_properties: Vec<VariantPropertyType> = known_properties.into_iter().collect();

        // Compute the set of available variants.
        // Run all providers.
        let mut resolved_priorities = FxHashMap::default();
        for (name, provider) in &variants_json.providers {
            debug!("Querying provider {name}");
            let config = {
                async {
                    // TODO(konsti): That's not spec compliant
                    let backend_name = provider.plugin_api.clone().unwrap_or(name.clone());
                    let builder = self
                        .build_context
                        .setup_variants(backend_name, provider, BuildOutput::Debug)
                        .await?;
                    Ok::<_, Error>(builder.query(&known_properties).await?)
                }
                .instrument(info_span!("query_variant_provider"))
                .await?
            };
            trace!(
                "Found namespace {} with configs {:?}",
                config.namespace, config
            );
            resolved_priorities.insert(config.namespace.clone(), config);
        }

        Ok(ResolvedVariants {
            variants_json,
            target_variants: resolved_priorities,
        })
    }

    /// Stream a wheel from a URL, unzipping it into the cache as it's downloaded.
    async fn stream_wheel(
        &self,
        url: DisplaySafeUrl,
        index: Option<&IndexUrl>,
        filename: &WheelFilename,
        size: Option<u64>,
        wheel_entry: &CacheEntry,
        dist: &BuiltDist,
        hashes: HashPolicy<'_>,
    ) -> Result<Archive, Error> {
        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = wheel_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(Error::CacheWrite)?
        };

        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.cache_key()));

        let download = |response: reqwest::Response| {
            async {
                let size = size.or_else(|| content_length(&response));

                let progress = self
                    .reporter
                    .as_ref()
                    .map(|reporter| (reporter, reporter.on_download_start(dist.name(), size)));

                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Create a hasher for each hash algorithm.
                let algorithms = hashes.algorithms();
                let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

                // Download and unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;

                match progress {
                    Some((reporter, progress)) => {
                        let mut reader = ProgressReader::new(&mut hasher, progress, &**reporter);
                        uv_extract::stream::unzip(&mut reader, temp_dir.path())
                            .await
                            .map_err(|err| Error::Extract(filename.to_string(), err))?;
                    }
                    None => {
                        uv_extract::stream::unzip(&mut hasher, temp_dir.path())
                            .await
                            .map_err(|err| Error::Extract(filename.to_string(), err))?;
                    }
                }

                // If necessary, exhaust the reader to compute the hash.
                if !hashes.is_none() {
                    hasher.finish().await.map_err(Error::HashExhaustion)?;
                }

                // Persist the temporary directory to the directory store.
                let id = self
                    .build_context
                    .cache()
                    .persist(temp_dir.keep(), wheel_entry.path())
                    .await
                    .map_err(Error::CacheRead)?;

                if let Some((reporter, progress)) = progress {
                    reporter.on_download_complete(dist.name(), progress);
                }

                Ok(Archive::new(
                    id,
                    hashers.into_iter().map(HashDigest::from).collect(),
                    filename.clone(),
                ))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        // Fetch the archive from the cache, or download it if necessary.
        let req = self.request(url.clone())?;

        // Determine the cache control policy for the URL.
        let cache_control = match self.client.unmanaged.connectivity() {
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
                            .freshness(&http_entry, Some(&filename.name), None)
                            .map_err(Error::CacheRead)?,
                    )
                }
            }
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let archive = self
            .client
            .managed(|client| {
                client.cached_client().get_serde_with_retry(
                    req,
                    &http_entry,
                    cache_control,
                    download,
                )
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback { err, .. } => err,
                CachedClientError::Client { err, .. } => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, or has since been removed, force a refresh.
        let archive = Some(archive)
            .filter(|archive| archive.has_digests(hashes))
            .filter(|archive| archive.exists(self.build_context.cache()));

        let archive = if let Some(archive) = archive {
            archive
        } else {
            self.client
                .managed(async |client| {
                    client
                        .cached_client()
                        .skip_cache_with_retry(
                            self.request(url)?,
                            &http_entry,
                            cache_control,
                            download,
                        )
                        .await
                        .map_err(|err| match err {
                            CachedClientError::Callback { err, .. } => err,
                            CachedClientError::Client { err, .. } => Error::Client(err),
                        })
                })
                .await?
        };

        Ok(archive)
    }

    /// Download a wheel from a URL, then unzip it into the cache.
    async fn download_wheel(
        &self,
        url: DisplaySafeUrl,
        index: Option<&IndexUrl>,
        filename: &WheelFilename,
        size: Option<u64>,
        wheel_entry: &CacheEntry,
        dist: &BuiltDist,
        hashes: HashPolicy<'_>,
    ) -> Result<Archive, Error> {
        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = wheel_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(Error::CacheWrite)?
        };

        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.cache_key()));

        let download = |response: reqwest::Response| {
            async {
                let size = size.or_else(|| content_length(&response));

                let progress = self
                    .reporter
                    .as_ref()
                    .map(|reporter| (reporter, reporter.on_download_start(dist.name(), size)));

                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Download the wheel to a temporary file.
                let temp_file = tempfile::tempfile_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::from_std(temp_file));

                match progress {
                    Some((reporter, progress)) => {
                        // Wrap the reader in a progress reporter. This will report 100% progress
                        // after the download is complete, even if we still have to unzip and hash
                        // part of the file.
                        let mut reader =
                            ProgressReader::new(reader.compat(), progress, &**reporter);

                        tokio::io::copy(&mut reader, &mut writer)
                            .await
                            .map_err(Error::CacheWrite)?;
                    }
                    None => {
                        tokio::io::copy(&mut reader.compat(), &mut writer)
                            .await
                            .map_err(Error::CacheWrite)?;
                    }
                }

                // Unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut file = writer.into_inner();
                file.seek(io::SeekFrom::Start(0))
                    .await
                    .map_err(Error::CacheWrite)?;

                // If no hashes are required, parallelize the unzip operation.
                let hashes = if hashes.is_none() {
                    let file = file.into_std().await;
                    tokio::task::spawn_blocking({
                        let target = temp_dir.path().to_owned();
                        move || -> Result<(), uv_extract::Error> {
                            // Unzip the wheel into a temporary directory.
                            uv_extract::unzip(file, &target)?;
                            Ok(())
                        }
                    })
                    .await?
                    .map_err(|err| Error::Extract(filename.to_string(), err))?;

                    HashDigests::empty()
                } else {
                    // Create a hasher for each hash algorithm.
                    let algorithms = hashes.algorithms();
                    let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                    let mut hasher = uv_extract::hash::HashReader::new(file, &mut hashers);
                    uv_extract::stream::unzip(&mut hasher, temp_dir.path())
                        .await
                        .map_err(|err| Error::Extract(filename.to_string(), err))?;

                    // If necessary, exhaust the reader to compute the hash.
                    hasher.finish().await.map_err(Error::HashExhaustion)?;

                    hashers.into_iter().map(HashDigest::from).collect()
                };

                // Persist the temporary directory to the directory store.
                let id = self
                    .build_context
                    .cache()
                    .persist(temp_dir.keep(), wheel_entry.path())
                    .await
                    .map_err(Error::CacheRead)?;

                if let Some((reporter, progress)) = progress {
                    reporter.on_download_complete(dist.name(), progress);
                }

                Ok(Archive::new(id, hashes, filename.clone()))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        // Fetch the archive from the cache, or download it if necessary.
        let req = self.request(url.clone())?;

        // Determine the cache control policy for the URL.
        let cache_control = match self.client.unmanaged.connectivity() {
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
                            .freshness(&http_entry, Some(&filename.name), None)
                            .map_err(Error::CacheRead)?,
                    )
                }
            }
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let archive = self
            .client
            .managed(|client| {
                client.cached_client().get_serde_with_retry(
                    req,
                    &http_entry,
                    cache_control,
                    download,
                )
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback { err, .. } => err,
                CachedClientError::Client { err, .. } => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, or has since been removed, force a refresh.
        let archive = Some(archive)
            .filter(|archive| archive.has_digests(hashes))
            .filter(|archive| archive.exists(self.build_context.cache()));

        let archive = if let Some(archive) = archive {
            archive
        } else {
            self.client
                .managed(async |client| {
                    client
                        .cached_client()
                        .skip_cache_with_retry(
                            self.request(url)?,
                            &http_entry,
                            cache_control,
                            download,
                        )
                        .await
                        .map_err(|err| match err {
                            CachedClientError::Callback { err, .. } => err,
                            CachedClientError::Client { err, .. } => Error::Client(err),
                        })
                })
                .await?
        };

        Ok(archive)
    }

    /// Load a wheel from a local path.
    async fn load_wheel(
        &self,
        path: &Path,
        filename: &WheelFilename,
        wheel_entry: CacheEntry,
        dist: &BuiltDist,
        hashes: HashPolicy<'_>,
    ) -> Result<LocalWheel, Error> {
        #[cfg(windows)]
        let _lock = {
            let lock_entry = wheel_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(Error::CacheWrite)?
        };

        // Determine the last-modified time of the wheel.
        let modified = Timestamp::from_path(path).map_err(Error::CacheRead)?;

        // Attempt to read the archive pointer from the cache.
        let pointer_entry = wheel_entry.with_file(format!("{}.rev", filename.cache_key()));
        let pointer = LocalArchivePointer::read_from(&pointer_entry)?;

        // Extract the archive from the pointer.
        let archive = pointer
            .filter(|pointer| pointer.is_up_to_date(modified))
            .map(LocalArchivePointer::into_archive)
            .filter(|archive| archive.has_digests(hashes));

        // If the file is already unzipped, and the cache is up-to-date, return it.
        if let Some(archive) = archive {
            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: self
                    .build_context
                    .cache()
                    .archive(&archive.id)
                    .into_boxed_path(),
                hashes: archive.hashes,
                filename: filename.clone(),
                cache: CacheInfo::from_timestamp(modified),
            })
        } else if hashes.is_none() {
            // Otherwise, unzip the wheel.
            let archive = Archive::new(
                self.unzip_wheel(path, wheel_entry.path()).await?,
                HashDigests::empty(),
                filename.clone(),
            );

            // Write the archive pointer to the cache.
            let pointer = LocalArchivePointer {
                timestamp: modified,
                archive: archive.clone(),
            };
            pointer.write_to(&pointer_entry).await?;

            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: self
                    .build_context
                    .cache()
                    .archive(&archive.id)
                    .into_boxed_path(),
                hashes: archive.hashes,
                filename: filename.clone(),
                cache: CacheInfo::from_timestamp(modified),
            })
        } else {
            // If necessary, compute the hashes of the wheel.
            let file = fs_err::tokio::File::open(path)
                .await
                .map_err(Error::CacheRead)?;
            let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                .map_err(Error::CacheWrite)?;

            // Create a hasher for each hash algorithm.
            let algorithms = hashes.algorithms();
            let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
            let mut hasher = uv_extract::hash::HashReader::new(file, &mut hashers);

            // Unzip the wheel to a temporary directory.
            uv_extract::stream::unzip(&mut hasher, temp_dir.path())
                .await
                .map_err(|err| Error::Extract(filename.to_string(), err))?;

            // Exhaust the reader to compute the hash.
            hasher.finish().await.map_err(Error::HashExhaustion)?;

            let hashes = hashers.into_iter().map(HashDigest::from).collect();

            // Persist the temporary directory to the directory store.
            let id = self
                .build_context
                .cache()
                .persist(temp_dir.keep(), wheel_entry.path())
                .await
                .map_err(Error::CacheWrite)?;

            // Create an archive.
            let archive = Archive::new(id, hashes, filename.clone());

            // Write the archive pointer to the cache.
            let pointer = LocalArchivePointer {
                timestamp: modified,
                archive: archive.clone(),
            };
            pointer.write_to(&pointer_entry).await?;

            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: self
                    .build_context
                    .cache()
                    .archive(&archive.id)
                    .into_boxed_path(),
                hashes: archive.hashes,
                filename: filename.clone(),
                cache: CacheInfo::from_timestamp(modified),
            })
        }
    }

    /// Unzip a wheel into the cache, returning the path to the unzipped directory.
    async fn unzip_wheel(&self, path: &Path, target: &Path) -> Result<ArchiveId, Error> {
        let temp_dir = tokio::task::spawn_blocking({
            let path = path.to_owned();
            let root = self.build_context.cache().root().to_path_buf();
            move || -> Result<TempDir, Error> {
                // Unzip the wheel into a temporary directory.
                let temp_dir = tempfile::tempdir_in(root).map_err(Error::CacheWrite)?;
                let reader = fs_err::File::open(&path).map_err(Error::CacheWrite)?;
                uv_extract::unzip(reader, temp_dir.path())
                    .map_err(|err| Error::Extract(path.to_string_lossy().into_owned(), err))?;
                Ok(temp_dir)
            }
        })
        .await??;

        // Persist the temporary directory to the directory store.
        let id = self
            .build_context
            .cache()
            .persist(temp_dir.keep(), target)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(id)
    }

    /// Returns a GET [`reqwest::Request`] for the given URL.
    fn request(&self, url: DisplaySafeUrl) -> Result<reqwest::Request, reqwest::Error> {
        self.client
            .unmanaged
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

    /// Return the [`ManagedClient`] used by this resolver.
    pub fn client(&self) -> &ManagedClient<'a> {
        &self.client
    }
}

/// A wrapper around `RegistryClient` that manages a concurrency limit.
pub struct ManagedClient<'a> {
    pub unmanaged: &'a RegistryClient,
    control: Semaphore,
}

impl<'a> ManagedClient<'a> {
    /// Create a new `ManagedClient` using the given client and concurrency limit.
    fn new(client: &'a RegistryClient, concurrency: usize) -> ManagedClient<'a> {
        ManagedClient {
            unmanaged: client,
            control: Semaphore::new(concurrency),
        }
    }

    /// Perform a request using the client, respecting the concurrency limit.
    ///
    /// If the concurrency limit has been reached, this method will wait until a pending
    /// operation completes before executing the closure.
    pub async fn managed<F, T>(&self, f: impl FnOnce(&'a RegistryClient) -> F) -> T
    where
        F: Future<Output = T>,
    {
        let _permit = self.control.acquire().await.unwrap();
        f(self.unmanaged).await
    }

    /// Perform a request using a client that internally manages the concurrency limit.
    ///
    /// The callback is passed the client and a semaphore. It must acquire the semaphore before
    /// any request through the client and drop it after.
    ///
    /// This method serves as an escape hatch for functions that may want to send multiple requests
    /// in parallel.
    pub async fn manual<F, T>(&'a self, f: impl FnOnce(&'a RegistryClient, &'a Semaphore) -> F) -> T
    where
        F: Future<Output = T>,
    {
        f(self.unmanaged, &self.control).await
    }
}

/// Returns the value of the `Content-Length` header from the [`reqwest::Response`], if present.
fn content_length(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|val| val.to_str().ok())
        .and_then(|val| val.parse::<u64>().ok())
}

/// An asynchronous reader that reports progress as bytes are read.
struct ProgressReader<'a, R> {
    reader: R,
    index: usize,
    reporter: &'a dyn Reporter,
}

impl<'a, R> ProgressReader<'a, R> {
    /// Create a new [`ProgressReader`] that wraps another reader.
    fn new(reader: R, index: usize, reporter: &'a dyn Reporter) -> Self {
        Self {
            reader,
            index,
            reporter,
        }
    }
}

impl<R> AsyncRead for ProgressReader<'_, R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.as_mut().reader)
            .poll_read(cx, buf)
            .map_ok(|()| {
                self.reporter
                    .on_download_progress(self.index, buf.filled().len() as u64);
            })
    }
}

/// A pointer to an archive in the cache, fetched from an HTTP archive.
///
/// Encoded with `MsgPack`, and represented on disk by a `.http` file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HttpArchivePointer {
    archive: Archive,
}

impl HttpArchivePointer {
    /// Read an [`HttpArchivePointer`] from the cache.
    pub fn read_from(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        match fs_err::File::open(path.as_ref()) {
            Ok(file) => {
                let data = DataWithCachePolicy::from_reader(file)?.data;
                let archive = rmp_serde::from_slice::<Archive>(&data)?;
                Ok(Some(Self { archive }))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Return the [`Archive`] from the pointer.
    pub fn into_archive(self) -> Archive {
        self.archive
    }

    /// Return the [`CacheInfo`] from the pointer.
    pub fn to_cache_info(&self) -> CacheInfo {
        CacheInfo::default()
    }
}

/// A pointer to an archive in the cache, fetched from a local path.
///
/// Encoded with `MsgPack`, and represented on disk by a `.rev` file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalArchivePointer {
    timestamp: Timestamp,
    archive: Archive,
}

impl LocalArchivePointer {
    /// Read an [`LocalArchivePointer`] from the cache.
    pub fn read_from(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        match fs_err::read(path) {
            Ok(cached) => Ok(Some(rmp_serde::from_slice::<LocalArchivePointer>(&cached)?)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Write an [`LocalArchivePointer`] to the cache.
    pub async fn write_to(&self, entry: &CacheEntry) -> Result<(), Error> {
        write_atomic(entry.path(), rmp_serde::to_vec(&self)?)
            .await
            .map_err(Error::CacheWrite)
    }

    /// Returns `true` if the archive is up-to-date with the given modified timestamp.
    pub fn is_up_to_date(&self, modified: Timestamp) -> bool {
        self.timestamp == modified
    }

    /// Return the [`Archive`] from the pointer.
    pub fn into_archive(self) -> Archive {
        self.archive
    }

    /// Return the [`CacheInfo`] from the pointer.
    pub fn to_cache_info(&self) -> CacheInfo {
        CacheInfo::from_timestamp(self.timestamp)
    }
}

/// A very simple variants provider cache
///
/// TODO(konsti): Cache by provider provider plugin and `requires` compatibility
/// TODO(konsti): Cache to disk
#[derive(Debug, Default)]
pub struct VariantProviderCache {
    cache: Mutex<FxHashMap<ResourceId, ResolvedVariants>>,
}

impl VariantProviderCache {
    pub fn get_resolved_variants(&self, resource_id: &ResourceId) -> Option<ResolvedVariants> {
        self.cache
            .lock()
            .expect("there was a panic in another thread")
            .get(resource_id)
            .cloned()
    }

    pub fn insert_resolved_variants(
        &self,
        resource_id: ResourceId,
        resolved_variants: ResolvedVariants,
    ) {
        self.cache
            .lock()
            .expect("there was a panic in another thread")
            .insert(resource_id, resolved_variants);
    }
}

/// TODO(konsti): Find a better home for those functions
pub async fn resolve_variants<Context: BuildContext>(
    resolution: Resolution,
    distribution_database: DistributionDatabase<'_, Context>,
    variants_cache: Arc<VariantProviderCache>,
) -> anyhow::Result<Resolution> {
    // Fetch variants.json and then query providers, running in parallel for all dists
    // TODO(konsti): Reuse in memory index from resolve phase and merge equivalent requests.
    let dist_resolved_variants: FxHashMap<DistributionId, ResolvedVariants> =
        futures::stream::iter(
            resolution
                .graph()
                .node_weights()
                .filter_map(|node| extract_variants(node)),
        )
        .map(async |(variants_json, dist)| {
            let resolved_variants = if let Some(resolved_variants) =
                variants_cache.get_resolved_variants(&variants_json.resource_id())
            {
                resolved_variants.clone()
            } else {
                // Fetch variants_json and run providers
                let resolved_variants = distribution_database
                    .fetch_and_query_variants(variants_json)
                    .await?;

                variants_cache.insert_resolved_variants(
                    variants_json.resource_id(),
                    resolved_variants.clone(),
                );

                resolved_variants
            };

            Ok::<_, anyhow::Error>((dist.distribution_id(), resolved_variants))
        })
        // TODO(konsti): Buffer size
        .buffered(8)
        .try_collect()
        .await?;

    // Determine modification to the resolutions to select variant wheels, or error if there
    // is no matching variant wheel and no matching non-variant wheel.
    let mut new_best_wheel_index: FxHashMap<DistributionId, usize> = FxHashMap::default();
    for node in resolution.graph().node_weights() {
        let Some((_, dist)) = extract_variants(node) else {
            continue;
        };
        let resolved_variants = &dist_resolved_variants[&dist.distribution_id()];
        dist.distribution_id();

        // Select best wheel
        let mut highest_priority_variant_wheel: Option<(usize, Vec<usize>)> = None;
        for (wheel_index, wheel) in dist.wheels.iter().enumerate() {
            let Some(variant) = wheel.filename.variant() else {
                // The non-variant wheel is already supported
                continue;
            };

            let Some(variants_properties) = resolved_variants.variants_json.variants.get(variant)
            else {
                // TODO(konsti): For production, this should be a warning.
                panic!("Variant {variant} not found in variants.json");
            };

            let Some(scores) = score_variant(
                &resolved_variants.variants_json.default_priorities,
                &resolved_variants.target_variants,
                variants_properties,
            ) else {
                // The wheel is not compatible.
                continue;
            };

            if let Some((_, old_scores)) = &highest_priority_variant_wheel {
                if &scores > old_scores {
                    highest_priority_variant_wheel = Some((wheel_index, scores));
                }
            } else {
                highest_priority_variant_wheel = Some((wheel_index, scores));
            }
        }

        // Determine if we need to modify the resolution
        if let Some((wheel_index, _scores)) = highest_priority_variant_wheel {
            debug!(
                "{} for {}: {}",
                "Using variant wheel".red(),
                dist.name(),
                dist.wheels[wheel_index].filename,
            );
            new_best_wheel_index.insert(dist.distribution_id(), wheel_index);
        } else if dist.best_wheel().filename.variant().is_some() {
            return Err(anyhow!(
                "Package {} has no matching wheel for the current platform. Mismatching wheel variants: {}",
                dist.name(),
                dist.wheels
                    .iter()
                    .filter_map(|wheel| wheel.filename.variant())
                    .join(", ")
            ));
        } else {
            trace!(
                "No matching variant wheel, but matching non-variant wheel for {}",
                dist.name()
            );
        }
    }
    let resolution = resolution.map(|dist| {
        let ResolvedDist::Installable {
            dist,
            version,
            variants_json,
        } = dist
        else {
            return None;
        };
        let Dist::Built(BuiltDist::Registry(dist)) = &**dist else {
            return None;
        };
        // Check whether there is a matching variant wheel we want to use instead of the default.
        let best_wheel_index = new_best_wheel_index.get(&dist.distribution_id())?;
        Some(ResolvedDist::Installable {
            dist: Arc::new(Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                wheels: dist.wheels.clone(),
                best_wheel_index: *best_wheel_index,
                sdist: dist.sdist.clone(),
            }))),
            variants_json: variants_json.clone(),
            version: version.clone(),
        })
    });

    Ok(resolution)
}

fn extract_variants(node: &Node) -> Option<(&RegistryVariantsJson, &RegistryBuiltDist)> {
    let Node::Dist { dist, .. } = node else {
        // The root node has no variants
        return None;
    };
    let ResolvedDist::Installable {
        dist,
        variants_json,
        ..
    } = dist
    else {
        // TODO(konsti): Installed dists? Or is that not a thing here?
        return None;
    };
    let Some(variants_json) = variants_json else {
        return None;
    };
    let Dist::Built(BuiltDist::Registry(dist)) = &**dist else {
        return None;
    };
    if !dist
        .wheels
        .iter()
        .any(|wheel| wheel.filename.variant().is_some())
    {
        return None;
    }
    Some((variants_json, dist))
}
