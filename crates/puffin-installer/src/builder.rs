//! Build source distributions from downloaded archives.
//!
//! TODO(charlie): Unify with `crates/puffin-resolver/src/distribution/source_distribution.rs`.

use std::cmp::Reverse;

use anyhow::Result;
use fs_err::tokio as fs;
use tracing::debug;

use puffin_distribution::{BaseDist, Dist, RemoteDist};
use puffin_traits::BuildContext;

use crate::downloader::{DiskWheel, SourceDistDownload, WheelDownload};

const BUILT_WHEELS_CACHE: &str = "built-wheels-v0";

pub struct Builder<'a, T: BuildContext + Send + Sync> {
    build_context: &'a T,
    reporter: Option<Box<dyn Reporter>>,
}

impl<'a, T: BuildContext + Send + Sync> Builder<'a, T> {
    /// Initialize a new source distribution downloader.
    pub fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            reporter: None,
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

    /// Build a set of source distributions.
    pub async fn build(&self, dists: Vec<SourceDistDownload>) -> Result<Vec<WheelDownload>> {
        // Sort the distributions by size.
        let mut dists = dists;
        dists.sort_unstable_by_key(|distribution| {
            Reverse(distribution.dist.size().unwrap_or(usize::MAX))
        });

        // Build the distributions serially.
        let mut builds = Vec::with_capacity(dists.len());
        for dist in dists {
            debug!("Building source distribution: {dist}");

            let result = build_sdist(dist, self.build_context).await?;

            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_progress(result.remote());
            }

            builds.push(result);
        }

        if let Some(reporter) = self.reporter.as_ref() {
            reporter.on_complete();
        }

        Ok(builds)
    }
}

/// Build a source distribution into a wheel.
async fn build_sdist<T: BuildContext + Send + Sync>(
    dist: SourceDistDownload,
    build_context: &T,
) -> Result<WheelDownload> {
    // Create a directory for the wheel.
    let wheel_dir = build_context
        .cache()
        .join(BUILT_WHEELS_CACHE)
        .join(dist.dist.package_id());
    fs::create_dir_all(&wheel_dir).await?;

    // Build the wheel.
    // TODO(charlie): If this is a Git dependency, we should do another checkout. If the same
    // repository is used by multiple dependencies, at multiple commits, the local checkout may now
    // point to the wrong commit.
    let disk_filename = build_context
        .build_source(
            &dist.sdist_file,
            dist.subdirectory.as_deref(),
            &wheel_dir,
            &dist.dist.to_string(),
        )
        .await?;
    let wheel_filename = wheel_dir.join(disk_filename);

    Ok(WheelDownload::Disk(DiskWheel {
        dist: dist.dist,
        path: wheel_filename,
    }))
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a source distribution is built.
    fn on_progress(&self, dist: &Dist);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);
}
