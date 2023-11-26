use std::cmp::Reverse;
use std::path::Path;

use anyhow::Result;
use tracing::debug;

use distribution_types::{CachedDist, Dist, Identifier, RemoteSource};
use puffin_distribution::{LocalWheel, Unzip};

use crate::cache::WheelCache;

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
    pub async fn unzip(
        &self,
        downloads: Vec<LocalWheel>,
        target: &Path,
    ) -> Result<Vec<CachedDist>> {
        // Create the wheel cache subdirectory, if necessary.
        let wheel_cache = WheelCache::new(target);
        wheel_cache.init()?;

        // Sort the wheels by size.
        let mut downloads = downloads;
        downloads
            .sort_unstable_by_key(|wheel| Reverse(wheel.remote().size().unwrap_or(usize::MIN)));

        let staging = tempfile::tempdir_in(wheel_cache.root())?;

        // Unpack the wheels into the cache.
        let mut wheels = Vec::with_capacity(downloads.len());
        for download in downloads {
            let remote = download.remote().clone();
            let filename = download.filename().clone();

            debug!("Unpacking wheel: {remote}");

            // Unzip the wheel.
            tokio::task::spawn_blocking({
                let target = staging.path().join(remote.distribution_id());
                move || download.unzip(&target)
            })
            .await??;

            // Write the unzipped wheel to the target directory.
            let target = wheel_cache.entry(&remote, &filename);
            if let Some(parent) = target.parent() {
                fs_err::create_dir_all(parent)?;
            }
            let result =
                fs_err::tokio::rename(staging.path().join(remote.distribution_id()), target).await;

            if let Err(err) = result {
                // If the renaming failed because another instance was faster, that's fine
                // (`DirectoryNotEmpty` is not stable so we can't match on it)
                if !wheel_cache.entry(&remote, &filename).is_dir() {
                    return Err(err.into());
                }
            }

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_unzip_progress(&remote);
            }

            let path = wheel_cache.entry(&remote, &filename);
            wheels.push(CachedDist::from_remote(remote, filename, path));
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
