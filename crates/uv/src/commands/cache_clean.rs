use std::fmt::Write;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, Removal};
use uv_fs::Simplified;
use uv_normalize::PackageName;

use crate::commands::cache_clean_daemon::spawn_background_clean;
use crate::commands::reporters::{CleaningDirectoryReporter, CleaningPackageReporter};
use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Clear the cache, removing all entries or those linked to specific packages.
pub(crate) async fn cache_clean(
    packages: &[PackageName],
    force: bool,
    background: bool,
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

    // Background mode is only supported for full cache clearing
    if background && !packages.is_empty() {
        bail!("The `--background` flag is not supported when clearing specific packages");
    }

    // Background mode: take a lock, move the cache, then spawn a daemon to delete it.
    if background {
        return cache_clean_background(cache, printer).await;
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

/// Clear the cache in the background by moving it to a temporary directory and spawning a daemon.
async fn cache_clean_background(cache: Cache, printer: Printer) -> Result<ExitStatus> {
    let cache_root = cache.root().to_path_buf();

    // Take an exclusive lock before moving the cache directory.
    let cache = cache.with_exclusive_lock().await?;

    writeln!(
        printer.stderr(),
        "Clearing cache at: {} (in background)",
        cache_root.user_display().cyan()
    )?;

    // Create a temporary directory in the same parent directory as the cache.
    // This ensures the rename is atomic (same filesystem).
    let parent = cache_root
        .parent()
        .context("Cache root has no parent directory")?;
    let temp_dir = tempfile::Builder::new()
        .prefix(".uv-cache-clean-")
        .tempdir_in(parent)
        .context("Failed to create temporary directory for cache cleaning")?;
    let temp_dir = temp_dir.keep();

    // Remove the empty tempdir so we can atomically rename the cache to this path.
    fs_err::remove_dir(&temp_dir)?;

    // Move the cache directory to the temporary location.
    // This should be nearly instantaneous as it's just a rename operation.
    debug!(
        "Moving cache from {} to {}",
        cache_root.display(),
        temp_dir.display()
    );

    fs_err::rename(&cache_root, &temp_dir).with_context(|| {
        format!(
            "Failed to move cache to temporary directory: {}",
            temp_dir.display()
        )
    })?;

    // Release the lock (the lock file was inside the cache dir, which has been moved).
    drop(cache);

    // Spawn a background daemon to delete the temporary directory
    spawn_background_clean(&temp_dir)?;

    writeln!(
        printer.stderr(),
        "Cache moved; deletion continuing in background"
    )?;

    Ok(ExitStatus::Success)
}
