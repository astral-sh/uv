use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalMode};
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Prune dangling cache entries and cached environments.
pub(crate) async fn cache_prune(
    ci: bool,
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

    writeln!(
        printer.stderr(),
        "Pruning cache at: {}",
        cache.root().user_display().cyan()
    )?;

    let mut summary = cache.removal();

    // Prune the source distribution cache, which is tightly coupled to the builder crate.
    summary += uv_distribution::prune(&cache)
        .with_context(|| format!("Failed to prune cache at: {}", cache.root().user_display()))?;

    // Prune the remaining cache buckets.
    summary += cache
        .prune(ci)
        .with_context(|| format!("Failed to prune cache at: {}", cache.root().user_display()))?;

    // Write a summary of the number of files and directories removed.
    match (summary.num_files, summary.num_dirs) {
        (0, 0) => {
            write!(printer.stderr(), "No unused entries found")?;
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
        let bytes = human_readable_bytes(reported_bytes);
        if summary.physical_bytes_incomplete {
            write!(printer.stderr(), " (at least {:.1})", bytes.green())?;
        } else {
            write!(printer.stderr(), " ({:.1})", bytes.green())?;
        }
    }

    writeln!(printer.stderr())?;

    Ok(ExitStatus::Success)
}
