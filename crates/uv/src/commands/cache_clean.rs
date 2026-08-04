use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalMode};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::reporters::{CleaningDirectoryReporter, CleaningPackageReporter};
use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Clear the cache, removing all entries or those linked to specific packages.
pub(crate) async fn cache_clean(
    packages: &[PackageName],
    force: bool,
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !cache.root().exists() {
        writeln!(
            printer.stderr(),
            "No cache found at: {}",
            cache.root().user_display().cyan()
        )?;
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

    let removal_mode = if preview.is_enabled(PreviewFeature::CachePhysicalSpace) {
        RemovalMode::Physical
    } else {
        RemovalMode::Logical
    };
    let cache = cache.with_removal_mode(removal_mode);

    let summary = if packages.is_empty() {
        writeln!(
            printer.stderr(),
            "Clearing cache at: {}",
            cache.root().user_display().cyan()
        )?;

        let num_paths = walkdir::WalkDir::new(cache.root()).into_iter().count();
        let reporter = CleaningDirectoryReporter::new(printer, Some(num_paths));

        let root = cache.root().to_path_buf();
        cache
            .clear(Box::new(reporter))
            .with_context(|| format!("Failed to clear cache at: {}", root.user_display()))?
    } else {
        let reporter = CleaningPackageReporter::new(printer, Some(packages.len()));
        let mut summary = cache.removal();

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

    // If any, report the physical space, falling back to the logical removed size.
    let reported_bytes = summary.physical_bytes.unwrap_or(summary.logical_bytes);
    if summary.logical_bytes > 0 || reported_bytes > 0 {
        let bytes = if reported_bytes < 1024 {
            format!("{reported_bytes}B")
        } else {
            let (bytes, unit) = human_readable_bytes(reported_bytes);
            format!("{bytes:.1}{unit}")
        };
        if summary.physical_bytes_incomplete {
            write!(printer.stderr(), " (at least {})", bytes.green())?;
        } else {
            write!(printer.stderr(), " ({})", bytes.green())?;
        }
    }

    writeln!(printer.stderr())?;

    Ok(ExitStatus::Success)
}
