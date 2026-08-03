use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, Removal};
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

    writeln!(
        printer.stderr(),
        "Pruning cache at: {}",
        cache.root().user_display().cyan()
    )?;

    let available_before = preview
        .is_enabled(PreviewFeature::CacheReclaimedSpace)
        .then(|| uv_fs::available_space(cache.root()).ok())
        .flatten();
    let mut summary = Removal::default();

    // Prune the source distribution cache, which is tightly coupled to the builder crate.
    summary += uv_distribution::prune(&cache)
        .with_context(|| format!("Failed to prune cache at: {}", cache.root().user_display()))?;

    // Prune the remaining cache buckets.
    summary += cache
        .prune(ci)
        .with_context(|| format!("Failed to prune cache at: {}", cache.root().user_display()))?;

    let reclaimed_bytes = available_before.and_then(|before| {
        uv_fs::available_space(cache.root())
            .ok()
            .map(|after| after.saturating_sub(before))
    });

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
