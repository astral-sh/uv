use std::cmp::Reverse;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::{debug, instrument, warn};

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

            debug!("Unpacking wheel: {remote}");

            // Unzip the wheel.
            let normalized_path = tokio::task::spawn_blocking({
                move || -> Result<PathBuf> {
                    let parent = download.path().parent().expect("Cache paths can't be root");
                    fs_err::create_dir_all(parent)?;
                    let staging = tempfile::tempdir_in(parent)?;

                    download.unzip(staging.path())?;

                    // Remove the file we just unzipped and replace it with the unzipped directory.
                    // If we abort before renaming the directory that's not a problem, we just lose
                    // the cache.
                    if !matches!(download, LocalWheel::InMemory(_)) {
                        fs_err::remove_file(download.path())?;
                    }

                    let normalized_path = parent.join(download.filename().to_string());
                    if fs_err::remove_dir_all(&normalized_path).is_ok() {
                        // If we're replacing an existing directory, warn. If a wheel already exists
                        // in the cache, we should avoid re-downloading it, so reaching this
                        // condition represents a bug in the install plan.
                        warn!(
                            "Removed existing directory at: {}",
                            normalized_path.display()
                        );
                    }
                    fs_err::rename(staging.into_path(), &normalized_path)?;

                    Ok(normalized_path)
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
