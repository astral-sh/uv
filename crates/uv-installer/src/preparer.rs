use std::cmp::Reverse;
use std::sync::Arc;

use futures::{stream::FuturesUnordered, FutureExt, Stream, TryFutureExt, TryStreamExt};
use pep508_rs::PackageName;
use tokio::task::JoinError;
use tracing::instrument;
use url::Url;

use distribution_types::{BuildableSource, CachedDist, Dist, Hashed, Identifier, RemoteSource};
use platform_tags::Tags;
use uv_cache::Cache;
use uv_distribution::{DistributionDatabase, LocalWheel};
use uv_types::{BuildContext, HashStrategy, InFlight};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to unzip wheel: {0}")]
    Unzip(Dist, #[source] uv_extract::Error),
    #[error("Failed to fetch wheel: {0}")]
    Fetch(Dist, #[source] uv_distribution::Error),
    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error(transparent)]
    Editable(#[from] uv_distribution::Error),
    #[error("Failed to write to the client cache")]
    CacheWrite(#[source] std::io::Error),
    #[error("Unzip failed in another thread: {0}")]
    Thread(String),
}

/// Prepare distributions for installation.
///
/// Downloads, builds, and unzips a set of distributions.
pub struct Preparer<'a, Context: BuildContext> {
    tags: &'a Tags,
    cache: &'a Cache,
    hashes: &'a HashStrategy,
    database: DistributionDatabase<'a, Context>,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext> Preparer<'a, Context> {
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        hashes: &'a HashStrategy,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            tags,
            cache,
            hashes,
            database,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for operations.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter: Arc<dyn Reporter> = Arc::new(reporter);
        Self {
            tags: self.tags,
            cache: self.cache,
            hashes: self.hashes,
            database: self.database.with_reporter(Facade::from(reporter.clone())),
            reporter: Some(reporter.clone()),
        }
    }

    /// Fetch, build, and unzip the distributions in parallel.
    pub fn prepare_stream<'stream>(
        &'stream self,
        distributions: Vec<Dist>,
        in_flight: &'stream InFlight,
    ) -> impl Stream<Item = Result<CachedDist, Error>> + 'stream {
        distributions
            .into_iter()
            .map(|dist| async {
                let wheel = self.get_wheel(dist, in_flight).boxed_local().await?;
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
        mut distributions: Vec<Dist>,
        in_flight: &InFlight,
    ) -> Result<Vec<CachedDist>, Error> {
        // Sort the distributions by size.
        distributions
            .sort_unstable_by_key(|distribution| Reverse(distribution.size().unwrap_or(u64::MAX)));

        let wheels = self
            .prepare_stream(distributions, in_flight)
            .try_collect()
            .await?;

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(wheels)
    }

    /// Download, build, and unzip a single wheel.
    #[instrument(skip_all, fields(name = % dist, size = ? dist.size(), url = dist.file().map(| file | file.url.to_string()).unwrap_or_default()))]
    pub async fn get_wheel(&self, dist: Dist, in_flight: &InFlight) -> Result<CachedDist, Error> {
        let id = dist.distribution_id();
        if in_flight.downloads.register(id.clone()) {
            let policy = self.hashes.get(&dist);

            let result = self
                .database
                .get_or_build_wheel(&dist, self.tags, policy)
                .boxed_local()
                .map_err(|err| Error::Fetch(dist.clone(), err))
                .await
                .and_then(|wheel: LocalWheel| {
                    if wheel.satisfies(policy) {
                        Ok(wheel)
                    } else {
                        Err(Error::Fetch(
                            dist.clone(),
                            uv_distribution::Error::hash_mismatch(
                                dist.to_string(),
                                policy.digests(),
                                wheel.hashes(),
                            ),
                        ))
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
                Ok(cached) => Ok(cached.clone()),
                Err(err) => Err(Error::Thread(err.to_string())),
            }
        }
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

/// A facade for converting from [`Reporter`] to [`uv_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl From<Arc<dyn Reporter>> for Facade {
    fn from(reporter: Arc<dyn Reporter>) -> Self {
        Self { reporter }
    }
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
