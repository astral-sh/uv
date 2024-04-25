use std::sync::{Arc, Mutex};
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use url::Url;

use distribution_types::{
    BuildableSource, CachedDist, DistributionMetadata, LocalEditable, Name, SourceDist,
    VersionOrUrlRef,
};
use uv_normalize::PackageName;

use crate::printer::Printer;

#[derive(Debug)]
struct ProgressReporter {
    printer: Printer,
    root: ProgressBar,
    multi_progress: MultiProgress,
    bars: Arc<Mutex<Vec<ProgressBar>>>,
}

impl ProgressReporter {
    fn on_any_build_start(&self, color_string: &str) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.root,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!("{} {}", "Building".bold().cyan(), color_string));

        let mut bars = self.bars.lock().unwrap();
        bars.push(progress);
        bars.len() - 1
    }

    fn on_any_build_complete(&self, color_string: &str, id: usize) {
        let bars = self.bars.lock().unwrap();
        let progress = &bars[id];
        progress.finish_with_message(format!("   {} {}", "Built".bold().green(), color_string));
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        let progress = self.multi_progress.insert_after(
            &self.root,
            ProgressBar::with_draw_target(size, self.printer.target()),
        );

        if size.is_some() {
            progress.set_style(
                ProgressStyle::with_template(
                    "{wide_msg:.dim} {decimal_bytes}/{decimal_total_bytes} [{bar:30}]",
                )
                .unwrap()
                .progress_chars("##-"),
            );
            progress.set_message(name.to_string());
        } else {
            progress.set_style(ProgressStyle::with_template("{wide_msg:.dim} n/a [....]").unwrap());
            progress.set_message(name.to_string());
            progress.finish();
        }

        let mut bars = self.bars.lock().unwrap();
        bars.push(progress);
        bars.len() - 1
    }

    fn on_download_progress(&self, index: usize, bytes: u64) {
        self.bars.lock().unwrap()[index].inc(bytes);
    }

    fn on_download_complete(&self, _name: &PackageName, index: usize) {
        self.bars.lock().unwrap()[index].finish_and_clear();
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.root,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!(
            "{} {} ({})",
            "Updating".bold().cyan(),
            url,
            rev.dimmed()
        ));
        progress.finish();

        let mut bars = self.bars.lock().unwrap();
        bars.push(progress);
        bars.len() - 1
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        let bars = self.bars.lock().unwrap();
        let progress = &bars[index];
        progress.finish_with_message(format!(
            " {} {} ({})",
            "Updated".bold().green(),
            url,
            rev.dimmed()
        ));
    }
}

#[derive(Debug)]
pub(crate) struct DownloadReporter {
    reporter: ProgressReporter,
}

impl From<Printer> for DownloadReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());

        let progress = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        progress.set_style(
            ProgressStyle::with_template(":: Downloading dependencies... ({pos}/{len})")
                .unwrap()
                .progress_chars("##-"),
        );
        progress.set_message("Fetching packages...");

        let reporter = ProgressReporter {
            printer,
            multi_progress,
            root: progress,
            bars: Arc::new(Mutex::new(Vec::new())),
        };

        Self { reporter }
    }
}

impl DownloadReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.reporter.root.set_length(length);
        self
    }
}

impl uv_installer::DownloadReporter for DownloadReporter {
    fn on_progress(&self, dist: &CachedDist) {
        self.reporter.root.set_message(format!("{dist}"));
        self.reporter.root.inc(1);
    }

    fn on_complete(&self) {
        self.reporter.root.finish_and_clear();
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_any_build_start(&source.to_color_string())
    }

    fn on_build_complete(&self, source: &BuildableSource, index: usize) {
        self.reporter
            .on_any_build_complete(&source.to_color_string(), index);
    }

    fn on_editable_build_start(&self, dist: &LocalEditable) -> usize {
        self.reporter.on_any_build_start(&dist.to_color_string())
    }

    fn on_editable_build_complete(&self, dist: &LocalEditable, id: usize) {
        self.reporter
            .on_any_build_complete(&dist.to_color_string(), id);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name, size)
    }

    fn on_download_progress(&self, index: usize, bytes: u64) {
        self.reporter.on_download_progress(index, bytes);
    }

    fn on_download_complete(&self, name: &PackageName, index: usize) {
        self.reporter.on_download_complete(name, index);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}

#[derive(Debug)]
pub(crate) struct ResolverReporter {
    reporter: ProgressReporter,
}

impl ResolverReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
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

        let reporter = ProgressReporter {
            root,
            printer,
            multi_progress,
            bars: Arc::new(Mutex::new(Vec::new())),
        };

        ResolverReporter { reporter }
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
        self.reporter.root.finish_and_clear();
    }

    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_any_build_start(&source.to_color_string())
    }

    fn on_build_complete(&self, source: &BuildableSource, index: usize) {
        self.reporter
            .on_any_build_complete(&source.to_color_string(), index);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name, size)
    }

    fn on_download_progress(&self, index: usize, bytes: u64) {
        self.reporter.on_download_progress(index, bytes);
    }

    fn on_download_complete(&self, name: &PackageName, index: usize) {
        self.reporter.on_download_complete(name, index);
    }
}

impl uv_distribution::Reporter for ResolverReporter {
    fn on_build_start(&self, source: &BuildableSource) -> usize {
        self.reporter.on_any_build_start(&source.to_color_string())
    }

    fn on_build_complete(&self, source: &BuildableSource, index: usize) {
        self.reporter
            .on_any_build_complete(&source.to_color_string(), index);
    }

    fn on_download_start(&self, name: &PackageName, size: Option<u64>) -> usize {
        self.reporter.on_download_start(name, size)
    }

    fn on_download_progress(&self, index: usize, bytes: u64) {
        self.reporter.on_download_progress(index, bytes);
    }

    fn on_download_complete(&self, name: &PackageName, bytes: usize) {
        self.reporter.on_download_complete(name, bytes);
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}

#[derive(Debug)]
pub(crate) struct InstallReporter {
    progress: ProgressBar,
}

impl From<Printer> for InstallReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Installing wheels...");
        Self { progress }
    }
}

impl InstallReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl uv_installer::InstallReporter for InstallReporter {
    fn on_install_progress(&self, wheel: &CachedDist) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_install_complete(&self) {
        self.progress.finish_and_clear();
    }
}

/// Like [`std::fmt::Display`], but with colors.
trait ColorDisplay {
    fn to_color_string(&self) -> String;
}

impl ColorDisplay for SourceDist {
    fn to_color_string(&self) -> String {
        let name = self.name();
        let version_or_url = self.version_or_url();
        format!("{}{}", name, version_or_url.to_string().dimmed())
    }
}

impl ColorDisplay for BuildableSource<'_> {
    fn to_color_string(&self) -> String {
        match self {
            BuildableSource::Dist(dist) => dist.to_color_string(),
            BuildableSource::Url(url) => url.to_string(),
        }
    }
}

impl ColorDisplay for LocalEditable {
    fn to_color_string(&self) -> String {
        format!("{}", self.to_string().dimmed())
    }
}
