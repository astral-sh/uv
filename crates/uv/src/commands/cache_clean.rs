use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, Removal, RemovalAccounting};
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};
use uv_python_managed::ManagedPythonInstallations;

use crate::commands::human_readable_bytes;
use crate::commands::reporters::{CleaningDirectoryReporter, CleaningPackageReporter};

/// Clear the cache, removing all entries or those linked to specific packages.
pub(crate) async fn cache_clean(
    packages: &[PackageName],
    force: bool,
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let removal_accounting = if preview.is_enabled(PreviewFeature::CachePhysicalSpace) {
        RemovalAccounting::Fine
    } else {
        RemovalAccounting::Coarse
    };
    let cache = cache.with_removal_accounting(removal_accounting);

    let mut summary = if packages.is_empty() {
        clean_python_downloads(&cache, printer).await?
    } else {
        cache.removal()
    };
    if let Some(cache_summary) = clean_cache(packages, force, cache, printer).await? {
        summary += cache_summary;
    } else if summary.num_files == 0 && summary.num_dirs == 0 {
        return Ok(ExitStatus::Success);
    }

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

    // Prefer the fine-grained estimate, falling back to coarse accounting.
    let reported_bytes = summary.fine_bytes.unwrap_or(summary.coarse_bytes);
    if summary.num_files > 0 || summary.num_dirs > 0 {
        let bytes = human_readable_bytes(reported_bytes);
        if summary.fine_bytes_incomplete {
            write!(printer.stderr(), " (at least {:.1})", bytes.green())?;
        } else {
            write!(printer.stderr(), " ({:.1})", bytes.green())?;
        }
    }

    writeln!(printer.stderr())?;

    Ok(ExitStatus::Success)
}

/// Remove cache entries, returning `None` if the cache directory does not exist.
async fn clean_cache(
    packages: &[PackageName],
    force: bool,
    cache: Cache,
    printer: Printer,
) -> Result<Option<Removal>> {
    if !cache.root().exists() {
        writeln!(
            printer.stderr(),
            "No cache found at: {}",
            cache.root().user_display().cyan()
        )?;
        return Ok(None);
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
        let mut summary = cache.removal();

        for package in packages {
            let removed = cache.remove(package)?;
            summary += removed;
            reporter.on_clean(package.as_str(), &summary);
        }
        summary += cache.prune_archive_files()?;
        reporter.on_complete();

        summary
    };

    Ok(Some(summary))
}

/// Remove abandoned managed Python downloads outside the cache directory.
async fn clean_python_downloads(cache: &Cache, printer: Printer) -> Result<Removal> {
    let installations = ManagedPythonInstallations::from_settings(None)?;
    let scratch = installations.scratch();
    if !scratch.try_exists()? {
        return Ok(cache.removal());
    }

    let lock = installations.lock().await?;
    let cache = cache.clone();
    let summary = tokio::task::spawn_blocking(move || {
        let mut summary = cache.removal();
        installations.clear_scratch(&lock, |path| {
            summary += cache.remove_path(path)?;
            Ok(())
        })?;
        Ok::<_, std::io::Error>(summary)
    })
    .await?
    .with_context(|| {
        format!(
            "Failed to clear temporary Python installation files at: {}",
            scratch.user_display()
        )
    })?;
    if summary.num_files > 0 || summary.num_dirs > 0 {
        writeln!(
            printer.stderr(),
            "Cleared temporary Python installation files at: {}",
            scratch.user_display().cyan()
        )?;
    }
    Ok(summary)
}
