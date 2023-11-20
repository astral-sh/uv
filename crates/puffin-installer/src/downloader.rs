use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};
use tokio::task::JoinSet;

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
        let mut fetches = JoinSet::new();
        let mut downloads = Vec::with_capacity(dists.len());
        for dist in dists {
            if self.no_build && matches!(dist, Dist::Source(_)) {
                bail!(
                    "Building source distributions is disabled, not downloading {}",
                    dist
                );
            }

            fetches.spawn(fetch(
                dist.clone(),
                self.client.clone(),
                self.cache.to_path_buf(),
                self.locks.clone(),
            ));
        }

        while let Some(result) = fetches.join_next().await.transpose()? {
            let result = result?;

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
}

/// Download a built distribution (wheel) or source distribution (sdist).
async fn fetch(
    dist: Dist,
    client: RegistryClient,
    cache: PathBuf,
    locks: Arc<Locks>,
) -> Result<Download> {
    let lock = locks.acquire(&dist).await;
    let _guard = lock.lock().await;
    let metadata = Fetcher::new(&cache).fetch_dist(&dist, &client).await?;
    Ok(metadata)
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is downloaded.
    fn on_download_progress(&self, download: &Download);

    /// Callback to invoke when the operation is complete.
    fn on_download_complete(&self);
}
