use std::sync::Arc;

use url::Url;

use distribution_types::SourceDist;

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, dist: &SourceDist) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, dist: &SourceDist, id: usize);

    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to [`uv_git::Reporter`].
pub(crate) struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl From<Arc<dyn Reporter>> for Facade {
    fn from(reporter: Arc<dyn Reporter>) -> Self {
        Self { reporter }
    }
}

impl uv_git::Reporter for Facade {
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}
