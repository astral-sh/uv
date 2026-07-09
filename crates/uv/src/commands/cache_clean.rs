use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, Removal};
use uv_fs::Simplified;
use uv_normalize::PackageName;

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
    if !cache.root().exists() {
        writeln!(
            printer.stderr(),
            "No cache found at: {}",
            cache.root().user_display().cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    let mut cache = match cache.with_exclusive_lock_no_wait() {
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

    // Long-lived commands (e.g., servers started with `uv run`) release the main cache lock
    // while running, and instead hold in-use locks on the cache entries they run from. Since
    // removing the entire cache would delete those entries, wait for them to be released.
    // (Package-scoped cleaning skips in-use entries instead of waiting.)
    //
    // The main lock must not be held while waiting: a child process holding an in-use lock
    // may itself invoke uv, which would block on the main lock and prevent the child from
    // ever exiting. Instead, probe under the main lock, release it while waiting on the
    // in-use lock, then reacquire and re-check.
    if !force && packages.is_empty() {
        let mut warned = false;
        while let Some(held) = cache
            .find_held_in_use_lock()
            .context("Failed to read the cache's in-use locks")?
        {
            if !warned {
                writeln!(
                    printer.stderr(),
                    "Cache is currently in-use, waiting for other uv processes to finish (use `--force` to override)"
                )?;
                warned = true;
            }
            cache.release_lock();
            Cache::wait_for_in_use_lock(&held).await.context(
                "Failed waiting for a running uv-launched process to exit (use `--force` to override)",
            )?;
            cache = cache.with_exclusive_lock().await?;
        }
    }

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
