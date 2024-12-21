use std::sync::Arc;

use url::Url;

use uv_distribution_types::BuildableSource;
use uv_pep508::PackageName;

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a source distribution build is kicked off.
    fn on_build_start(&self, source: &BuildableSource) -> usize;

    /// Callback to invoke when a source distribution build is complete.
    fn on_build_complete(&self, source: &BuildableSource, id: usize);

    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize);

    /// Callback to invoke when a download is kicked off.
    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize;

    /// Callback to invoke when a download makes progress (i.e. some number of bytes are
    /// downloaded).
    fn on_download_progress(&self, id: usize, inc: u64);

    /// Callback to invoke when a download is complete.
    fn on_download_complete(&self, name: &PackageName, id: usize);
}

impl dyn Reporter {
    /// Converts this reporter to a [`uv_git::Reporter`].
    pub(crate) fn into_git_reporter(self: Arc<dyn Reporter>) -> Arc<dyn uv_git::Reporter> {
        Arc::new(Facade {
            reporter: self.clone(),
        })
    }
}

/// A facade for converting from [`Reporter`] to [`uv_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl uv_git::Reporter for Facade {
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }
}
