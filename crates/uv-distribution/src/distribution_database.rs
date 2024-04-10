use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::{FutureExt, TryStreamExt};
use tempfile::TempDir;
use tokio::io::AsyncSeekExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{info_span, instrument, warn, Instrument};
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{
    BuildableSource, BuiltDist, Dist, FileLocation, Hashed, IndexLocations, LocalEditable, Name,
    SourceDist,
};
use platform_tags::Tags;
use pypi_types::{HashDigest, Metadata23};
use uv_cache::{ArchiveTimestamp, CacheBucket, CacheEntry, CachedByTimestamp, WheelCache};
use uv_client::{CacheControl, CachedClientError, Connectivity, RegistryClient};
use uv_configuration::{NoBinary, NoBuild};
use uv_extract::hash::Hasher;
use uv_fs::write_atomic;
use uv_types::BuildContext;

use crate::archive::Archive;
use crate::locks::Locks;
use crate::{ArchiveMetadata, Error, LocalWheel, Reporter, SourceDistributionBuilder};

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
    client: &'a RegistryClient,
    build_context: &'a Context,
    builder: SourceDistributionBuilder<'a, Context>,
    locks: Arc<Locks>,
}

impl<'a, Context: BuildContext + Send + Sync> DistributionDatabase<'a, Context> {
    pub fn new(client: &'a RegistryClient, build_context: &'a Context) -> Self {
        Self {
            client,
            build_context,
            builder: SourceDistributionBuilder::new(client, build_context),
            locks: Arc::new(Locks::default()),
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter = Arc::new(reporter);
        Self {
            builder: self.builder.with_reporter(reporter),
            ..self
        }
    }

    /// Handle a specific `reqwest` error, and convert it to [`io::Error`].
    fn handle_response_errors(&self, err: reqwest::Error) -> io::Error {
        if err.is_timeout() {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",  self.client.timeout()
                ),
            )
        } else {
            io::Error::new(io::ErrorKind::Other, err)
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
        hashes: &[HashDigest],
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
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
        hashes: &[HashDigest],
    ) -> Result<ArchiveMetadata, Error> {
        match dist {
            Dist::Built(built) => self.get_wheel_metadata(built, hashes).await,
            Dist::Source(source) => {
                self.build_wheel_metadata(&BuildableSource::Dist(source), hashes)
                    .await
            }
        }
    }

    /// Build a directory into an editable wheel.
    pub async fn build_wheel_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(LocalWheel, Metadata23), Error> {
        // Build the wheel.
        let (dist, disk_filename, filename, metadata) = self
            .builder
            .build_editable(editable, editable_wheel_dir)
            .await?;

        // Unzip into the editable wheel directory.
        let path = editable_wheel_dir.join(&disk_filename);
        let target = editable_wheel_dir.join(cache_key::digest(&editable.path));
        let archive = self.unzip_wheel(&path, &target).await?;
        let wheel = LocalWheel {
            dist,
            filename,
            archive,
            hashes: vec![],
        };

        Ok((wheel, metadata))
    }

    /// Fetch a wheel from the cache or download it from the index.
    ///
    /// While hashes will be generated in some cases, hash-checking is _not_ enforced and should
    /// instead be enforced by the caller.
    async fn get_wheel(
        &self,
        dist: &BuiltDist,
        hashes: &[HashDigest],
    ) -> Result<LocalWheel, Error> {
        let no_binary = match self.build_context.no_binary() {
            NoBinary::None => false,
            NoBinary::All => true,
            NoBinary::Packages(packages) => packages.contains(dist.name()),
        };
        if no_binary {
            return Err(Error::NoBinary);
        }

        match dist {
            BuiltDist::Registry(wheel) => {
                let url = match &wheel.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let cache_entry = self.build_context.cache().entry(
                            CacheBucket::Wheels,
                            WheelCache::Index(&wheel.index).wheel_dir(wheel.name().as_ref()),
                            wheel.filename.stem(),
                        );

                        return self
                            .load_wheel(path, &wheel.filename, cache_entry, dist, hashes)
                            .await;
                    }
                };

                // Create a cache entry for the wheel.
                let wheel_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                // Download and unzip.
                match self
                    .stream_wheel(url.clone(), &wheel.filename, &wheel_entry, dist, hashes)
                    .await
                {
                    Ok(archive) => Ok(LocalWheel {
                        dist: Dist::Built(dist.clone()),
                        archive: archive.path,
                        hashes: archive.hashes,
                        filename: wheel.filename.clone(),
                    }),
                    Err(Error::Extract(err)) if err.is_http_streaming_unsupported() => {
                        warn!(
                            "Streaming unsupported for {dist}; downloading wheel to disk ({err})"
                        );

                        // If the request failed because streaming is unsupported, download the
                        // wheel directly.
                        let archive = self
                            .download_wheel(url, &wheel.filename, &wheel_entry, dist, hashes)
                            .await?;
                        Ok(LocalWheel {
                            dist: Dist::Built(dist.clone()),
                            archive: archive.path,
                            hashes: archive.hashes,
                            filename: wheel.filename.clone(),
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
                    wheel.filename.stem(),
                );

                // Download and unzip.
                match self
                    .stream_wheel(
                        wheel.url.raw().clone(),
                        &wheel.filename,
                        &wheel_entry,
                        dist,
                        hashes,
                    )
                    .await
                {
                    Ok(archive) => Ok(LocalWheel {
                        dist: Dist::Built(dist.clone()),
                        archive: archive.path,
                        hashes: archive.hashes,
                        filename: wheel.filename.clone(),
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
                                &wheel.filename,
                                &wheel_entry,
                                dist,
                                hashes,
                            )
                            .await?;
                        Ok(LocalWheel {
                            dist: Dist::Built(dist.clone()),
                            archive: archive.path,
                            hashes: archive.hashes,
                            filename: wheel.filename.clone(),
                        })
                    }
                    Err(err) => Err(err),
                }
            }

            BuiltDist::Path(wheel) => {
                let cache_entry = self.build_context.cache().entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                self.load_wheel(&wheel.path, &wheel.filename, cache_entry, dist, hashes)
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
        hashes: &[HashDigest],
    ) -> Result<LocalWheel, Error> {
        let lock = self.locks.acquire(&Dist::Source(dist.clone())).await;
        let _guard = lock.lock().await;

        let built_wheel = self
            .builder
            .download_and_build(&BuildableSource::Dist(dist), tags, hashes)
            .boxed()
            .await?;

        // If the wheel was unzipped previously, respect it. Source distributions are
        // cached under a unique revision ID, so unzipped directories are never stale.
        match built_wheel.target.canonicalize() {
            Ok(archive) => {
                return Ok(LocalWheel {
                    dist: Dist::Source(dist.clone()),
                    archive,
                    filename: built_wheel.filename,
                    hashes: built_wheel.hashes,
                });
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(Error::CacheRead(err)),
        }

        // Otherwise, unzip the wheel.
        Ok(LocalWheel {
            dist: Dist::Source(dist.clone()),
            archive: self
                .unzip_wheel(&built_wheel.path, &built_wheel.target)
                .await?,
            hashes: built_wheel.hashes,
            filename: built_wheel.filename,
        })
    }

    /// Fetch the wheel metadata from the index, or from the cache if possible.
    ///
    /// While hashes will be generated in some cases, hash-checking is _not_ enforced and should
    /// instead be enforced by the caller.
    pub async fn get_wheel_metadata(
        &self,
        dist: &BuiltDist,
        hashes: &[HashDigest],
    ) -> Result<ArchiveMetadata, Error> {
        match self.client.wheel_metadata(dist).boxed().await {
            Ok(metadata) => Ok(ArchiveMetadata::from(metadata)),
            Err(err) if err.is_http_streaming_unsupported() => {
                warn!("Streaming unsupported when fetching metadata for {dist}; downloading wheel directly ({err})");

                // If the request failed due to an error that could be resolved by
                // downloading the wheel directly, try that.
                let wheel = self.get_wheel(dist, hashes).await?;
                let metadata = wheel.metadata()?;
                let hashes = wheel.hashes;
                Ok(ArchiveMetadata { metadata, hashes })
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
        hashes: &[HashDigest],
    ) -> Result<ArchiveMetadata, Error> {
        let no_build = match self.build_context.no_build() {
            NoBuild::All => true,
            NoBuild::None => false,
            NoBuild::Packages(packages) => {
                source.name().is_some_and(|name| packages.contains(name))
            }
        };

        // Optimization: Skip source dist download when we must not build them anyway.
        if no_build {
            return Err(Error::NoBuild);
        }

        let lock = self.locks.acquire(source).await;
        let _guard = lock.lock().await;

        let metadata = self
            .builder
            .download_and_build_metadata(source, hashes)
            .boxed()
            .await?;
        Ok(metadata)
    }

    /// Stream a wheel from a URL, unzipping it into the cache as it's downloaded.
    async fn stream_wheel(
        &self,
        url: Url,
        filename: &WheelFilename,
        wheel_entry: &CacheEntry,
        dist: &BuiltDist,
        hashes: &[HashDigest],
    ) -> Result<Archive, Error> {
        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.stem()));

        let download = |response: reqwest::Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Create a hasher for each hash algorithm.
                let algorithms = {
                    let mut hash = hashes.iter().map(HashDigest::algorithm).collect::<Vec<_>>();
                    hash.sort();
                    hash.dedup();
                    hash
                };
                let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

                // Download and unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                uv_extract::stream::unzip(&mut hasher, temp_dir.path()).await?;

                // If necessary, exhaust the reader to compute the hash.
                if !hashes.is_empty() {
                    hasher.finish().await.map_err(Error::HashExhaustion)?;
                }

                // Persist the temporary directory to the directory store.
                let path = self
                    .build_context
                    .cache()
                    .persist(temp_dir.into_path(), wheel_entry.path())
                    .await
                    .map_err(Error::CacheRead)?;
                Ok(Archive::new(
                    path,
                    hashers.into_iter().map(HashDigest::from).collect(),
                ))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        // Fetch the archive from the cache, or download it if necessary.
        let req = self.request(url.clone())?;
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&http_entry, Some(&filename.name))
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };
        let archive = self
            .client
            .cached_client()
            .get_serde(req, &http_entry, cache_control, download)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, force a refresh.
        let archive = if archive.has_digests(hashes) {
            archive
        } else {
            self.client
                .cached_client()
                .skip_cache(self.request(url)?, &http_entry, download)
                .await
                .map_err(|err| match err {
                    CachedClientError::Callback(err) => err,
                    CachedClientError::Client(err) => Error::Client(err),
                })?
        };

        Ok(archive)
    }

    /// Download a wheel from a URL, then unzip it into the cache.
    async fn download_wheel(
        &self,
        url: Url,
        filename: &WheelFilename,
        wheel_entry: &CacheEntry,
        dist: &BuiltDist,
        hashes: &[HashDigest],
    ) -> Result<Archive, Error> {
        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.stem()));

        let download = |response: reqwest::Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Download the wheel to a temporary file.
                let temp_file = tempfile::tempfile_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::from_std(temp_file));
                tokio::io::copy(&mut reader.compat(), &mut writer)
                    .await
                    .map_err(Error::CacheWrite)?;

                // Unzip the wheel to a temporary directory.
                let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                    .map_err(Error::CacheWrite)?;
                let mut file = writer.into_inner();
                file.seek(io::SeekFrom::Start(0))
                    .await
                    .map_err(Error::CacheWrite)?;

                // If no hashes are required, parallelize the unzip operation.
                let hashes = if hashes.is_empty() {
                    let file = file.into_std().await;
                    tokio::task::spawn_blocking({
                        let target = temp_dir.path().to_owned();
                        move || -> Result<(), uv_extract::Error> {
                            // Unzip the wheel into a temporary directory.
                            uv_extract::unzip(file, &target)?;
                            Ok(())
                        }
                    })
                    .await??;

                    vec![]
                } else {
                    // Create a hasher for each hash algorithm.
                    let algorithms = {
                        let mut hash = hashes.iter().map(HashDigest::algorithm).collect::<Vec<_>>();
                        hash.sort();
                        hash.dedup();
                        hash
                    };
                    let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
                    let mut hasher = uv_extract::hash::HashReader::new(file, &mut hashers);
                    uv_extract::stream::unzip(&mut hasher, temp_dir.path()).await?;

                    // If necessary, exhaust the reader to compute the hash.
                    hasher.finish().await.map_err(Error::HashExhaustion)?;

                    hashers.into_iter().map(HashDigest::from).collect()
                };

                // Persist the temporary directory to the directory store.
                let path = self
                    .build_context
                    .cache()
                    .persist(temp_dir.into_path(), wheel_entry.path())
                    .await
                    .map_err(Error::CacheRead)?;

                Ok(Archive::new(path, hashes))
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        let req = self.request(url.clone())?;
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.build_context
                    .cache()
                    .freshness(&http_entry, Some(&filename.name))
                    .map_err(Error::CacheRead)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };
        let archive = self
            .client
            .cached_client()
            .get_serde(req, &http_entry, cache_control, download)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => Error::Client(err),
            })?;

        // If the archive is missing the required hashes, force a refresh.
        let archive = if archive.has_digests(hashes) {
            archive
        } else {
            self.client
                .cached_client()
                .skip_cache(self.request(url)?, &http_entry, download)
                .await
                .map_err(|err| match err {
                    CachedClientError::Callback(err) => err,
                    CachedClientError::Client(err) => Error::Client(err),
                })?
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
        hashes: &[HashDigest],
    ) -> Result<LocalWheel, Error> {
        // Determine the last-modified time of the wheel.
        let modified = ArchiveTimestamp::from_file(path).map_err(Error::CacheRead)?;

        // Attempt to read the archive pointer from the cache.
        let archive_entry = wheel_entry.with_file(format!("{}.rev", filename.stem()));
        let archive = read_timestamped_archive(&archive_entry, modified)?;

        // If the file is already unzipped, and the cache is up-to-date, return it.
        if let Some(archive) = archive.filter(|archive| archive.has_digests(hashes)) {
            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: archive.path,
                hashes: archive.hashes,
                filename: filename.clone(),
            })
        } else if hashes.is_empty() {
            // Otherwise, unzip the wheel.
            let archive = Archive::new(self.unzip_wheel(path, wheel_entry.path()).await?, vec![]);
            write_timestamped_archive(&archive_entry, archive.clone(), modified).await?;

            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: archive.path,
                hashes: archive.hashes,
                filename: filename.clone(),
            })
        } else {
            // If necessary, compute the hashes of the wheel.
            let file = fs_err::tokio::File::open(path)
                .await
                .map_err(Error::CacheRead)?;
            let temp_dir = tempfile::tempdir_in(self.build_context.cache().root())
                .map_err(Error::CacheWrite)?;

            // Create a hasher for each hash algorithm.
            let algorithms = {
                let mut hash = hashes.iter().map(HashDigest::algorithm).collect::<Vec<_>>();
                hash.sort();
                hash.dedup();
                hash
            };
            let mut hashers = algorithms.into_iter().map(Hasher::from).collect::<Vec<_>>();
            let mut hasher = uv_extract::hash::HashReader::new(file, &mut hashers);

            // Unzip the wheel to a temporary directory.
            uv_extract::stream::unzip(&mut hasher, temp_dir.path()).await?;

            // Exhaust the reader to compute the hash.
            hasher.finish().await.map_err(Error::HashExhaustion)?;

            // Persist the temporary directory to the directory store.
            let archive = self
                .build_context
                .cache()
                .persist(temp_dir.into_path(), wheel_entry.path())
                .await
                .map_err(Error::CacheWrite)?;

            let hashes = hashers.into_iter().map(HashDigest::from).collect();

            // Write the archive pointer to the cache.
            let archive = Archive::new(archive, hashes);
            write_timestamped_archive(&archive_entry, archive.clone(), modified).await?;

            Ok(LocalWheel {
                dist: Dist::Built(dist.clone()),
                archive: archive.path,
                hashes: archive.hashes,
                filename: filename.clone(),
            })
        }
    }

    /// Unzip a wheel into the cache, returning the path to the unzipped directory.
    async fn unzip_wheel(&self, path: &Path, target: &Path) -> Result<PathBuf, Error> {
        let temp_dir = tokio::task::spawn_blocking({
            let path = path.to_owned();
            let root = self.build_context.cache().root().to_path_buf();
            move || -> Result<TempDir, uv_extract::Error> {
                // Unzip the wheel into a temporary directory.
                let temp_dir = tempfile::tempdir_in(root)?;
                uv_extract::unzip(fs_err::File::open(path)?, temp_dir.path())?;
                Ok(temp_dir)
            }
        })
        .await??;

        // Persist the temporary directory to the directory store.
        let archive = self
            .build_context
            .cache()
            .persist(temp_dir.into_path(), target)
            .await
            .map_err(Error::CacheWrite)?;

        Ok(archive)
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

    /// Return the [`IndexLocations`] used by this resolver.
    pub fn index_locations(&self) -> &IndexLocations {
        self.build_context.index_locations()
    }
}

/// Write a timestamped archive path to the cache.
async fn write_timestamped_archive(
    cache_entry: &CacheEntry,
    data: Archive,
    modified: ArchiveTimestamp,
) -> Result<(), Error> {
    write_atomic(
        cache_entry.path(),
        rmp_serde::to_vec(&CachedByTimestamp {
            timestamp: modified.timestamp(),
            data,
        })?,
    )
    .await
    .map_err(Error::CacheWrite)
}

/// Read an existing timestamped archive path, if it exists and is up-to-date.
pub fn read_timestamped_archive(
    cache_entry: &CacheEntry,
    modified: ArchiveTimestamp,
) -> Result<Option<Archive>, Error> {
    match fs_err::read(cache_entry.path()) {
        Ok(cached) => {
            let cached = rmp_serde::from_slice::<CachedByTimestamp<Archive>>(&cached)?;
            if cached.timestamp == modified.timestamp() {
                return Ok(Some(cached.data));
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(Error::CacheRead(err)),
    }
    Ok(None)
}
