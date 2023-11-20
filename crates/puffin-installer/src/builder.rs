//! Build source distributions from downloaded archives.

use std::cmp::Reverse;

use anyhow::Result;
use tracing::debug;

use distribution_types::{Dist, RemoteSource};
use puffin_distribution::{Fetcher, SourceDistDownload, WheelDownload};
use puffin_traits::BuildContext;

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
            Reverse(distribution.remote().size().unwrap_or(usize::MAX))
        });

        // Build the distributions serially.
        let mut builds = Vec::with_capacity(dists.len());
        for dist in dists {
            debug!("Building source distribution: {dist}");

            let result = Fetcher::new(self.build_context.cache())
                .build_sdist(dist, self.build_context)
                .await?;

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

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a source distribution is built.
    fn on_progress(&self, dist: &Dist);

    /// Callback to invoke when the operation is complete.
    fn on_complete(&self);
}
