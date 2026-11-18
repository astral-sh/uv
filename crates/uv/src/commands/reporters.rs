use std::fmt::{self, Write};

use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use uv_cache::{PhysicalBytes, Removal};
use uv_command_support::Printer;
use uv_console::human_readable_bytes;

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

/// The tense to report a cache removal in.
#[derive(Debug, Clone, Copy)]
pub(super) enum RemovalTense {
    /// The entries were removed.
    Past,
    /// The entries would be removed, as part of a dry run.
    Conditional,
}

impl RemovalTense {
    /// The verb to report a removal with.
    fn verb(self) -> &'static str {
        match self {
            Self::Past => "Removed",
            Self::Conditional => "Would remove",
        }
    }
}

/// Report the entries and estimated space reclaimed by cache cleaning or pruning.
pub(super) fn print_removal_summary(
    summary: &Removal,
    empty: &str,
    tense: RemovalTense,
    printer: Printer,
) -> fmt::Result {
    let mut stderr = printer.stderr();
    let verb = tense.verb();
    match (summary.num_files, summary.num_dirs) {
        (0, 0) => return writeln!(stderr, "{empty}"),
        (0, 1) => write!(stderr, "{verb} 1 directory")?,
        (0, dirs) => write!(stderr, "{verb} {dirs} directories")?,
        (1, _) => write!(stderr, "{verb} 1 file")?,
        (files, _) => write!(stderr, "{verb} {files} files")?,
    }

    let (prefix, bytes) = match summary.physical_bytes {
        PhysicalBytes::Unavailable => ("", summary.coarse_bytes),
        PhysicalBytes::Measured(bytes) => ("", bytes),
        PhysicalBytes::LowerBound(bytes) => ("at least ", bytes),
    };
    let bytes = human_readable_bytes(bytes);
    writeln!(stderr, " ({prefix}{:.1})", bytes.green())
}
