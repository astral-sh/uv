use std::fmt::{self, Write};
use uv_command_support::Printer;
use uv_command_support::progress::{Direction, ProgressReporter};
use uv_console::human_readable_bytes;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;

use uv_cache::Removal;
use uv_distribution_filename::DistFilename;
use uv_pep440::Version;

#[derive(Debug)]
pub(crate) struct PublishReporter {
    reporter: ProgressReporter,
    dry_run: bool,
}

impl PublishReporter {
    /// Initialize a [`PublishReporter`] for a single upload.
    pub(crate) fn single(printer: Printer, dry_run: bool) -> Self {
        Self::new(printer, None, dry_run)
    }

    /// Initialize a [`PublishReporter`] for multiple uploads.
    fn new(printer: Printer, length: Option<u64>, dry_run: bool) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(length, printer.target()));
        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter, dry_run }
    }
}

impl uv_publish::Reporter for PublishReporter {
    fn on_validation_start(&self, name: &DistFilename, size: u64) -> Result<(), fmt::Error> {
        let bytes = human_readable_bytes(size);
        if self.dry_run {
            writeln!(
                self.reporter.printer.stderr(),
                "{} {name} {}",
                "Checking".bold().cyan(),
                format!("({bytes:.1})").dimmed()
            )
        } else {
            writeln!(
                self.reporter.printer.stderr(),
                "{} {name} {}",
                "Hashing".bold().green(),
                format!("({bytes:.1})").dimmed()
            )
        }
    }

    fn on_upload_ready(&self, name: &DistFilename, size: u64) -> Result<(), fmt::Error> {
        let bytes = human_readable_bytes(size);
        writeln!(
            self.reporter.printer.stderr(),
            "{} {name} {}",
            "Uploading".bold().green(),
            format!("({bytes:.1})").dimmed()
        )
    }

    fn on_progress(&self, _name: &str, id: usize) {
        self.reporter.on_download_complete(id);
    }

    fn on_upload_start(&self, name: &str, size: Option<u64>) -> usize {
        self.reporter.on_upload_start(name.to_string(), size)
    }

    fn on_upload_progress(&self, id: usize, inc: u64) {
        self.reporter.on_upload_progress(id, inc);
    }

    fn on_upload_complete(&self, id: usize) {
        self.reporter.on_upload_complete(id);
    }

    fn on_hash_start(&self, name: &DistFilename, size: Option<u64>) -> usize {
        self.reporter.on_hash_start(name.to_string(), size)
    }

    fn on_hash_progress(&self, id: usize, inc: u64) {
        self.reporter.on_hash_progress(id, inc);
    }

    fn on_hash_complete(&self, id: usize) {
        self.reporter.on_hash_complete(id);
    }
}

#[derive(Debug)]
pub(crate) struct CleaningDirectoryReporter {
    bar: ProgressBar,
}

impl CleaningDirectoryReporter {
    /// Initialize a [`CleaningDirectoryReporter`] for cleaning the cache directory.
    pub(crate) fn new(printer: Printer, max: Option<usize>) -> Self {
        let bar = ProgressBar::with_draw_target(max.map(|m| m as u64), printer.target());
        bar.set_style(
            ProgressStyle::with_template("{prefix} [{bar:20}] {percent}%")
                .unwrap()
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("{}", "Cleaning".bold().cyan()));
        Self { bar }
    }
}

impl uv_cache::CleanReporter for CleaningDirectoryReporter {
    fn on_clean(&self) {
        self.bar.inc(1);
    }

    fn on_complete(&self) {
        self.bar.finish_and_clear();
    }
}

#[derive(Debug)]
pub(crate) struct CleaningPackageReporter {
    bar: ProgressBar,
}

impl CleaningPackageReporter {
    /// Initialize a [`CleaningPackageReporter`] for cleaning packages from the cache.
    pub(crate) fn new(printer: Printer, max: Option<usize>) -> Self {
        let bar = ProgressBar::with_draw_target(max.map(|m| m as u64), printer.target());
        bar.set_style(
            ProgressStyle::with_template("{prefix} [{bar:20}] {pos}/{len}{msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        bar.set_prefix(format!("{}", "Cleaning".bold().cyan()));
        Self { bar }
    }

    pub(crate) fn on_clean(&self, package: &str, removal: &Removal) {
        self.bar.inc(1);
        self.bar.set_message(format!(
            ": {}, {} files {} folders removed",
            package, removal.num_files, removal.num_dirs,
        ));
    }

    pub(crate) fn on_complete(&self) {
        self.bar.finish_and_clear();
    }
}

pub(crate) struct BinaryDownloadReporter {
    reporter: ProgressReporter,
}

impl BinaryDownloadReporter {
    /// Initialize a [`BinaryDownloadReporter`] for a single binary download.
    pub(crate) fn single(printer: Printer) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_bin_install::Reporter for BinaryDownloadReporter {
    fn on_download_start(&self, name: &str, version: &Version, size: Option<u64>) -> usize {
        self.reporter
            .on_request_start(Direction::Download, format!("{name} v{version}"), size)
    }

    fn on_download_progress(&self, id: usize, inc: u64) {
        self.reporter.on_request_progress(id, inc);
    }

    fn on_download_complete(&self, id: usize) {
        self.reporter.on_request_complete(Direction::Download, id);
    }
}
