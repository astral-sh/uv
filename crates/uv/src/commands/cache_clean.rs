use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, Removal};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_python::managed::ManagedPythonInstallations;

use crate::commands::reporters::{CleaningDirectoryReporter, CleaningPackageReporter};
use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Clear the cache, removing all entries or those linked to specific packages.
pub(crate) async fn cache_clean(
    packages: &[PackageName],
    force: bool,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let mut python_removal = Removal::default();
    let mut python_scratch = None;

    if packages.is_empty() {
        let installations = ManagedPythonInstallations::from_settings(None)?;
        let scratch = installations.scratch();

        if scratch.is_dir() {
            let lock = installations.lock().await?;
            python_removal = installations.clear_scratch(&lock).with_context(|| {
                format!(
                    "Failed to clear temporary Python downloads at: {}",
                    scratch.user_display()
                )
            })?;

            if python_removal.num_files > 0 || python_removal.num_dirs > 0 {
                python_scratch = Some(scratch);
            }
        }
    }

    if !cache.root().exists() {
        writeln!(
            printer.stderr(),
            "No cache found at: {}",
            cache.root().user_display().cyan()
        )?;

        if let Some(scratch) = python_scratch {
            writeln!(
                printer.stderr(),
                "Clearing temporary Python downloads at: {}",
                scratch.user_display().cyan()
            )?;
            write_removal_summary(&python_removal, printer)?;
        }

        return Ok(ExitStatus::Success);
    }

    let cache = match cache.with_exclusive_lock_no_wait() {
        Ok(cache) => cache,
        Err(cache) if force => {
            debug!("Cache is currently in use, proceeding due to `--force`");
            cache
        }
        Err(cache) => {
            writeln!(
                printer.stderr(),
                "Cache is currently in-use, waiting for other uv processes to finish (use `--force` to override)"
            )?;
            cache.with_exclusive_lock().await?
        }
    };

    let mut summary = if packages.is_empty() {
        writeln!(
            printer.stderr(),
            "Clearing cache at: {}",
            cache.root().user_display().cyan()
        )?;

        if let Some(scratch) = python_scratch {
            writeln!(
                printer.stderr(),
                "Clearing temporary Python downloads at: {}",
                scratch.user_display().cyan()
            )?;
        }

        let num_paths = walkdir::WalkDir::new(cache.root()).into_iter().count();
        let reporter = CleaningDirectoryReporter::new(printer, Some(num_paths));

        let root = cache.root().to_path_buf();
        cache
            .clear(Box::new(reporter))
            .with_context(|| format!("Failed to clear cache at: {}", root.user_display()))?
    } else {
        let reporter = CleaningPackageReporter::new(printer, Some(packages.len()));
        let mut summary = Removal::default();

        for package in packages {
            let removed = cache.remove(package)?;
            summary += removed;
            reporter.on_clean(package.as_str(), &summary);
        }
        reporter.on_complete();

        summary
    };

    summary += python_removal;

    write_removal_summary(&summary, printer)?;

    Ok(ExitStatus::Success)
}

/// Write a summary of the files, directories, and bytes removed.
fn write_removal_summary(summary: &Removal, printer: Printer) -> Result<()> {
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

    Ok(())
}
