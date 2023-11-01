use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use puffin_distribution::{CachedDistribution, RemoteDistribution, VersionOrUrl};
use puffin_normalize::ExtraName;
use puffin_normalize::PackageName;

use crate::printer::Printer;

#[derive(Debug)]
pub(crate) struct WheelFinderReporter {
    progress: ProgressBar,
}

impl From<Printer> for WheelFinderReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        progress.set_message("Resolving dependencies...");
        Self { progress }
    }
}

impl WheelFinderReporter {
    #[must_use]
    pub(crate) fn with_length(self, length: u64) -> Self {
        self.progress.set_length(length);
        self
    }
}

impl puffin_resolver::WheelFinderReporter for WheelFinderReporter {
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
pub(crate) struct ResolverReporter {
    progress: ProgressBar,
}

impl From<Printer> for ResolverReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.enable_steady_tick(Duration::from_millis(200));
        progress.set_style(
            ProgressStyle::with_template("{spinner:.white} {wide_msg:.dim}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        progress.set_message("Resolving dependencies...");
        Self { progress }
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

    fn on_complete(&self) {
        self.progress.finish_and_clear();
    }
}
