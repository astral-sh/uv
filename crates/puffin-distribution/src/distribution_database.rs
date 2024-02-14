use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::sync::Arc;

use futures::{FutureExt, TryStreamExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{info_span, instrument, Instrument};
use url::Url;

use distribution_types::{
    BuiltDist, DirectGitUrl, Dist, FileLocation, IndexLocations, LocalEditable, Name, SourceDist,
};
use platform_tags::Tags;
use pypi_types::Metadata21;
use uv_cache::{Cache, CacheBucket, Timestamp, WheelCache};
use uv_client::{CacheControl, CachedClientError, Connectivity, RegistryClient};
use uv_fs::metadata_if_exists;
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
                                if let (Some(cache_metadata), Some(path_metadata)) = (
                                    metadata_if_exists(&archive).map_err(Error::CacheRead)?,
                                    metadata_if_exists(path).map_err(Error::CacheRead)?,
                                ) {
                                    let cache_modified = Timestamp::from_metadata(&cache_metadata);
                                    let path_modified = Timestamp::from_metadata(&path_metadata);
                                    if cache_modified >= path_modified {
                                        return Ok(LocalWheel::Unzipped(UnzippedWheel {
                                            dist: dist.clone(),
                                            archive,
                                            filename: wheel.filename.clone(),
                                        }));
                                    }
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

                // Create an entry for the wheel itself alongside its HTTP cache.
                let wheel_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Index(&wheel.index).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );
                let http_entry = wheel_entry.with_file(format!("{}.http", wheel.filename.stem()));

                let download = |response: reqwest::Response| {
                    async {
                        let reader = response
                            .bytes_stream()
                            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
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
                    .instrument(info_span!("download", wheel = %wheel))
                };

                let req = self.client.cached_client().uncached().get(url).build()?;
                let cache_control = match self.client.connectivity() {
                    Connectivity::Online => CacheControl::from(
                        self.cache
                            .freshness(&http_entry, Some(wheel.name()))
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

                Ok(LocalWheel::Unzipped(UnzippedWheel {
                    dist: dist.clone(),
                    archive,
                    filename: wheel.filename.clone(),
                }))
            }

            Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                if no_binary {
                    return Err(Error::NoBinary);
                }

                // Create an entry for the wheel itself alongside its HTTP cache.
                let wheel_entry = self.cache.entry(
                    CacheBucket::Wheels,
                    WheelCache::Url(&wheel.url).remote_wheel_dir(wheel.name().as_ref()),
                    wheel.filename.stem(),
                );
                let http_entry = wheel_entry.with_file(format!("{}.http", wheel.filename.stem()));

                let download = |response: reqwest::Response| {
                    async {
                        let reader = response
                            .bytes_stream()
                            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
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
                    .instrument(info_span!("download", wheel = %wheel))
                };

                let req = self
                    .client
                    .cached_client()
                    .uncached()
                    .get(wheel.url.raw().clone())
                    .build()?;
                let cache_control = match self.client.connectivity() {
                    Connectivity::Online => CacheControl::from(
                        self.cache
                            .freshness(&http_entry, Some(wheel.name()))
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

                Ok(LocalWheel::Unzipped(UnzippedWheel {
                    dist: dist.clone(),
                    archive,
                    filename: wheel.filename.clone(),
                }))
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
                        if let (Some(cache_metadata), Some(path_metadata)) = (
                            metadata_if_exists(&archive).map_err(Error::CacheRead)?,
                            metadata_if_exists(&wheel.path).map_err(Error::CacheRead)?,
                        ) {
                            let cache_modified = Timestamp::from_metadata(&cache_metadata);
                            let path_modified = Timestamp::from_metadata(&path_metadata);
                            if cache_modified >= path_modified {
                                return Ok(LocalWheel::Unzipped(UnzippedWheel {
                                    dist: dist.clone(),
                                    archive,
                                    filename: wheel.filename.clone(),
                                }));
                            }
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
    /// Returns the [`Metadata21`], along with a "precise" URL for the source distribution, if
    /// possible. For example, given a Git dependency with a reference to a branch or tag, return a
    /// URL with a precise reference to the current commit of that branch or tag.
    #[instrument(skip_all, fields(%dist))]
    pub async fn get_or_build_wheel_metadata(
        &self,
        dist: &Dist,
    ) -> Result<(Metadata21, Option<Url>), Error> {
        match dist {
            Dist::Built(built_dist) => {
                Ok((self.client.wheel_metadata(built_dist).boxed().await?, None))
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
    ) -> Result<(LocalWheel, Metadata21), Error> {
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

    pub fn index_locations(&self) -> &IndexLocations {
        self.build_context.index_locations()
    }
}
