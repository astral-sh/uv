use std::cmp::Reverse;
use std::path::PathBuf;

use anyhow::{format_err, Context, Result};
use tracing::{debug, instrument, warn};

use distribution_types::{CachedDist, Dist, RemoteSource};
use puffin_distribution::{LocalWheel, Unzip};
use puffin_traits::OnceMap;

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
    pub async fn unzip(
        &self,
        downloads: Vec<LocalWheel>,
        in_flight: &OnceMap<PathBuf, Result<CachedDist, String>>,
    ) -> Result<Vec<CachedDist>> {
        // Sort the wheels by size.
        let mut downloads = downloads;
        downloads
            .sort_unstable_by_key(|wheel| Reverse(wheel.remote().size().unwrap_or(usize::MIN)));

        // Unpack the wheels into the cache.
        let mut unzipped = Vec::with_capacity(downloads.len());
        for wheel in downloads {
            let remote = wheel.remote().clone();
            let wheel_path = wheel.target().to_path_buf();

            let cached_dist =
                if let Some(cached_dist) = in_flight.wait_or_register(&wheel_path).await {
                    cached_dist
                        .value()
                        .clone()
                        .map_err(|err| format_err!("Failed to unpack: {err}"))?
                } else {
                    let result = unzip_wheel(wheel)
                        .await
                        .with_context(|| format!("Failed to unpack: {remote}"));
                    match result {
                        Ok(cached) => {
                            in_flight.done(wheel_path, Ok(cached.clone()));
                            cached
                        }
                        Err(err) => {
                            in_flight.done(wheel_path, Err(err.to_string()));
                            return Err(err);
                        }
                    }
                };

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_unzip_progress(&remote);
            }

            unzipped.push(cached_dist);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_unzip_complete();
        }

        Ok(unzipped)
    }
}

/// Unzip a locally-available wheel into the cache.
async fn unzip_wheel(wheel: LocalWheel) -> Result<CachedDist> {
    debug!("Unpacking wheel: {}", wheel.remote());

    let remote = wheel.remote().clone();
    let filename = wheel.filename().clone();
    let target = wheel.target().to_path_buf();

    tokio::task::spawn_blocking(move || -> Result<()> {
        // Unzip the wheel into a temporary directory.
        let parent = wheel.target().parent().expect("Cache paths can't be root");
        fs_err::create_dir_all(parent)?;
        let staging = tempfile::tempdir_in(parent)?;
        wheel.unzip(staging.path())?;

        // Move the unzipped wheel into the cache,.
        if let Err(err) = fs_err::rename(staging.into_path(), wheel.target()) {
            // If another thread already unpacked the wheel, we can ignore the error.
            if wheel.target().is_dir() {
                warn!("Wheel is already unpacked: {}", wheel.remote());
            } else {
                return Err(err.into());
            }
        }

        Ok(())
    })
    .await??;

    Ok(CachedDist::from_remote(remote, filename, target))
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a wheel is unzipped.
    fn on_unzip_progress(&self, dist: &Dist);

    /// Callback to invoke when the operation is complete.
    fn on_unzip_complete(&self);
}
