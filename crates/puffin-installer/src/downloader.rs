use std::cmp::Reverse;
use std::path::Path;
use std::sync::Arc;

use futures::{FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use tokio::task::JoinError;
use tracing::{instrument, warn};
use url::Url;

use distribution_types::{CachedDist, Dist, Identifier, LocalEditable, RemoteSource, SourceDist};
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClient;
use puffin_distribution::{DistributionDatabase, DistributionDatabaseError, LocalWheel, Unzip};
use puffin_traits::{BuildContext, InFlight};

use crate::editable::BuiltEditable;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to unzip wheel: {0}")]
    Unzip(Dist, #[source] puffin_extract::Error),
    #[error("Failed to fetch wheel: {0}")]
    Fetch(Dist, #[source] DistributionDatabaseError),
    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Failed to build editable: {0}")]
    Editable(LocalEditable, #[source] DistributionDatabaseError),
    #[error("Unzip failed in another thread: {0}")]
    Thread(String),
}

/// Download, build, and unzip a set of distributions.
pub struct Downloader<'a, Context: BuildContext + Send + Sync> {
    database: DistributionDatabase<'a, Context>,
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
        }
    }

    /// Set the [`Reporter`] to use for this unzipper.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        let reporter: Arc<dyn Reporter> = Arc::new(reporter);
        Self {
            reporter: Some(reporter.clone()),
            database: self.database.with_reporter(Facade::from(reporter.clone())),
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
                    .map_err(|err| Error::Editable(editable.clone(), err))?;
                let cached_dist = Self::unzip_wheel(local_wheel).await?;
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
    #[instrument(skip_all, fields(name = % dist, size = ? dist.size(), url = dist.file().map(|file| file.url.to_string()).unwrap_or_default()))]
    pub async fn get_wheel(&self, dist: Dist, in_flight: &InFlight) -> Result<CachedDist, Error> {
        let id = dist.distribution_id();
        let wheel = if in_flight.downloads.register(&id) {
            let download: LocalWheel = self
                .database
                .get_or_build_wheel(dist.clone())
                .boxed()
                .map_err(|err| Error::Fetch(dist.clone(), err))
                .await?;
            let result = Self::unzip_wheel(download).await;
            match result {
                Ok(cached) => {
                    in_flight.downloads.done(id, Ok(cached.clone()));
                    cached
                }
                Err(err) => {
                    in_flight.downloads.done(id, Err(err.to_string()));
                    return Err(err);
                }
            }
        } else {
            in_flight
                .downloads
                .wait(&id)
                .await
                .value()
                .clone()
                .map_err(Error::Thread)?
        };

        Ok(wheel)
    }

    /// Unzip a locally-available wheel into the cache.
    async fn unzip_wheel(download: LocalWheel) -> Result<CachedDist, Error> {
        // Just an optimization: Avoid spawning a blocking task if there is no work to be done.
        if matches!(download, LocalWheel::Unzipped(_)) {
            return Ok(download.into_cached_dist());
        }

        // If the wheel is already unpacked, we should avoid attempting to unzip it at all.
        if download.target().is_dir() {
            warn!("Wheel is already unpacked: {download}");
            return Ok(download.into_cached_dist());
        }

        // Unzip the wheel.
        tokio::task::spawn_blocking({
            let download = download.clone();
            move || -> Result<(), puffin_extract::Error> {
                // Unzip the wheel into a temporary directory.
                let parent = download
                    .target()
                    .parent()
                    .expect("Cache paths can't be root");
                fs_err::create_dir_all(parent)?;
                let staging = tempfile::tempdir_in(parent)?;
                download.unzip(staging.path())?;

                // Move the unzipped wheel into the cache.
                if let Err(err) = fs_err::rename(staging.into_path(), download.target()) {
                    // If another thread already unpacked the wheel, we can ignore the error.
                    return if download.target().is_dir() {
                        warn!("Wheel is already unpacked: {download}");
                        Ok(())
                    } else {
                        Err(err.into())
                    };
                }

                Ok(())
            }
        })
        .await?
        .map_err(|err| Error::Unzip(download.remote().clone(), err))?;

        Ok(download.into_cached_dist())
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

/// A facade for converting from [`Reporter`] to [`puffin_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl From<Arc<dyn Reporter>> for Facade {
    fn from(reporter: Arc<dyn Reporter>) -> Self {
        Self { reporter }
    }
}

impl puffin_distribution::Reporter for Facade {
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
