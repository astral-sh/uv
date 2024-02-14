use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::{FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use tokio::task::JoinError;
use tracing::instrument;
use url::Url;

use distribution_types::{CachedDist, Dist, Identifier, LocalEditable, RemoteSource, SourceDist};
use platform_tags::Tags;
use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_distribution::{DistributionDatabase, LocalWheel, Unzip};
use uv_traits::{BuildContext, InFlight};

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
    #[error("Unzip failed in another thread: {0}")]
    Thread(String),
}

/// Download, build, and unzip a set of distributions.
pub struct Downloader<'a, Context: BuildContext + Send + Sync> {
    database: DistributionDatabase<'a, Context>,
    cache: &'a Cache,
    reporter: Option<Arc<dyn Reporter>>,
}

impl<'a, Context: BuildContext + Send + Sync> Downloader<'a, Context> {
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        client: &'a RegistryClient,
        build_context: &'a Context,
    ) -> Self {
        Self {
            database: DistributionDatabase::new(cache, tags, client, build_context),
            reporter: None,
            cache,
        }
    }

    /// Set the [`Reporter`] to use for this unzipper.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter: Arc<dyn Reporter> = Arc::new(reporter);
        Self {
            reporter: Some(reporter.clone()),
            database: self.database.with_reporter(Facade::from(reporter.clone())),
            cache: self.cache,
        }
    }

    /// Fetch, build, and unzip the distributions in parallel.
    pub fn download_stream<'stream>(
        &'stream self,
        distributions: Vec<Dist>,
        in_flight: &'stream InFlight,
    ) -> impl Stream<Item = Result<CachedDist, Error>> + 'stream {
        futures::stream::iter(distributions)
            .map(|dist| async {
                let wheel = self.get_wheel(dist, in_flight).boxed().await?;
                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_progress(&wheel);
                }
                Ok::<CachedDist, Error>(wheel)
            })
            // TODO(charlie): The number of concurrent fetches, such that we limit the number of
            // concurrent builds to the number of cores, while allowing more concurrent downloads.
            .buffer_unordered(50)
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
        editables: Vec<LocalEditable>,
        editable_wheel_dir: &Path,
    ) -> Result<Vec<BuiltEditable>, Error> {
        // Build editables in parallel
        let mut results = Vec::with_capacity(editables.len());
        let mut fetches = futures::stream::iter(editables)
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
                let cached_dist = self.unzip_wheel(local_wheel).await?;
                if let Some(task_id) = task_id {
                    if let Some(reporter) = &self.reporter {
                        reporter.on_editable_build_complete(&editable, task_id);
                    }
                }
                Ok::<_, Error>((editable, cached_dist, metadata))
            })
            .buffer_unordered(50);

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
            let download: LocalWheel = self
                .database
                .get_or_build_wheel(dist.clone())
                .boxed()
                .map_err(|err| Error::Fetch(dist.clone(), err))
                .await?;
            let result = self.unzip_wheel(download).await;
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

    /// Unzip a locally-available wheel into the cache.
    async fn unzip_wheel(&self, download: LocalWheel) -> Result<CachedDist, Error> {
        // Just an optimization: Avoid spawning a blocking task if there is no work to be done.
        if let LocalWheel::Unzipped(download) = download {
            return Ok(download.into_cached_dist());
        }

        // Unzip the wheel.
        let archive = tokio::task::spawn_blocking({
            let download = download.clone();
            let cache = self.cache.clone();
            move || -> Result<PathBuf, uv_extract::Error> {
                // Unzip the wheel into a temporary directory.
                let temp_dir = tempfile::tempdir_in(cache.root())?;
                download.unzip(temp_dir.path())?;

                // Persist the temporary directory to the directory store.
                Ok(cache.persist(temp_dir.into_path(), download.target())?)
            }
        })
        .await?
        .map_err(|err| Error::Unzip(download.remote().clone(), err))?;

        Ok(download.into_cached_dist(archive))
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is unzipped. This implies that the wheel was downloaded and,
    /// if necessary, built.
    fn on_progress(&self, dist: &CachedDist);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, dist: &SourceDist) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, dist: &SourceDist, id: usize);

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
    fn on_build_start(&self, dist: &SourceDist) -> usize {
        self.reporter.on_build_start(dist)
    }

    fn on_build_complete(&self, dist: &SourceDist, id: usize) {
        self.reporter.on_build_complete(dist, id);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}
