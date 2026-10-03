//! Progress reporting for managed Python downloads.

use indicatif::{MultiProgress, ProgressBar};
use uv_command_support::Printer;
use uv_command_support::progress::{Direction, ProgressReporter};
use uv_python::PythonInstallationKey;

#[derive(Debug)]
pub struct PythonDownloadReporter {
    reporter: ProgressReporter,
}

impl PythonDownloadReporter {
    /// Initialize a [`PythonDownloadReporter`] for a single Python download.
    pub fn single(printer: Printer) -> Self {
        Self::new(printer, None)
    }

    /// Initialize a [`PythonDownloadReporter`] for multiple Python downloads.
    pub fn new(printer: Printer, length: Option<u64>) -> Self {
        let multi_progress = MultiProgress::with_draw_target(printer.target());
        let root = multi_progress.add(ProgressBar::with_draw_target(length, printer.target()));
        let reporter = ProgressReporter::new(root, multi_progress, printer);
        Self { reporter }
    }
}

impl uv_python::downloads::Reporter for PythonDownloadReporter {
    fn on_request_start(
        &self,
        direction: uv_python::downloads::Direction,
        name: &PythonInstallationKey,
        size: Option<u64>,
    ) -> usize {
        self.reporter.on_request_start(
            progress_direction(direction),
            format!("{name} ({direction})"),
            size,
        )
    }

    fn on_request_progress(&self, id: usize, inc: u64) {
        self.reporter.on_request_progress(id, inc);
    }

    fn on_request_complete(&self, direction: uv_python::downloads::Direction, id: usize) {
        self.reporter
            .on_request_complete(progress_direction(direction), id);
    }
}

/// Map Python's download phases to the shared progress renderer.
fn progress_direction(direction: uv_python::downloads::Direction) -> Direction {
    match direction {
        uv_python::downloads::Direction::Download => Direction::Download,
        uv_python::downloads::Direction::Extract => Direction::Extract,
    }
}
