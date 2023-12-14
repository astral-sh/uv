use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::Arc;

use futures::{StreamExt, TryFutureExt};
use tokio::task::JoinError;
use tracing::{instrument, warn};
use url::Url;

use distribution_types::{CachedDist, Dist, Metadata, RemoteSource, SourceDist};
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClient;
use puffin_distribution::{DistributionDatabase, DistributionDatabaseError, LocalWheel, Unzip};
use puffin_traits::{BuildContext, OnceMap};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to unzip wheel: {0}")]
    Unzip(Dist, #[source] puffin_extract::Error),
    #[error("Failed to fetch wheel: {0}")]
    Fetch(Dist, #[source] DistributionDatabaseError),
    /// Should not occur; only seen when another task panicked.
    #[error("The task executor is broken, did some other task panic?")]
    Join(#[from] JoinError),
    #[error("Unzip failed in another thread: {0}")]
    Thread(String),
}

/// Download, build, and unzip a set of distributions.
pub struct Downloader<'a, Context: BuildContext + Send + Sync> {
    cache: &'a Cache,
    tags: &'a Tags,
    client: &'a RegistryClient,
    build_context: &'a Context,
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
            cache,
            tags,
            client,
            build_context,
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this unzipper.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Arc::new(reporter)),
            ..self
        }
    }

    /// Unzip a set of downloaded wheels.
    #[instrument(skip_all)]
    pub async fn download(
        &self,
        distributions: Vec<Dist>,
        in_flight: &OnceMap<PathBuf, Result<CachedDist, String>>,
    ) -> Result<Vec<CachedDist>, Error> {
        let database = if let Some(reporter) = self.reporter.as_ref() {
            DistributionDatabase::new(self.cache, self.tags, self.client, self.build_context)
                .with_reporter(Facade::from(reporter.clone()))
        } else {
            DistributionDatabase::new(self.cache, self.tags, self.client, self.build_context)
        };

        // Sort the distributions by size.
        let mut distributions = distributions;
        distributions.sort_unstable_by_key(|distribution| {
            Reverse(distribution.size().unwrap_or(usize::MAX))
        });

        // Fetch, build, and unzip the distributions in parallel.
        // TODO(charlie): The number of concurrent fetches, such that we limit the number of
        // concurrent builds to the number of cores, while allowing more concurrent downloads.
        let mut wheels = Vec::with_capacity(distributions.len());
        let mut fetches = futures::stream::iter(distributions)
            .map(|dist| Self::get_wheel(dist, &database, in_flight))
            .buffer_unordered(50);

        while let Some(wheel) = fetches.next().await.transpose()? {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_progress(&wheel);
            }
            wheels.push(wheel);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(wheels)
    }

    /// Download, build, and unzip a single wheel.
    async fn get_wheel(
        dist: Dist,
        database: &DistributionDatabase<'a, Context>,
        in_flight: &OnceMap<PathBuf, Result<CachedDist, String>>,
    ) -> Result<CachedDist, Error> {
        // TODO(charlie): Add in-flight tracking around `get_or_build_wheel`.
        let download: LocalWheel = database
            .get_or_build_wheel(dist.clone())
            .map_err(|err| Error::Fetch(dist.clone(), err))
            .await?;

        let target = download.target().to_path_buf();
        let wheel = if in_flight.register(&target) {
            let result = Self::unzip_wheel(download).await;
            match result {
                Ok(cached) => {
                    in_flight.done(target, Ok(cached.clone()));
                    cached
                }
                Err(err) => {
                    in_flight.done(target, Err(err.to_string()));
                    return Err(err);
                }
            }
        } else {
            in_flight
                .wait(&target)
                .await
                .value()
                .clone()
                .map_err(Error::Thread)?
        };

        Ok(wheel)
    }

    /// Unzip a locally-available wheel into the cache.
    async fn unzip_wheel(download: LocalWheel) -> Result<CachedDist, Error> {
        let remote = download.remote().clone();
        let filename = download.filename().clone();

        // If the wheel is already unpacked, we should avoid attempting to unzip it at all.
        if download.target().is_dir() {
            warn!("Wheel is already unpacked: {remote}");
            return Ok(CachedDist::from_remote(
                remote,
                filename,
                download.target().to_path_buf(),
            ));
        }

        // Unzip the wheel.
        let normalized_path = tokio::task::spawn_blocking({
            move || -> Result<PathBuf, puffin_extract::Error> {
                // Unzip the wheel into a temporary directory.
                let parent = download
                    .target()
                    .parent()
                    .expect("Cache paths can't be root");
                fs_err::create_dir_all(parent)?;
                let staging = tempfile::tempdir_in(parent)?;
                download.unzip(staging.path())?;

                // Move the unzipped wheel into the cache,.
                if let Err(err) = fs_err::rename(staging.into_path(), download.target()) {
                    // If another thread already unpacked the wheel, we can ignore the error.
                    return if download.target().is_dir() {
                        warn!("Wheel is already unpacked: {}", download.remote());
                        Ok(download.target().to_path_buf())
                    } else {
                        Err(err.into())
                    };
                }

                Ok(download.target().to_path_buf())
            }
        })
        .await?
        .map_err(|err| Error::Unzip(remote.clone(), err))?;

        Ok(CachedDist::from_remote(remote, filename, normalized_path))
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is unzipped. This implies that the wheel was downloaded and,
    /// if necessary, built.
    fn on_progress(&self, dist: &dyn Metadata);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, dist: &dyn Metadata) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, dist: &dyn Metadata, id: usize);

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
