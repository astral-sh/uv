//! Build source distributions from downloaded archives.
//!
//! TODO(charlie): Unify with `crates/puffin-resolver/src/distribution/source_distribution.rs`.

use std::cmp::Reverse;

use anyhow::Result;
use fs_err::tokio as fs;
use tracing::debug;

use puffin_distribution::RemoteDistribution;
use puffin_traits::BuildContext;

use crate::downloader::{DiskWheel, SourceDistribution, Wheel};

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
    pub async fn build(&'a self, distributions: Vec<SourceDistribution>) -> Result<Vec<Wheel>> {
        // Sort the distributions by size.
        let mut distributions = distributions;
        distributions.sort_unstable_by_key(|distribution| match &distribution.remote {
            RemoteDistribution::Registry(_package, _version, file) => Reverse(file.size),
            RemoteDistribution::Url(_, _) => Reverse(usize::MIN),
        });

        // Build the distributions serially.
        let mut builds = Vec::with_capacity(distributions.len());
        for distribution in distributions {
            debug!("Building source distribution: {distribution}");

            let result = build_sdist(distribution, self.build_context).await?;

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
    distribution: SourceDistribution,
    build_context: &T,
) -> Result<Wheel> {
    // Create a directory for the wheel.
    let wheel_dir = build_context
        .cache()
        .join(BUILT_WHEELS_CACHE)
        .join(distribution.remote.id());
    fs::create_dir_all(&wheel_dir).await?;

    // Build the wheel.
    // TODO(charlie): If this is a Git dependency, we should do another checkout. If the same
    // repository is used by multiple dependencies, at multiple commits, the local checkout may now
    // point to the wrong commit.
    let disk_filename = build_context
        .build_source_distribution(
            &distribution.sdist_file,
            distribution.subdirectory.as_deref(),
            &wheel_dir,
        )
        .await?;
    let wheel_filename = wheel_dir.join(disk_filename);

    Ok(Wheel::Disk(DiskWheel {
        remote: distribution.remote,
        path: wheel_filename,
    }))
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a source distribution is built.
    fn on_progress(&self, wheel: &RemoteDistribution);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);
}
