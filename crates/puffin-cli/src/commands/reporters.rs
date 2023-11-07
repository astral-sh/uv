use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use colored::Colorize;
use std::time::Duration;

use indicatif::{MultiProgress, MultiProgressAlignment, ProgressBar, ProgressStyle};
use url::Url;

use puffin_distribution::{
    CachedDistribution, RemoteDistribution, RemoteDistributionRef, VersionOrUrl,
};
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
    fn on_progress(&self, wheel: &RemoteDistribution) {
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
    fn on_unzip_progress(&self, wheel: &RemoteDistribution) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_unzip_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct DownloadReporter {
    progress: ProgressBar,
}

impl From<Printer> for DownloadReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Downloading wheels...");
        Self { progress }
    }
}

impl DownloadReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_installer::DownloadReporter for DownloadReporter {
    fn on_download_progress(&self, wheel: &RemoteDistribution) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_download_complete(&self) {
        self.progress.finish_and_clear();
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
    fn on_install_progress(&self, wheel: &CachedDistribution) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_install_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct BuildReporter {
    progress: ProgressBar,
}

impl From<Printer> for BuildReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Building wheels...");
        Self { progress }
    }
}

impl BuildReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_installer::BuildReporter for BuildReporter {
    fn on_progress(&self, wheel: &RemoteDistribution) {
        self.progress.set_message(format!("{wheel}"));
        self.progress.inc(1);
    }

    fn on_complete(&self) {
        self.progress.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct ResolverReporter {
    printer: Printer,
    multi_progress: Arc<Mutex<MultiProgress>>,
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
            multi_progress: Arc::new(Mutex::new(multi_progress)),
            progress,
            bars: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// A task ID to capture an in-flight progress bar.
#[derive(Debug)]
pub(crate) struct TaskId(ProgressBar);

impl puffin_resolver::ResolverReporter<TaskId> for ResolverReporter {
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

    fn on_complete(&self) {
        self.progress.finish_and_clear();
    }

    fn on_build_start(&self, distribution: &RemoteDistributionRef<'_>) -> TaskId {
        let multi_progress = self.multi_progress.lock().unwrap();
        let progress = multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!("{} {}", "Building".bold().green(), distribution));

        TaskId(progress)
    }

    fn on_build_complete(&self, distribution: &RemoteDistributionRef<'_>, task: TaskId) {
        let progress = task.0;
        progress.finish_with_message(format!("{} {}", "Built".bold().green(), distribution));
    }

    fn on_fetch_git_repo(&self, url: &Url) {
        let multi_progress = self.multi_progress.lock().unwrap();
        let progress = multi_progress.insert_before(
            &self.progress,
            ProgressBar::with_draw_target(None, self.printer.target()),
        );

        progress.set_style(ProgressStyle::with_template("{wide_msg}").unwrap());
        progress.set_message(format!("{} {}", "Updating".bold().green(), url));
        progress.finish();
    }
}
