use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use uv_command_support::{Printer, progress::ProgressReporter};
use uv_distribution_types::BuildableSource;
use uv_distribution_types::VersionOrUrlRef;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_redacted::DisplaySafeUrl;

#[derive(Debug)]
pub struct ResolverReporter {
    reporter: ProgressReporter,
}

impl ResolverReporter {
    #[must_use]
    pub(super) fn with_length(self, length: u64) -> Self {
        self.reporter.root.set_length(length);
        self
    }
}

impl From<Printer> for ResolverReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        root.enable_steady_tick(Duration::from_millis(200));
        root.set_style(
            ProgressStyle::with_template("{spinner:.white} {wide_msg:.dim}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        root.set_message("Resolving dependencies...");

        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_resolver::ResolverReporter for ResolverReporter {
    fn on_progress(&self, name: &PackageName, version_or_url: &VersionOrUrlRef) {
        match version_or_url {
            VersionOrUrlRef::Version(version) => {
                self.reporter.root.set_message(format!("{name}=={version}"));
            }
            VersionOrUrlRef::Url(url) => {
                self.reporter.root.set_message(format!("{name} @ {url}"));
            }
        }
    }

    fn on_complete(&self) {
        self.reporter.root.set_message("");
        self.reporter.root.finish_and_clear();
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(&source.color_display())
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(&source.color_display(), id);
    }

    fn on_checkout_start(&self, url: &DisplaySafeUrl, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &DisplaySafeUrl, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.reporter.on_download_progress(id, bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, id: usize) {
        self.reporter.on_download_complete(id);
    }
}

impl uv_distribution::Reporter for ResolverReporter {
    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_build_start(&source.color_display())
    }

    fn on_build_complete(&self, source: &BuildableSource, id: usize) {
        self.reporter.on_build_complete(&source.color_display(), id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name.to_string(), size)
    }

    fn on_download_progress(&self, id: usize, bytes: u64) {
        self.reporter.on_download_progress(id, bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_checkout_start(&self, url: &DisplaySafeUrl, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &DisplaySafeUrl, rev: &str, id: usize) {
        self.reporter.on_checkout_complete(url, rev, id);
    }
}

#[derive(Debug)]
pub struct LatestVersionReporter {
    progress: ProgressBar,
}

impl From<Printer> for LatestVersionReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Fetching latest versions...");
        Self { progress }
    }
}

impl LatestVersionReporter {
    #[must_use]
    pub fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }

    pub fn on_fetch_progress(&self) {
        self.progress.inc(1);
    }

    pub fn on_fetch_version(&self, name: &PackageName, version: &Version) {
        self.progress.set_message(format!("{name} v{version}"));
        self.progress.inc(1);
    }

    pub fn on_fetch_complete(&self) {
        self.progress.set_message("");
        self.progress.finish_and_clear();
    }
}
