use std::cmp::Reverse;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Result};
use futures::StreamExt;

use distribution_types::{Dist, RemoteSource};
use puffin_client::RegistryClient;
use puffin_distribution::{Download, Fetcher};

use crate::locks::Locks;

pub struct Downloader<'a> {
    client: &'a RegistryClient,
    cache: &'a Path,
    locks: Arc<Locks>,
    reporter: Option<Box<dyn Reporter>>,
    no_build: bool,
}

impl<'a> Downloader<'a> {
    /// Initialize a new distribution downloader.
    pub fn new(client: &'a RegistryClient, cache: &'a Path) -> Self {
        Self {
            client,
            cache,
            locks: Arc::new(Locks::default()),
            reporter: None,
            no_build: false,
        }
    }

    /// Set the [`Reporter`] to use for this downloader.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Optionally, block downloading source distributions.
    #[must_use]
    pub fn with_no_build(self, no_build: bool) -> Self {
        Self { no_build, ..self }
    }

    /// Download a set of distributions.
    pub async fn download(&self, dists: Vec<Dist>) -> Result<Vec<Download>> {
        // Sort the distributions by size.
        let mut dists = dists;
        dists.sort_unstable_by_key(|distribution| {
            Reverse(distribution.size().unwrap_or(usize::MAX))
        });

        // Fetch the distributions in parallel.
        let mut downloads = Vec::with_capacity(dists.len());
        let mut fetches = futures::stream::iter(dists)
            .map(|dist| self.fetch(dist))
            .buffer_unordered(50);

        while let Some(result) = fetches.next().await.transpose()? {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_download_progress(&result);
            }

            downloads.push(result);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_download_complete();
        }

        Ok(downloads)
    }

    /// Download a built distribution (wheel) or source distribution (sdist).
    async fn fetch(&self, dist: Dist) -> Result<Download> {
        match dist {
            Dist::Source(_) => {
                if self.no_build {
                    bail!("Building source distributions is disabled; skipping: {dist}");
                }

                let lock = self.locks.acquire(&dist).await;
                let _guard = lock.lock().await;

                let metadata = Fetcher::new(self.cache)
                    .fetch_dist(&dist, self.client)
                    .await?;
                Ok(metadata)
            }
            Dist::Built(_) => {
                let metadata = Fetcher::new(self.cache)
                    .fetch_dist(&dist, self.client)
                    .await?;
                Ok(metadata)
            }
        }
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, download: &Download);

    /// Callback to invoke when the operation is complete.
    fn on_download_complete(&self);
}
