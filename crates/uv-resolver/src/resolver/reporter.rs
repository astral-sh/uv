use std::sync::Arc;

use url::Url;

use distribution_types::{SourceDist, VersionOrUrl};
use uv_normalize::PackageName;

pub type BuildId = usize;

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a dependency is resolved.
    fn on_progress(&self, name: &PackageName, version: VersionOrUrl);

    /// Callback to invoke when the resolution is complete.
    fn on_complete(&self);

    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, dist: &SourceDist) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, dist: &SourceDist, id: usize);

    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to [`uv_distribution::Reporter`].
pub(crate) struct Facade {
    pub(crate) reporter: Arc<dyn Reporter>,
}

impl uv_distribution::Reporter for Facade {
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
