use std::fmt::{self, Write};

use indicatif::{MultiProgress, ProgressBar};
use owo_colors::OwoColorize;
use uv_command_support::Printer;
use uv_command_support::progress::ProgressReporter;
use uv_console::human_readable_bytes;
use uv_distribution_filename::DistFilename;

#[derive(Debug)]
pub(super) struct PublishReporter {
    reporter: ProgressReporter,
    dry_run: bool,
}

impl PublishReporter {
    /// Initialize a [`PublishReporter`] for a single upload.
    pub(super) fn single(printer: Printer, dry_run: bool) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(None, printer.target()));
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
