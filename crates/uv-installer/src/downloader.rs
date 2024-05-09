use std::cmp::Reverse;
use std::path::Path;
use std::sync::Arc;

use futures::{stream::FuturesUnordered, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use tokio::task::JoinError;
use tracing::instrument;
use url::Url;

use distribution_types::{
    BuildableSource, CachedDist, Dist, Hashed, Identifier, LocalEditable, LocalEditables,
    RemoteSource,
};
use platform_tags::Tags;
use uv_cache::Cache;
use uv_distribution::{DistributionDatabase, LocalWheel};
use uv_types::{BuildContext, HashStrategy, InFlight};

use crate::editable::BuiltEditable;

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

/// Download, build, and unzip a set of distributions.
pub struct Downloader<'a, Context: BuildContext> {
    tags: &'a Tags,
    cache: &'a Cache,
    hashes: &'a HashStrategy,
    database: DistributionDatabase<'a, Context>,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext> Downloader<'a, Context> {
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

    /// Set the [`Reporter`] to use for this downloader.
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
    pub fn download_stream<'stream>(
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

    /// Download, build, and unzip a set of downloaded wheels.
    #[instrument(skip_all, fields(total = distributions.len()))]
    pub async fn download(
        &self,
        mut distributions: Vec<Dist>,
        in_flight: &InFlight,
    ) -> Result<Vec<CachedDist>, Error> {
        // Sort the distributions by size.
        distributions
            .sort_unstable_by_key(|distribution| Reverse(distribution.size().unwrap_or(u64::MAX)));

        let wheels = self
            .download_stream(distributions, in_flight)
            .try_collect()
            .await?;

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(wheels)
    }

    /// Build a set of editables
    #[instrument(skip_all)]
    pub async fn build_editables(
        &self,
        editables: LocalEditables,
        editable_wheel_dir: &Path,
    ) -> Result<Vec<BuiltEditable>, Error> {
        // Build editables in parallel
        let mut results = Vec::with_capacity(editables.len());
        let mut fetches = editables
            .into_iter()
            .map(|editable| async move {
                let task_id = self
                    .reporter
                    .as_ref()
                    .map(|reporter| reporter.on_editable_build_start(&editable));
                let (local_wheel, metadata) = self
                    .database
                    .build_wheel_editable(&editable, editable_wheel_dir)
                    .await
                    .map_err(Error::Editable)?;
                let cached_dist = CachedDist::from(local_wheel);
                if let Some(task_id) = task_id {
                    if let Some(reporter) = &self.reporter {
                        reporter.on_editable_build_complete(&editable, task_id);
                    }
                }
                Ok::<_, Error>((editable, cached_dist, metadata))
            })
            .collect::<FuturesUnordered<_>>();

        while let Some((editable, wheel, metadata)) = fetches.next().await.transpose()? {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_progress(&wheel);
            }
            results.push(BuiltEditable {
                editable,
                wheel,
                metadata,
            });
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(results)
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

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, source: &BuildableSource) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, source: &BuildableSource, id: usize);

    /// Callback to invoke when a editable build is kicked off.
    fn on_editable_build_start(&self, dist: &LocalEditable) -> usize;

    /// Callback to invoke when a editable build is complete.
    fn on_editable_build_complete(&self, dist: &LocalEditable, id: usize);

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
}
