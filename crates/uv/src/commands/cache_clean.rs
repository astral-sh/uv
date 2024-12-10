use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use uv_cache::{Cache, Removal};
use uv_fs::Simplified;
use uv_normalize::PackageName;

use crate::commands::reporters::{CleaningDirectoryReporter, CleaningPackageReporter};
use crate::commands::{human_readable_bytes, ExitStatus};
use crate::printer::Printer;

/// Clear the cache, removing all entries or those linked to specific packages.
pub(crate) fn cache_clean(
    packages: &[PackageName],
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if !cache.root().exists() {
        writeln!(
            printer.stderr(),
            "No cache found at: {}",
            cache.root().user_display().cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    let summary = if packages.is_empty() {
        writeln!(
            printer.stderr(),
            "Clearing cache at: {}",
            cache.root().user_display().cyan()
        )?;

        let num_paths = walkdir::WalkDir::new(cache.root()).into_iter().count();
        let reporter = CleaningDirectoryReporter::new(printer, num_paths);

        cache
            .clear(Box::new(reporter))
            .with_context(|| format!("Failed to clear cache at: {}", cache.root().user_display()))?
    } else {
        let reporter = CleaningPackageReporter::new(printer, packages.len());
        let mut summary = Removal::default();

        for package in packages {
            let removed = cache.remove(package)?;
            summary += removed;
            reporter.on_clean(package.as_str(), &summary);
        }
        reporter.on_complete();

        summary
    };

    // Write a summary of the number of files and directories removed.
    match (summary.num_files, summary.num_dirs) {
        (0, 0) => {
            write!(printer.stderr(), "No cache entries found")?;
        }
        (0, 1) => {
            write!(printer.stderr(), "Removed 1 directory")?;
        }
        (0, num_dirs_removed) => {
            write!(printer.stderr(), "Removed {num_dirs_removed} directories")?;
        }
        (1, _) => {
            write!(printer.stderr(), "Removed 1 file")?;
        }
        (num_files_removed, _) => {
            write!(printer.stderr(), "Removed {num_files_removed} files")?;
        }
    }

    // If any, write a summary of the total byte count removed.
    if summary.total_bytes > 0 {
        let bytes = if summary.total_bytes < 1024 {
            format!("{}B", summary.total_bytes)
        } else {
            let (bytes, unit) = human_readable_bytes(summary.total_bytes);
            format!("{bytes:.1}{unit}")
        };
        write!(printer.stderr(), " ({})", bytes.green())?;
    }

    writeln!(printer.stderr())?;

    Ok(ExitStatus::Success)
}
