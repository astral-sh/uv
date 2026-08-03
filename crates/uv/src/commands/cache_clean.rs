use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
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

    let measure_reclaimed_space = preview.is_enabled(PreviewFeature::CacheReclaimedSpace);
    let cache = cache.with_reclaimed_space(measure_reclaimed_space);

    // The cache root itself is removed when cleaning the entire cache, so measure its parent.
    let filesystem_path = cache.root().parent().unwrap_or(cache.root()).to_path_buf();
    let available_before = measure_reclaimed_space
        .then(|| uv_fs::available_space(&filesystem_path).ok())
        .flatten();

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

    let reclaimed_bytes = summary.reclaimed_bytes.or_else(|| {
        available_before.and_then(|before| {
            uv_fs::available_space(&filesystem_path)
                .ok()
                .map(|after| after.saturating_sub(before))
        })
    });

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

    // If any, report the reclaimed space, falling back to the apparent removed size.
    if summary.total_bytes > 0 {
        let total_bytes = reclaimed_bytes.unwrap_or(summary.total_bytes);
        let bytes = if total_bytes < 1024 {
            format!("{total_bytes}B")
        } else {
            let (bytes, unit) = human_readable_bytes(total_bytes);
            format!("{bytes:.1}{unit}")
        };
        write!(printer.stderr(), " ({})", bytes.green())?;
    }

    writeln!(printer.stderr())?;

    Ok(ExitStatus::Success)
}
