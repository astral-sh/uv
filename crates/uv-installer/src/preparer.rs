use std::cmp::Reverse;
use std::sync::Arc;

use futures::{stream::FuturesUnordered, FutureExt, Stream, TryFutureExt, TryStreamExt};
use tracing::{debug, instrument};
use url::Url;

use uv_cache::Cache;
use uv_configuration::BuildOptions;
use uv_distribution::{DistributionDatabase, LocalWheel};
use uv_distribution_types::{
    BuildableSource, CachedDist, DerivationChain, Dist, DistErrorKind, Hashed, Identifier, Name,
    RemoteSource, Resolution,
};
use uv_pep508::PackageName;
use uv_platform_tags::Tags;
use uv_types::{BuildContext, HashStrategy, InFlight};

/// Prepare distributions for installation.
///
/// Downloads, builds, and unzips a set of distributions.
pub struct Preparer<'a, Context: BuildContext> {
    tags: &'a Tags,
    cache: &'a Cache,
    hashes: &'a HashStrategy,
    build_options: &'a BuildOptions,
    database: DistributionDatabase<'a, Context>,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext> Preparer<'a, Context> {
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        hashes: &'a HashStrategy,
        build_options: &'a BuildOptions,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            tags,
            cache,
            hashes,
            build_options,
            database,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for operations.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            tags: self.tags,
            cache: self.cache,
            hashes: self.hashes,
            build_options: self.build_options,
            database: self
                .database
                .with_reporter(reporter.clone().into_distribution_reporter()),
            reporter: Some(reporter),
        }
    }

    /// Fetch, build, and unzip the distributions in parallel.
    pub fn prepare_stream<'stream>(
        &'stream self,
        distributions: Vec<Arc<Dist>>,
        in_flight: &'stream InFlight,
        resolution: &'stream Resolution,
    ) -> impl Stream<Item = Result<CachedDist, Error>> + 'stream {
        distributions
            .into_iter()
            .map(|dist| async move {
                let wheel = self
                    .get_wheel((*dist).clone(), in_flight, resolution)
                    .boxed_local()
                    .await?;
                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_progress(&wheel);
                }
                Ok::<CachedDist, Error>(wheel)
            })
            .collect::<FuturesUnordered<_>>()
    }

    /// Download, build, and unzip a set of distributions.
    #[instrument(skip_all, fields(total = distributions.len()))]
    pub async fn prepare(
        &self,
        mut distributions: Vec<Arc<Dist>>,
        in_flight: &InFlight,
        resolution: &Resolution,
    ) -> Result<Vec<CachedDist>, Error> {
        // Sort the distributions by size.
        distributions
            .sort_unstable_by_key(|distribution| Reverse(distribution.size().unwrap_or(u64::MAX)));

        let wheels = self
            .prepare_stream(distributions, in_flight, resolution)
            .try_collect()
            .await?;

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(wheels)
    }
    /// Download, build, and unzip a single wheel.
    #[instrument(skip_all, fields(name = % dist, size = ? dist.size(), url = dist.file().map(| file | file.url.to_string()).unwrap_or_default()))]
    pub async fn get_wheel(
        &self,
        dist: Dist,
        in_flight: &InFlight,
        resolution: &Resolution,
    ) -> Result<CachedDist, Error> {
        // Validate that the distribution is compatible with the build options.
        match dist {
            Dist::Built(ref dist) => {
                if self.build_options.no_binary_package(dist.name()) {
                    return Err(Error::NoBinary(dist.name().clone()));
                }
            }
            Dist::Source(ref dist) => {
                if self.build_options.no_build_package(dist.name()) {
                    if dist.is_editable() {
                        debug!("Allowing build for editable source distribution: {dist}");
                    } else {
                        return Err(Error::NoBuild(dist.name().clone()));
                    }
                }
            }
        }

        let id = dist.distribution_id();
        if in_flight.downloads.register(id.clone()) {
            let policy = self.hashes.get(&dist);

            let result = self
                .database
                .get_or_build_wheel(&dist, self.tags, policy)
                .boxed_local()
                .map_err(|err| Error::from_dist(dist.clone(), err, resolution))
                .await
                .and_then(|wheel: LocalWheel| {
                    if wheel.satisfies(policy) {
                        Ok(wheel)
                    } else {
                        let err = uv_distribution::Error::hash_mismatch(
                            dist.to_string(),
                            policy.digests(),
                            wheel.hashes(),
                        );
                        Err(Error::from_dist(dist, err, resolution))
                    }
                })
                .map(CachedDist::from);
            match result {
                Ok(cached) => {
                    in_flight.downloads.done(id, Ok(cached.clone()));
                    Ok(cached)
                }
                Err(err) => {
                    in_flight.downloads.done(id, Err(err.to_string()));
                    Err(err)
                }
            }
        } else {
            let result = in_flight
                .downloads
                .wait(&id)
                .await
                .expect("missing value for registered task");

            match result.as_ref() {
                Ok(cached) => {
                    // Validate that the wheel is compatible with the distribution.
                    //
                    // `get_or_build_wheel` is guaranteed to return a wheel that matches the
                    // distribution. But there could be multiple requested distributions that share
                    // a cache entry in `in_flight`, so we need to double-check here.
                    //
                    // For example, if two requirements are based on the same local path, but use
                    // different names, then they'll share an `in_flight` entry, but one of the two
                    // should be rejected (since at least one of the names will not match the
                    // package name).
                    if *dist.name() != cached.filename().name {
                        let err = uv_distribution::Error::WheelMetadataNameMismatch {
                            given: dist.name().clone(),
                            metadata: cached.filename().name.clone(),
                        };
                        return Err(Error::from_dist(dist, err, resolution));
                    }
                    if let Some(version) = dist.version() {
                        if *version != cached.filename().version
                            && *version != cached.filename().version.clone().without_local()
                        {
                            let err = uv_distribution::Error::WheelMetadataVersionMismatch {
                                given: version.clone(),
                                metadata: cached.filename().version.clone(),
                            };
                            return Err(Error::from_dist(dist, err, resolution));
                        }
                    }
                    Ok(cached.clone())
                }
                Err(err) => Err(Error::Thread(err.to_string())),
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Building source distributions is disabled, but attempted to build `{0}`")]
    NoBuild(PackageName),
    #[error("Using pre-built wheels is disabled, but attempted to use `{0}`")]
    NoBinary(PackageName),
    #[error("{0} `{1}`")]
    Dist(
        DistErrorKind,
        Box<Dist>,
        DerivationChain,
        #[source] uv_distribution::Error,
    ),
    #[error("Cyclic build dependency detected for `{0}`")]
    CyclicBuildDependency(PackageName),
    #[error("Unzip failed in another thread: {0}")]
    Thread(String),
}

impl Error {
    /// Create an [`Error`] from a distribution error.
    fn from_dist(dist: Dist, err: uv_distribution::Error, resolution: &Resolution) -> Self {
        let chain =
            DerivationChain::from_resolution(resolution, (&dist).into()).unwrap_or_default();
        Self::Dist(
            DistErrorKind::from_dist(&dist, &err),
            Box::new(dist),
            chain,
            err,
        )
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is unzipped. This implies that the wheel was downloaded and,
    /// if necessary, built.
    fn on_progress(&self, dist: &CachedDist);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);

    /// Callback to invoke when a download is kicked off.
    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize;

    /// Callback to invoke when a download makes progress (i.e. some number of bytes are
    /// downloaded).
    fn on_download_progress(&self, index: usize, bytes: u64);

    /// Callback to invoke when a download is complete.
    fn on_download_complete(&self, name: &PackageName, index: usize);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, source: &BuildableSource) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, source: &BuildableSource, id: usize);

    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

impl dyn Reporter {
    /// Converts this reporter to a [`uv_distribution::Reporter`].
    pub(crate) fn into_distribution_reporter(
        self: Arc<dyn Reporter>,
    ) -> Arc<dyn uv_distribution::Reporter> {
        Arc::new(Facade {
            reporter: self.clone(),
        })
    }
}

/// A facade for converting from [`Reporter`] to [`uv_distribution::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl uv_distribution::Reporter for Facade {
    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(source)
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(source, id);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name, size)
    }

    fn on_download_progress(&self, index: usize, inc: u64) {
        self.reporter.on_download_progress(index, inc);
    }

    fn on_download_complete(&self, name: &PackageName, index: usize) {
        self.reporter.on_download_complete(name, index);
    }
}
