use std::cmp::Reverse;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::{instrument, warn};

use distribution_types::{CachedDist, Dist, RemoteSource};
use puffin_distribution::{LocalWheel, Unzip};

#[derive(Default)]
pub struct Unzipper {
    reporter: Option<Box<dyn Reporter>>,
}

impl Unzipper {
    /// Set the [`Reporter`] to use for this unzipper.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
        }
    }

    /// Unzip a set of downloaded wheels.
    #[instrument(skip_all)]
    pub async fn unzip(&self, downloads: Vec<LocalWheel>) -> Result<Vec<CachedDist>> {
        // Sort the wheels by size.
        let mut downloads = downloads;
        downloads
            .sort_unstable_by_key(|wheel| Reverse(wheel.remote().size().unwrap_or(usize::MIN)));

        // Unpack the wheels into the cache.
        let mut wheels = Vec::with_capacity(downloads.len());
        for download in downloads {
            let remote = download.remote().clone();
            let filename = download.filename().clone();

            // If the wheel is already unpacked, we should avoid attempting to unzip it at all.
            if download.target().is_dir() {
                warn!("Wheel is already unpacked: {remote}");
                wheels.push(CachedDist::from_remote(
                    remote,
                    filename,
                    download.target().to_path_buf(),
                ));
                continue;
            }

            // Unzip the wheel.
            let normalized_path = tokio::task::spawn_blocking({
                move || -> Result<PathBuf> {
                    // Unzip the wheel into a temporary directory.
                    let parent = download
                        .target()
                        .parent()
                        .expect("Cache paths can't be root");
                    fs_err::create_dir_all(parent)?;
                    let staging = tempfile::tempdir_in(parent)?;
                    download.unzip(staging.path())?;

                    // Move the unzipped wheel into the cache,.
                    fs_err::rename(staging.into_path(), download.target())?;

                    Ok(download.target().to_path_buf())
                }
            })
            .await?
            .with_context(|| format!("Failed to unpack: {remote}"))?;

            wheels.push(CachedDist::from_remote(remote, filename, normalized_path));
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_unzip_complete();
        }

        Ok(wheels)
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is unzipped.
    fn on_unzip_progress(&self, dist: &Dist);

    /// Callback to invoke when the operation is complete.
    fn on_unzip_complete(&self);
}
