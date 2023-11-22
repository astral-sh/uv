use std::sync::{Arc, Mutex};
use std::time::Duration;

use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use url::Url;

use distribution_types::{CachedDist, Dist, Metadata, SourceDist, VersionOrUrl};
use puffin_distribution::Download;
use puffin_normalize::ExtraName;
use puffin_normalize::PackageName;

use crate::printer::Printer;

#[derive(Debug)]
pub(crate) struct FinderReporter {
    progress: ProgressBar,
}

impl From<Printer> for FinderReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Resolving dependencies...");
        Self { progress }
    }
}

impl FinderReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_resolver::FinderReporter for FinderReporter {
    fn on_progress(&self, wheel: &Dist) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct UnzipReporter {
    progress: ProgressBar,
}

impl From<Printer> for UnzipReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Unzipping wheels...");
        Self { progress }
    }
}

impl UnzipReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_installer::UnzipReporter for UnzipReporter {
    fn on_unzip_progress(&self, wheel: &Dist) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_unzip_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct FetcherReporter {
    printer: Printer,
    multi_progress: MultiProgress,
    progress: ProgressBar,
    bars: Arc<Mutex<Vec<ProgressBar>>>,
}

impl From<Printer> for FetcherReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());

        let progress = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        progress.enable_steady_tick(Duration::from_millis(200));
        progress.set_style(
            ProgressStyle::with_template("{spinner:.white} {wide_msg:.dim}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        progress.set_message("Resolving dependencies...");

        Self {
            printer,
            multi_progress,
            progress,
            bars: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl FetcherReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_distribution::Reporter for FetcherReporter {
    fn on_download_progress(&self, download: &Download) {
        self.progress.set_message(format!("{download}"));
        self.progress.inc(1);
    }

    fn on_build_start(&self, dist: &SourceDist) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!(
            "{} {}",
            "Building".bold().green(),
            dist.to_color_string()
        ));

        let mut bars = self.bars.lock().unwrap();
        bars.push(progress);
        bars.len() - 1
    }

    fn on_build_complete(&self, dist: &SourceDist, index: usize) {
        let bars = self.bars.lock().unwrap();
        let progress = &bars[index];
        progress.finish_with_message(format!(
            "{} {}",
            "Built".bold().green(),
            dist.to_color_string()
        ));
    }

    fn on_download_and_build_complete(&self) {
        self.progress.finish_and_clear();
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!(
            "{} {} ({})",
            "Updating".bold().green(),
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
            "{} {} ({})",
            "Updated".bold().green(),
            url,
            rev.dimmed()
        ));
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

impl puffin_installer::InstallReporter for InstallReporter {
    fn on_install_progress(&self, wheel: &CachedDist) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_install_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct ResolverReporter {
    printer: Printer,
    multi_progress: MultiProgress,
    progress: ProgressBar,
    bars: Arc<Mutex<Vec<ProgressBar>>>,
}

impl From<Printer> for ResolverReporter {
    fn from(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());

        let progress = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        progress.enable_steady_tick(Duration::from_millis(200));
        progress.set_style(
            ProgressStyle::with_template("{spinner:.white} {wide_msg:.dim}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        progress.set_message("Resolving dependencies...");

        Self {
            printer,
            multi_progress,
            progress,
            bars: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl puffin_resolver::ResolverReporter for ResolverReporter {
    fn on_progress(
        &self,
        name: &PackageName,
        extra: Option<&ExtraName>,
        version_or_url: VersionOrUrl,
    ) {
        match (extra, version_or_url) {
            (None, VersionOrUrl::Version(version)) => {
                self.progress.set_message(format!("{name}=={version}"));
            }
            (None, VersionOrUrl::Url(url)) => {
                self.progress.set_message(format!("{name} @ {url}"));
            }
            (Some(extra), VersionOrUrl::Version(version)) => {
                self.progress
                    .set_message(format!("{name}[{extra}]=={version}"));
            }
            (Some(extra), VersionOrUrl::Url(url)) => {
                self.progress
                    .set_message(format!("{name}[{extra}] @ {url}"));
            }
        }
    }

    fn on_download_progress(&self, download: &Download) {
        self.progress.set_message(format!("{download}"));
        self.progress.inc(1);
    }

    fn on_complete(&self) {
        self.progress.finish_and_clear();
    }

    fn on_build_start(&self, dist: &SourceDist) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!(
            "{} {}",
            "Building".bold().green(),
            dist.to_color_string()
        ));

        let mut bars = self.bars.lock().unwrap();
        bars.push(progress);
        bars.len() - 1
    }

    fn on_build_complete(&self, dist: &SourceDist, index: usize) {
        let bars = self.bars.lock().unwrap();
        let progress = &bars[index];
        progress.finish_with_message(format!(
            "{} {}",
            "Built".bold().green(),
            dist.to_color_string()
        ));
    }

    fn on_download_and_build_complete(&self) {
        self.progress.finish_and_clear();
    }

    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        let progress = self.multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!(
            "{} {} ({})",
            "Updating".bold().green(),
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
            "{} {} ({})",
            "Updated".bold().green(),
            url,
            rev.dimmed()
        ));
    }
}

/// Like [`std::fmt::Display`], but with colors.
trait ColorDisplay {
    fn to_color_string(&self) -> String;
}

impl ColorDisplay for &Dist {
    fn to_color_string(&self) -> String {
        let name = self.name();
        let version_or_url = self.version_or_url();
        format!("{}{}", name, version_or_url.to_string().dimmed())
    }
}

impl ColorDisplay for SourceDist {
    fn to_color_string(&self) -> String {
        let name = self.name();
        let version_or_url = self.version_or_url();
        format!("{}{}", name, version_or_url.to_string().dimmed())
    }
}
