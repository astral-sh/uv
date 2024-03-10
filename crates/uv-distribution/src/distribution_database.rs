use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::{FutureExt, TryStreamExt};
use tokio::io::AsyncSeekExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{info_span, instrument, warn, Instrument};
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{
    BuiltDist, DirectGitUrl, Dist, FileLocation, IndexLocations, LocalEditable, Name, SourceDist,
};
use platform_tags::Tags;
use pypi_types::Metadata23;
use uv_cache::{ArchiveTarget, ArchiveTimestamp, Cache, CacheBucket, CacheEntry, WheelCache};
use uv_client::{CacheControl, CachedClientError, Connectivity, RegistryClient};
use uv_git::GitSource;
use uv_traits::{BuildContext, NoBinary, NoBuild};

use crate::download::{BuiltWheel, UnzippedWheel};
use crate::locks::Locks;
use crate::reporter::Facade;
use crate::{DiskWheel, Error, LocalWheel, Reporter, SourceDistCachedBuilder};

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
    cache: &'a Cache,
    reporter: Option<Arc<dyn Reporter>>,
    locks: Arc<Locks>,
    client: &'a RegistryClient,
    build_context: &'a Context,
    builder: SourceDistCachedBuilder<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> DistributionDatabase<'a, Context> {
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            cache,
            reporter: None,
            locks: Arc::new(Locks::default()),
            client,
            build_context,
            builder: SourceDistCachedBuilder::new(build_context, client, tags),
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter = Arc::new(reporter);
        Self {
            reporter: Some(reporter.clone()),
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
    /// If `no_remote_wheel` is set, the wheel will be built from a source distribution
    /// even if compatible pre-built wheels are available.
    #[instrument(skip(self))]
    pub async fn get_or_build_wheel(&self, dist: Dist) -> Result<LocalWheel, Error> {
        let no_binary = match self.build_context.no_binary() {
            NoBinary::None => false,
            NoBinary::All => true,
            NoBinary::Packages(packages) => packages.contains(dist.name()),
        };
        match &dist {
            Dist::Built(BuiltDist::Registry(wheel)) => {
                if no_binary {
                    return Err(Error::NoBinary);
                }

                let url = match &wheel.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        pypi_types::base_url_join_relative(base, url)?
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        Url::parse(url).map_err(|err| Error::Url(url.clone(), err))?
                    }
                    FileLocation::Path(path) => {
                        let url = Url::from_file_path(path).expect("path is absolute");
                        let cache_entry = self.cache.entry(
                            CacheBucket::Wheels,
                            WheelCache::Url(&url).remote_wheel_dir(wheel.name().as_ref()),
                            wheel.filename.stem(),
                        );

                        // If the file is already unzipped, and the unzipped directory is fresh,
                        // return it.
                        match cache_entry.path().canonicalize() {
                            Ok(archive) => {
                                if ArchiveTimestamp::up_to_date_with(
                                    path,
                                    ArchiveTarget::Cache(&archive),
                                )
                                .map_err(Error::CacheRead)?
                                {
                                    return Ok(LocalWheel::Unzipped(UnzippedWheel {
                                        dist: dist.clone(),
                                        archive,
                                        filename: wheel.filename.clone(),
                                    }));
                                }
                            }
                            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                            Err(err) => return Err(Error::CacheRead(err)),
                        }

                        // Otherwise, unzip the file.
                        return Ok(LocalWheel::Disk(DiskWheel {
                            dist: dist.clone(),
                            path: path.clone(),
                            target: cache_entry.into_path_buf(),
                            filename: wheel.filename.clone(),
                        }));
                    }
                };

                // Create a cache entry for the wheel.
                let wheel_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                // Download and unzip.
                match self
                    .stream_wheel(url.clone(), &wheel.filename, &wheel_entry, &dist)
                    .await
                {
                    Ok(archive) => Ok(LocalWheel::Unzipped(UnzippedWheel {
                        dist: dist.clone(),
                        archive,
                        filename: wheel.filename.clone(),
                    })),
                    Err(Error::Extract(err)) if err.is_http_streaming_unsupported() => {
                        warn!(
                            "Streaming unsupported for {dist}; downloading wheel to disk ({err})"
                        );

                        // If the request failed because streaming is unsupported, download the
                        // wheel directly.
                        let archive = self
                            .download_wheel(url, &wheel.filename, &wheel_entry, &dist)
                            .await?;
                        Ok(LocalWheel::Unzipped(UnzippedWheel {
                            dist: dist.clone(),
                            archive,
                            filename: wheel.filename.clone(),
                        }))
                    }
                    Err(err) => Err(err),
                }
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                if no_binary {
                    return Err(Error::NoBinary);
                }

                // Create a cache entry for the wheel.
                let wheel_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                // Download and unzip.
                match self
                    .stream_wheel(
                        wheel.url.raw().clone(),
                        &wheel.filename,
                        &wheel_entry,
                        &dist,
                    )
                    .await
                {
                    Ok(archive) => Ok(LocalWheel::Unzipped(UnzippedWheel {
                        dist: dist.clone(),
                        archive,
                        filename: wheel.filename.clone(),
                    })),
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
                                &dist,
                            )
                            .await?;
                        Ok(LocalWheel::Unzipped(UnzippedWheel {
                            dist: dist.clone(),
                            archive,
                            filename: wheel.filename.clone(),
                        }))
                    }
                    Err(err) => Err(err),
                }
            }

            Dist::Built(BuiltDist::Path(wheel)) => {
                if no_binary {
                    return Err(Error::NoBinary);
                }

                let cache_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );

                // If the file is already unzipped, and the unzipped directory is fresh,
                // return it.
                match cache_entry.path().canonicalize() {
                    Ok(archive) => {
                        if ArchiveTimestamp::up_to_date_with(
                            &wheel.path,
                            ArchiveTarget::Cache(&archive),
                        )
                        .map_err(Error::CacheRead)?
                        {
                            return Ok(LocalWheel::Unzipped(UnzippedWheel {
                                dist: dist.clone(),
                                archive,
                                filename: wheel.filename.clone(),
                            }));
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(Error::CacheRead(err)),
                }

                Ok(LocalWheel::Disk(DiskWheel {
                    dist: dist.clone(),
                    path: wheel.path.clone(),
                    target: cache_entry.into_path_buf(),
                    filename: wheel.filename.clone(),
                }))
            }

            Dist::Source(source_dist) => {
                let lock = self.locks.acquire(&dist).await;
                let _guard = lock.lock().await;

                let built_wheel = self.builder.download_and_build(source_dist).boxed().await?;

                // If the wheel was unzipped previously, respect it. Source distributions are
                // cached under a unique build ID, so unzipped directories are never stale.
                match built_wheel.target.canonicalize() {
                    Ok(archive) => Ok(LocalWheel::Unzipped(UnzippedWheel {
                        dist: dist.clone(),
                        archive,
                        filename: built_wheel.filename,
                    })),
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        Ok(LocalWheel::Built(BuiltWheel {
                            dist: dist.clone(),
                            path: built_wheel.path,
                            target: built_wheel.target,
                            filename: built_wheel.filename,
                        }))
                    }
                    Err(err) => return Err(Error::CacheRead(err)),
                }
            }
        }
    }

    /// Either fetch the only wheel metadata (directly from the index or with range requests) or
    /// fetch and build the source distribution.
    ///
    /// Returns the [`Metadata23`], along with a "precise" URL for the source distribution, if
    /// possible. For example, given a Git dependency with a reference to a branch or tag, return a
    /// URL with a precise reference to the current commit of that branch or tag.
    #[instrument(skip_all, fields(%dist))]
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
    ) -> Result<(Metadata23, Option<Url>), Error> {
        match dist {
            Dist::Built(built_dist) => {
                match self.client.wheel_metadata(built_dist).boxed().await {
                    Ok(metadata) => Ok((metadata, None)),
                    Err(err) if err.is_http_streaming_unsupported() => {
                        warn!("Streaming unsupported when fetching metadata for {dist}; downloading wheel directly ({err})");

                        // If the request failed due to an error that could be resolved by
                        // downloading the wheel directly, try that.
                        let wheel = self.get_or_build_wheel(dist.clone()).await?;
                        Ok((wheel.metadata()?, None))
                    }
                    Err(err) => Err(err.into()),
                }
            }
            Dist::Source(source_dist) => {
                let no_build = match self.build_context.no_build() {
                    NoBuild::All => true,
                    NoBuild::None => false,
                    NoBuild::Packages(packages) => packages.contains(source_dist.name()),
                };
                // Optimization: Skip source dist download when we must not build them anyway.
                if no_build {
                    return Err(Error::NoBuild);
                }

                let lock = self.locks.acquire(dist).await;
                let _guard = lock.lock().await;

                // Insert the `precise` URL, if it exists.
                let precise = self.precise(source_dist).await?;

                let source_dist = match precise.as_ref() {
                    Some(url) => Cow::Owned(source_dist.clone().with_url(url.clone())),
                    None => Cow::Borrowed(source_dist),
                };

                let metadata = self
                    .builder
                    .download_and_build_metadata(&source_dist)
                    .boxed()
                    .await?;
                Ok((metadata, precise))
            }
        }
    }

    /// Build a directory into an editable wheel.
    pub async fn build_wheel_editable(
        &self,
        editable: &LocalEditable,
        editable_wheel_dir: &Path,
    ) -> Result<(LocalWheel, Metadata23), Error> {
        let (dist, disk_filename, filename, metadata) = self
            .builder
            .build_editable(editable, editable_wheel_dir)
            .await?;

        let built_wheel = BuiltWheel {
            dist,
            filename,
            path: editable_wheel_dir.join(disk_filename),
            target: editable_wheel_dir.join(cache_key::digest(&editable.path)),
        };
        Ok((LocalWheel::Built(built_wheel), metadata))
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    async fn precise(&self, dist: &SourceDist) -> Result<Option<Url>, Error> {
        let SourceDist::Git(source_dist) = dist else {
            return Ok(None);
        };
        let git_dir = self.build_context.cache().bucket(CacheBucket::Git);

        let DirectGitUrl { url, subdirectory } =
            DirectGitUrl::try_from(source_dist.url.raw()).map_err(Error::Git)?;

        // If the commit already contains a complete SHA, short-circuit.
        if url.precise().is_some() {
            return Ok(None);
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let source = if let Some(reporter) = self.reporter.clone() {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter))
        } else {
            GitSource::new(url, git_dir)
        };
        let precise = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(Error::Git)?;
        let url = precise.into_git();

        // Re-encode as a URL.
        Ok(Some(Url::from(DirectGitUrl { url, subdirectory })))
    }

    /// Stream a wheel from a URL, unzipping it into the cache as it's downloaded.
    async fn stream_wheel(
        &self,
        url: Url,
        filename: &WheelFilename,
        wheel_entry: &CacheEntry,
        dist: &Dist,
    ) -> Result<PathBuf, Error> {
        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.stem()));

        let download = |response: reqwest::Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Download and unzip the wheel to a temporary directory.
                let temp_dir =
                    tempfile::tempdir_in(self.cache.root()).map_err(Error::CacheWrite)?;
                uv_extract::stream::unzip(reader.compat(), temp_dir.path()).await?;

                // Persist the temporary directory to the directory store.
                let archive = self
                    .cache
                    .persist(temp_dir.into_path(), wheel_entry.path())
                    .map_err(Error::CacheRead)?;
                Ok(archive)
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        let req = self
            .client
            .cached_client()
            .uncached()
            .get(url)
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
            .build()?;
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.cache
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

        Ok(archive)
    }

    /// Download a wheel from a URL, then unzip it into the cache.
    async fn download_wheel(
        &self,
        url: Url,
        filename: &WheelFilename,
        wheel_entry: &CacheEntry,
        dist: &Dist,
    ) -> Result<PathBuf, Error> {
        // Create an entry for the HTTP cache.
        let http_entry = wheel_entry.with_file(format!("{}.http", filename.stem()));

        let download = |response: reqwest::Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                // Download the wheel to a temporary file.
                let temp_file =
                    tempfile::tempfile_in(self.cache.root()).map_err(Error::CacheWrite)?;
                let mut writer = tokio::io::BufWriter::new(tokio::fs::File::from_std(temp_file));
                tokio::io::copy(&mut reader.compat(), &mut writer)
                    .await
                    .map_err(Error::CacheWrite)?;

                // Unzip the wheel to a temporary directory.
                let temp_dir =
                    tempfile::tempdir_in(self.cache.root()).map_err(Error::CacheWrite)?;
                let mut file = writer.into_inner();
                file.seek(io::SeekFrom::Start(0))
                    .await
                    .map_err(Error::CacheWrite)?;
                let reader = tokio::io::BufReader::new(file);
                uv_extract::seek::unzip(reader, temp_dir.path()).await?;

                // Persist the temporary directory to the directory store.
                let archive = self
                    .cache
                    .persist(temp_dir.into_path(), wheel_entry.path())
                    .map_err(Error::CacheRead)?;
                Ok(archive)
            }
            .instrument(info_span!("wheel", wheel = %dist))
        };

        let req = self
            .client
            .cached_client()
            .uncached()
            .get(url)
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
            .build()?;
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.cache
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

        Ok(archive)
    }

    /// Return the [`IndexLocations`] used by this resolver.
    pub fn index_locations(&self) -> &IndexLocations {
        self.build_context.index_locations()
    }
}
