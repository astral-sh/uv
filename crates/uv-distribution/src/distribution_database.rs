use std::fmt::Display;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{FutureExt, TryStreamExt};
use tokio::io::{AsyncRead, AsyncSeekExt, ReadBuf};
use tokio::sync::Semaphore;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{Instrument, info_span, instrument, warn};
use url::Url;

use uv_cache::{ArchiveFileId, ArchiveId, Cache, CacheBucket, CacheEntry, WheelCache};
use uv_cache_info::{CacheInfo, Timestamp};
use uv_client::{
    CacheControl, CachedClientError, Connectivity, DataWithCachePolicy, RegistryClient,
};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuildInfo, BuildableSource, BuiltDist, Dist, DistRef, HashPolicy, Hashed, IndexUrl,
    InstalledDist, Name, SourceDist,
};
use uv_extract::dirhash::{DirectoryDigest, DirhashTree, ExtractedFile, dirhash_path};
use uv_extract::hash::Hasher;
use uv_fs::{PortablePath, write_atomic};
use uv_git::{GIT_LFS, GitError};
use uv_install_wheel::{ArchiveFileManifest, ArchiveFileManifestEntry, validate_and_heal_record};
use uv_platform_tags::Tags;
use uv_preview::PreviewFeature;
use uv_pypi_types::{HashDigest, HashDigests, PyProjectToml};
use uv_python::PythonVariant;
use uv_redacted::DisplaySafeUrl;
use uv_types::{BuildContext, BuildStack};

use crate::archive::Archive;
use crate::error::PythonVersion;
use crate::hash::http_hash_algorithms;
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
    content_addressed_cache: bool,
}

impl<'a, Context: BuildContext> DistributionDatabase<'a, Context> {
    pub fn new(
        client: &'a RegistryClient,
        build_context: &'a Context,
        downloads_semaphore: Arc<Semaphore>,
    ) -> Self {
        // When ZIP validation is disabled, the extracted tree can contain files that aren't
        // represented in the central directory and therefore aren't included in its digest.
        // Avoid using an incomplete digest as a content-addressed archive ID.
        let content_addressed_cache = uv_preview::is_enabled(PreviewFeature::ContentAddressedCache)
            && !uv_extract::insecure_no_validate();
        Self {
            build_context,
            builder: SourceDistributionBuilder::new(build_context),
            client: ManagedClient::new(client, downloads_semaphore),
            reporter: None,
            content_addressed_cache,
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
            // Assumption: The connect timeout with the 10s default is not the culprit.
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",
                    self.client.unmanaged.read_timeout().as_secs()
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
            return Ok(ArchiveMetadata::from_metadata23(metadata));
        }

        let metadata = dist
            .read_metadata()
            .map_err(|err| Error::ReadInstalled(Box::new(dist.clone()), err))?;

        Ok(ArchiveMetadata::from_metadata23(metadata.clone()))
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
                let size = wheel.file.size;

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
                        size,
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
                        build: None,
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

                        // If the request failed because streaming was unsupported or failed,
                        // download the wheel directly.
                        let archive = self
                            .download_wheel(
                                url,
                                dist.index(),
                                &wheel.filename,
                                size,
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
                            build: None,
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
                        wheel.size,
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
                        build: None,
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

                        // If the request failed because streaming was unsupported or failed,
                        // download the wheel directly.
                        let archive = self
                            .download_wheel(
                                wheel.url.raw().clone(),
                                None,
                                &wheel.filename,
                                wheel.size,
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
                            build: None,
                        })
                    }
                    Err(err) => Err(err),
                }
            }

            BuiltDist::GitPath(wheel) => {
                // Fetch the Git repository.
                let fetch = self
                    .build_context
                    .git()
                    .fetch(
                        &wheel.git,
                        self.client.unmanaged.git_http_settings(wheel.git.url()),
                        self.build_context.cache().bucket(CacheBucket::Git),
                        self.reporter.clone().map(<dyn Reporter>::into_git_reporter),
                    )
                    .await?;

                if wheel.git.lfs().enabled() && !fetch.lfs_ready() {
                    if GIT_LFS.is_err() {
                        return Err(Error::MissingWheelGitLfsArtifacts(
                            wheel.url.to_url(),
                            GitError::GitLfsNotFound,
                        ));
                    }
                    return Err(Error::MissingWheelGitLfsArtifacts(
                        wheel.url.to_url(),
                        GitError::GitLfsNotConfigured,
                    ));
                }

                let git_sha = fetch.git().precise().expect("Exact commit after checkout");
                let cache_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Git(&wheel.url, git_sha.as_short_str()).root(),
                    wheel.filename.stem(),
                );

                let install_path = fetch.path().join(&wheel.install_path);

                self.load_wheel(&install_path, &wheel.filename, cache_entry, dist, hashes)
                    .await
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

        // Check that the wheel is compatible with its install target.
        //
        // When building a build dependency for a cross-install, the build dependency needs
        // to install and run on the host instead of the target. In this case the `tags` are already
        // for the host instead of the target, so this check passes.
        if !built_wheel.filename.is_compatible(tags) {
            return if tags.is_cross() {
                Err(Error::BuiltWheelIncompatibleTargetPlatform {
                    filename: built_wheel.filename,
                    python_platform: tags.python_platform().clone(),
                    python_version: PythonVersion {
                        version: tags.python_version(),
                        variant: if tags.is_freethreaded() {
                            PythonVariant::Freethreaded
                        } else {
                            PythonVariant::Default
                        },
                    },
                })
            } else {
                Err(Error::BuiltWheelIncompatibleHostPlatform {
                    filename: built_wheel.filename,
                    python_platform: tags.python_platform().clone(),
                    python_version: PythonVersion {
                        version: tags.python_version(),
                        variant: if tags.is_freethreaded() {
                            PythonVariant::Freethreaded
                        } else {
                            PythonVariant::Default
                        },
                    },
                })
            };
        }

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
            lock_entry.lock().await.map_err(Error::CacheLock)?
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
                    build: Some(built_wheel.build_info),
                });
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(Error::CacheRead(err)),
        }

        // Otherwise, unzip the wheel.
        let id = self
            .unzip_wheel(
                &built_wheel.path,
                &built_wheel.target,
                DistRef::Source(dist),
            )
            .await?;

        Ok(LocalWheel {
            dist: Dist::Source(dist.clone()),
            archive: self.build_context.cache().archive(&id).into_boxed_path(),
            hashes: built_wheel.hashes,
            filename: built_wheel.filename,
            cache: built_wheel.cache_info,
            build: Some(built_wheel.build_info),
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
                metadata
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
            return Ok(ArchiveMetadata::from_metadata23(metadata));
        }

        let result = self
            .client
            .managed(|client| {
                client
                    .wheel_metadata(
                        dist,
                        self.build_context.git(),
                        self.build_context.capabilities(),
                        self.reporter.clone().map(<dyn Reporter>::into_git_reporter),
                    )
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

                return Ok(ArchiveMetadata::from_metadata23(metadata));
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
        path: &Path,
        pyproject_toml: &PyProjectToml,
    ) -> Result<Option<RequiresDist>, Error> {
        self.builder
            .source_tree_requires_dist(
                path,
                pyproject_toml,
                self.client.unmanaged.credentials_cache(),
            )
            .await
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
        let expected_size = match dist {
            BuiltDist::Registry(dist) if dist.best_wheel().size_is_authoritative => size,
            BuiltDist::DirectUrl(_) => size,
            _ => None,
        };

        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = wheel_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(Error::CacheLock)?
        };

        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.cache_key()));

        let download = |response: reqwest::Response| {
            async {
                let progress_size = size.or_else(|| content_length(&response));

                let progress = self.reporter.as_ref().map(|reporter| {
                    (
                        reporter,
                        reporter.on_download_start(dist.name(), progress_size),
                    )
                });

                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Create a hasher for each hash algorithm.
                let algorithms = http_hash_algorithms(hashes);
                let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

                // Download and unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;

                let mut extracted = match progress {
                    Some((reporter, progress)) => {
                        let mut reader = ProgressReader::new(&mut hasher, progress, &**reporter);
                        ExtractedWheelManifest::extract_streaming(
                            &mut reader,
                            temp_dir.path(),
                            self.content_addressed_cache,
                        )
                        .await
                        .map_err(|err| Error::Extract(filename.to_string(), err))?
                    }
                    None => ExtractedWheelManifest::extract_streaming(
                        &mut hasher,
                        temp_dir.path(),
                        self.content_addressed_cache,
                    )
                    .await
                    .map_err(|err| Error::Extract(filename.to_string(), err))?,
                };
                // Exhaust the reader to compute the hashes.
                hasher.finish().await.map_err(Error::HashExhaustion)?;
                let actual_size = hasher.bytes_read();
                if let Some(expected) = expected_size
                    && actual_size != expected
                {
                    return Err(Error::MismatchedSize {
                        distribution: dist.to_string(),
                        expected,
                        actual: actual_size,
                    });
                }

                // Before we make the wheel accessible by persisting it, ensure that the RECORD is
                // valid.
                extracted.validate_and_heal_record(temp_dir.path(), dist)?;

                // Persist the temporary directory to the directory store.
                let id = self
                    .persist_extracted_wheel(
                        temp_dir,
                        wheel_entry.path(),
                        extracted.tree,
                        extracted.extracted_files,
                    )
                    .await?;

                if let Some((reporter, progress)) = progress {
                    reporter.on_download_complete(dist.name(), progress);
                }

                Ok(Archive::new(
                    id,
                    hashers.into_iter().map(HashDigest::from).collect(),
                    filename.clone(),
                    Some(actual_size),
                ))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        // Fetch the archive from the cache, or download it if necessary.
        let req = self.request(url.clone())?;

        // Determine the cache control policy for the URL.
        let cache_control = match self.client.unmanaged.connectivity() {
            Connectivity::Online
                if let Some(header) = index.and_then(|index| {
                    self.build_context
                        .locations()
                        .artifact_cache_control_for(index)
                }) =>
            {
                CacheControl::Override(header)
            }
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&http_entry, Some(&filename.name), None)
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let archive = self
            .client
            .managed(|client| {
                client.cached_client().get_serde_with_retry(
                    req,
                    &http_entry,
                    cache_control.clone(),
                    download,
                )
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback { err, .. } => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        if let (Some(expected), Some(actual)) = (expected_size, archive.size)
            && expected != actual
        {
            return Err(Error::MismatchedSize {
                distribution: dist.to_string(),
                expected,
                actual,
            });
        }

        // If the archive is missing the required hashes or size, or has since been removed, force a refresh.
        let archive = Some(archive)
            .filter(|archive| archive.has_digests(hashes))
            .filter(|archive| archive.exists(self.build_context.cache()))
            .filter(|archive| expected_size.is_none() || archive.size.is_some());

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
                            CachedClientError::Client(err) => Error::Client(err),
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
        let expected_size = match dist {
            BuiltDist::Registry(dist) if dist.best_wheel().size_is_authoritative => size,
            BuiltDist::DirectUrl(_) => size,
            _ => None,
        };

        let content_addressed_cache = self.content_addressed_cache;

        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = wheel_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(Error::CacheLock)?
        };

        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.cache_key()));

        let download = |response: reqwest::Response| {
            async {
                let progress_size = size.or_else(|| content_length(&response));

                let progress = self.reporter.as_ref().map(|reporter| {
                    (
                        reporter,
                        reporter.on_download_start(dist.name(), progress_size),
                    )
                });

                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();
                let algorithms = http_hash_algorithms(hashes);
                let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

                // Download the wheel to a temporary file.
                let temp_file = tempfile::tempfile_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut writer = tokio::io::BufWriter::new(fs_err::tokio::File::from_std(
                    // It's an unnamed file on Linux so that's the best approximation.
                    fs_err::File::from_parts(temp_file, self.build_context.cache().root()),
                ));

                match progress {
                    Some((reporter, progress)) => {
                        // Wrap the reader in a progress reporter. This will report 100% progress once
                        // the download is complete, before the wheel is unzipped.
                        let mut reader = ProgressReader::new(&mut hasher, progress, &**reporter);

                        tokio::io::copy(&mut reader, &mut writer)
                            .await
                            .map_err(Error::CacheWrite)?;
                    }
                    None => {
                        tokio::io::copy(&mut hasher, &mut writer)
                            .await
                            .map_err(Error::CacheWrite)?;
                    }
                }

                if let Some(expected) = expected_size
                    && hasher.bytes_read() != expected
                {
                    return Err(Error::MismatchedSize {
                        distribution: dist.to_string(),
                        expected,
                        actual: hasher.bytes_read(),
                    });
                }

                let actual_size = hasher.bytes_read();

                // Unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut file = writer.into_inner();
                file.seek(io::SeekFrom::Start(0))
                    .await
                    .map_err(Error::CacheWrite)?;

                let target = temp_dir.path().to_owned();
                let file = file.into_std().await;
                let mut extracted = tokio::task::spawn_blocking(move || {
                    ExtractedWheelManifest::extract_seekable(file, &target, content_addressed_cache)
                })
                .await?
                .map_err(|err| Error::Extract(filename.to_string(), err))?;
                let hashes = hashers.into_iter().map(HashDigest::from).collect();

                // Before we make the wheel accessible by persisting it, ensure that the RECORD is
                // valid.
                extracted.validate_and_heal_record(temp_dir.path(), dist)?;

                // Persist the temporary directory to the directory store.
                let id = self
                    .persist_extracted_wheel(
                        temp_dir,
                        wheel_entry.path(),
                        extracted.tree,
                        extracted.extracted_files,
                    )
                    .await?;

                if let Some((reporter, progress)) = progress {
                    reporter.on_download_complete(dist.name(), progress);
                }

                Ok(Archive::new(
                    id,
                    hashes,
                    filename.clone(),
                    Some(actual_size),
                ))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        // Fetch the archive from the cache, or download it if necessary.
        let req = self.request(url.clone())?;

        // Determine the cache control policy for the URL.
        let cache_control = match self.client.unmanaged.connectivity() {
            Connectivity::Online
                if let Some(header) = index.and_then(|index| {
                    self.build_context
                        .locations()
                        .artifact_cache_control_for(index)
                }) =>
            {
                CacheControl::Override(header)
            }
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&http_entry, Some(&filename.name), None)
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let archive = self
            .client
            .managed(|client| {
                client.cached_client().get_serde_with_retry(
                    req,
                    &http_entry,
                    cache_control.clone(),
                    download,
                )
            })
            .await
            .map_err(|err| match err {
                CachedClientError::Callback { err, .. } => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        if let (Some(expected), Some(actual)) = (expected_size, archive.size)
            && expected != actual
        {
            return Err(Error::MismatchedSize {
                distribution: dist.to_string(),
                expected,
                actual,
            });
        }

        // If the archive is missing the required hashes or size, or has since been removed, force a refresh.
        let archive = Some(archive)
            .filter(|archive| archive.has_digests(hashes))
            .filter(|archive| archive.exists(self.build_context.cache()))
            .filter(|archive| expected_size.is_none() || archive.size.is_some());

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
                            CachedClientError::Client(err) => Error::Client(err),
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
            lock_entry.lock().await.map_err(Error::CacheLock)?
        };

        // Determine the last-modified time of the wheel.
        let modified = Timestamp::from_path(path).map_err(Error::CacheRead)?;

        // Attempt to read the archive pointer from the cache.
        let pointer_entry = wheel_entry.with_file(format!("{}.rev", filename.cache_key()));
        let pointer = PathArchivePointer::read_from(&pointer_entry)?;

        // Extract the archive from the pointer.
        let archive = pointer
            .filter(|pointer| pointer.is_up_to_date(modified))
            .map(PathArchivePointer::into_archive)
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
                build: None,
            })
        } else if hashes.is_none() {
            // Otherwise, unzip the wheel.
            let archive = Archive::new(
                self.unzip_wheel(path, wheel_entry.path(), DistRef::Built(dist))
                    .await?,
                HashDigests::empty(),
                filename.clone(),
                None,
            );

            // Write the archive pointer to the cache.
            let pointer = PathArchivePointer {
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
                build: None,
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
            let mut extracted = ExtractedWheelManifest::extract_streaming(
                &mut hasher,
                temp_dir.path(),
                self.content_addressed_cache,
            )
            .await
            .map_err(|err| Error::Extract(filename.to_string(), err))?;

            // Exhaust the reader to compute the hash.
            hasher.finish().await.map_err(Error::HashExhaustion)?;

            let hashes = hashers.into_iter().map(HashDigest::from).collect();

            // Before we make the wheel accessible by persisting it, ensure that the RECORD is
            // valid.
            extracted.validate_and_heal_record(temp_dir.path(), dist)?;

            // Persist the temporary directory to the directory store.
            let id = self
                .persist_extracted_wheel(
                    temp_dir,
                    wheel_entry.path(),
                    extracted.tree,
                    extracted.extracted_files,
                )
                .await?;

            // Create an archive.
            let archive = Archive::new(id, hashes, filename.clone(), None);

            // Write the archive pointer to the cache.
            let pointer = PathArchivePointer {
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
                build: None,
            })
        }
    }

    /// Unzip a wheel into the cache, returning the path to the unzipped directory.
    async fn unzip_wheel(
        &self,
        path: &Path,
        target: &Path,
        dist: DistRef<'_>,
    ) -> Result<ArchiveId, Error> {
        let content_addressed_cache = self.content_addressed_cache;

        let (temp_dir, mut extracted) = tokio::task::spawn_blocking({
            let path = path.to_owned();
            let root = self.build_context.cache().root().to_path_buf();
            move || -> Result<_, Error> {
                // Unzip the wheel into a temporary directory.
                let temp_dir = tempfile::tempdir_in(root).map_err(Error::CacheWrite)?;
                let reader = fs_err::File::open(&path).map_err(Error::CacheWrite)?;
                let extracted = ExtractedWheelManifest::extract_seekable(
                    reader,
                    temp_dir.path(),
                    content_addressed_cache,
                )
                .map_err(|err| Error::Extract(path.to_string_lossy().into_owned(), err))?;
                Ok((temp_dir, extracted))
            }
        })
        .await??;

        // Before we make the wheel accessible by persisting it, ensure that the RECORD is valid.
        extracted.validate_and_heal_record(temp_dir.path(), dist)?;

        // Persist the temporary directory to the directory store.
        let id = self
            .persist_extracted_wheel(temp_dir, target, extracted.tree, extracted.extracted_files)
            .await?;

        Ok(id)
    }

    /// Persist an extracted wheel into the archive store.
    ///
    /// A hash tree makes identical extracted trees converge on one archive entry. Without one,
    /// persistence retains the existing behavior of assigning a unique archive ID.
    /// Binary payloads and their manifest are finalized before the archive becomes visible.
    async fn persist_extracted_wheel(
        &self,
        temp_dir: tempfile::TempDir,
        target: &Path,
        tree: Option<DirhashTree>,
        extracted_files: Option<Vec<ExtractedFile>>,
    ) -> Result<ArchiveId, Error> {
        let cache = self.build_context.cache();
        let id = match tree {
            Some(tree) => {
                let digest = DirectoryDigest::from(tree.hash());
                ArchiveId::from_digest(digest.into())
            }
            None => ArchiveId::default(),
        };

        let temp_dir = if let Some(extracted_files) = extracted_files {
            let cache = cache.clone();
            let archive_id = id.clone();
            tokio::task::spawn_blocking(move || {
                let archive_metadata = cache.archive_metadata(&archive_id);
                persist_binary_archive_files(
                    &cache,
                    temp_dir.path(),
                    &archive_metadata,
                    &extracted_files,
                )
                .map_err(Error::CacheWrite)?;
                Ok::<_, Error>(temp_dir)
            })
            .await??
        } else {
            temp_dir
        };

        cache
            .persist_with_id(temp_dir, target, id)
            .await
            .map_err(Error::CacheWrite)
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

/// The manifest of files extracted from a wheel, along with a hash tree of the unpacked archive.
struct ExtractedWheelManifest {
    files: Vec<(PathBuf, u64)>,
    extracted_files: Option<Vec<ExtractedFile>>,
    tree: Option<DirhashTree>,
}

impl ExtractedWheelManifest {
    /// Extract a wheel from a streaming reader, retaining its per-file digests.
    async fn extract_streaming<R>(
        reader: R,
        target: &Path,
        content_addressed: bool,
    ) -> Result<Self, uv_extract::Error>
    where
        R: AsyncRead + Unpin,
    {
        let (extracted_files, tree) =
            uv_extract::stream::unzip_and_hash(reader, target, content_addressed).await?;
        Ok(Self::with_extracted_files(
            extracted_files,
            content_addressed.then_some(tree),
        ))
    }

    /// Extract a wheel from a seekable file, retaining its per-file digests.
    fn extract_seekable(
        reader: fs_err::File,
        target: &Path,
        content_addressed: bool,
    ) -> Result<Self, uv_extract::Error> {
        let (extracted_files, tree) = uv_extract::unzip_and_hash(reader, target)?;
        Ok(Self::with_extracted_files(
            extracted_files,
            content_addressed.then_some(tree),
        ))
    }

    /// Derive wheel-record entries while retaining per-file digests for shared binary objects.
    fn with_extracted_files(
        extracted_files: Vec<ExtractedFile>,
        tree: Option<DirhashTree>,
    ) -> Self {
        Self {
            files: extracted_files
                .iter()
                .map(ExtractedFile::to_record)
                .collect(),
            extracted_files: Some(extracted_files),
            tree,
        }
    }

    fn without_tree(files: Vec<(PathBuf, u64)>) -> Self {
        Self {
            files,
            extracted_files: None,
            tree: None,
        }
    }

    /// Heal the wheel's `RECORD` and keep its hash tree consistent with the repaired contents.
    fn validate_and_heal_record(&mut self, root: &Path, dist: impl Display) -> Result<(), Error> {
        let Some(record_path) = validate_and_heal_record(root, self.files.iter(), dist)
            .map_err(Error::InstallWheelError)?
        else {
            return Ok(());
        };
        let Some(tree) = self.tree.as_mut() else {
            return Ok(());
        };

        let hash = dirhash_path(&root.join(&record_path)).map_err(|err| {
            Error::Extract(
                record_path.display().to_string(),
                uv_extract::Error::from(err),
            )
        })?;
        let record_path = PortablePath::from(record_path.as_path()).to_string();
        tree.update_file(&record_path, hash)
            .map_err(|err| Error::Extract(record_path, uv_extract::Error::from(err)))
    }
}

/// Move native-library payloads into shared storage before publishing their sparse archive.
///
/// Reuse an existing manifest when another writer has already prepared the same archive identity.
fn persist_binary_archive_files(
    cache: &Cache,
    archive: &Path,
    archive_metadata: &Path,
    files: &[ExtractedFile],
) -> io::Result<()> {
    if let Some(manifest) = ArchiveFileManifest::read_from_metadata(archive_metadata)? {
        for entry in manifest.files() {
            if let Err(err) = fs_err::remove_file(archive.join(entry.path()))
                && err.kind() != io::ErrorKind::NotFound
            {
                return Err(err);
            }
        }
        return Ok(());
    }

    let mut entries = Vec::new();

    for file in files.iter().filter(|file| is_binary_payload(file.path())) {
        let id = ArchiveFileId::from_content_digest(&file.digest_hex());
        persist_archive_file(&archive.join(file.path()), &cache.archive_file(&id))?;
        entries.push(ArchiveFileManifestEntry::new(
            file.path().to_path_buf(),
            id.as_ref().to_path_buf(),
        ));
    }

    ArchiveFileManifest::new(entries).write_to_metadata(archive_metadata)
}

/// Recognize native libraries, including versioned Unix shared objects like `libfoo.so.1`.
fn is_binary_payload(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
        return false;
    };
    let is_binary_extension = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("so")
            || extension.eq_ignore_ascii_case("pyd")
            || extension.eq_ignore_ascii_case("dll")
            || extension.eq_ignore_ascii_case("dylib")
    });

    is_binary_extension || file_name.to_ascii_lowercase().contains(".so.")
}

/// Publish a shared object, tolerating competing writers and filesystems without hardlinks.
fn persist_archive_file(src: &Path, dst: &Path) -> io::Result<()> {
    persist_archive_file_with(src, dst, |src, dst| fs_err::hard_link(src, dst))
}

/// Persist an object with an injectable hardlink operation for exercising copy fallbacks.
fn persist_archive_file_with(
    src: &Path,
    dst: &Path,
    hard_link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let Some(parent) = dst.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "archive file path must have a parent directory",
        ));
    };
    fs_err::create_dir_all(parent)?;

    if !dst.try_exists()? {
        match hard_link(src, dst) {
            Ok(()) => {}
            Err(_) if dst.try_exists()? => {}
            Err(_) => uv_fs::copy_atomic_sync(src, dst)?,
        }
    }

    if let Err(err) = fs_err::remove_file(src)
        && err.kind() != io::ErrorKind::NotFound
    {
        return Err(err);
    }

    Ok(())
}

/// A wrapper around `RegistryClient` that manages a concurrency limit.
pub struct ManagedClient<'a> {
    pub unmanaged: &'a RegistryClient,
    control: Arc<Semaphore>,
}

impl<'a> ManagedClient<'a> {
    /// Create a new `ManagedClient` using the given client and concurrency semaphore.
    fn new(client: &'a RegistryClient, control: Arc<Semaphore>) -> Self {
        ManagedClient {
            unmanaged: client,
            control,
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

    /// Return the [`BuildInfo`] from the pointer.
    pub fn to_build_info(&self) -> Option<BuildInfo> {
        None
    }
}

/// A pointer to an archive in the cache, fetched from a local path.
///
/// Encoded with `MsgPack`, and represented on disk by a `.rev` file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathArchivePointer {
    timestamp: Timestamp,
    archive: Archive,
}

impl PathArchivePointer {
    /// Read an [`PathArchivePointer`] from the cache.
    pub fn read_from(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        match fs_err::read(path) {
            Ok(cached) => Ok(Some(rmp_serde::from_slice::<Self>(&cached)?)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(Error::CacheRead(err)),
        }
    }

    /// Write an [`PathArchivePointer`] to the cache.
    async fn write_to(&self, entry: &CacheEntry) -> Result<(), Error> {
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

    /// Return the [`BuildInfo`] from the pointer.
    pub fn to_build_info(&self) -> Option<BuildInfo> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_binary_archive_files_sparsifies_unpublished_archive() -> io::Result<()> {
        let cache = Cache::temp()?;
        let archive_id = ArchiveId::default();
        let temp_dir = tempfile::tempdir()?;
        let archive = temp_dir.path().join("archive");
        let archive_metadata = cache.archive_metadata(&archive_id);
        let archive_file = archive.join("package/native.so");
        fs_err::create_dir_all(archive_file.parent().expect("archive file has a parent"))?;
        fs_err::write(&archive_file, "binary contents")?;
        let manifest = ArchiveFileManifest::new(vec![ArchiveFileManifestEntry::new(
            PathBuf::from("package/native.so"),
            PathBuf::from("ab/abcdef"),
        )]);
        manifest.write_to_metadata(&archive_metadata)?;

        assert!(!cache.archive(&archive_id).exists());
        persist_binary_archive_files(&cache, &archive, &archive_metadata, &[])?;

        assert_eq!(
            ArchiveFileManifest::read_from_metadata(&archive_metadata)?,
            Some(manifest)
        );
        assert!(!archive_file.exists());
        assert!(!cache.archive(&archive_id).exists());
        Ok(())
    }

    #[test]
    fn persist_archive_file_accepts_missing_source_for_existing_object() -> io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let archive_dir = temp_dir.path().join("archive/package");
        let archive_files_dir = temp_dir.path().join("archive-files");
        fs_err::create_dir_all(&archive_dir)?;
        fs_err::create_dir_all(&archive_files_dir)?;
        let src = archive_dir.join("native.so");
        let dst = archive_files_dir.join("native.so");
        fs_err::write(&dst, "binary contents")?;

        persist_archive_file(&src, &dst)?;

        assert!(!src.exists());
        assert_eq!(fs_err::read(&dst)?, b"binary contents");
        Ok(())
    }

    #[test]
    fn persist_archive_file_copies_when_hardlinks_are_unsupported() -> io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let archive_dir = temp_dir.path().join("archive/package");
        let archive_files_dir = temp_dir.path().join("archive-files");
        fs_err::create_dir_all(&archive_dir)?;
        let src = archive_dir.join("native.so");
        let dst = archive_files_dir.join("native.so");
        fs_err::write(&src, "binary contents")?;

        persist_archive_file_with(&src, &dst, |_, _| {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "hardlinks are unsupported",
            ))
        })?;

        assert!(!src.exists());
        assert_eq!(fs_err::read(&dst)?, b"binary contents");
        Ok(())
    }

    #[test]
    fn persist_archive_file_removes_source_for_existing_object() -> io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let archive_dir = temp_dir.path().join("archive/package");
        let archive_files_dir = temp_dir.path().join("archive-files");
        fs_err::create_dir_all(&archive_dir)?;
        fs_err::create_dir_all(&archive_files_dir)?;
        let src = archive_dir.join("native.so");
        let dst = archive_files_dir.join("native.so");
        fs_err::write(&src, "binary contents")?;
        fs_err::write(&dst, "binary contents")?;

        persist_archive_file(&src, &dst)?;

        assert!(!src.exists());
        assert_eq!(fs_err::read(&dst)?, b"binary contents");
        Ok(())
    }
}
