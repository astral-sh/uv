use std::fmt::Write;
use std::time::Duration;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalAccounting};
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::{ExitStatus, human_readable_bytes};
use crate::printer::Printer;

/// Prune dangling cache entries and cached environments.
pub(crate) async fn cache_prune(
    ci: bool,
    force: bool,
    max_age: Option<Duration>,
    dry_run: bool,
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
        Err(cache) if force && max_age.is_none() => {
            debug!("Cache is currently in use, proceeding due to `--force`");
            cache
        }
        Err(cache) => {
            if max_age.is_some() {
                writeln!(
                    printer.stderr(),
                    "Cache is currently in-use, waiting for other uv processes to finish"
                )?;
            } else {
                writeln!(
                    printer.stderr(),
                    "Cache is currently in-use, waiting for other uv processes to finish (use `--force` to override)"
                )?;
            }
            cache.with_exclusive_lock().await?
        }
    };

    if let Some(max_age) = max_age {
        let paths = cache.prune_unused(max_age, dry_run).with_context(|| {
            format!("Failed to prune cache at: {}", cache.root().user_display())
        })?;
        let action = if dry_run { "Would remove" } else { "Removed" };
        let wheels = if paths.len() == 1 { "wheel" } else { "wheels" };
        writeln!(printer.stderr(), "{action} {} unused {wheels}", paths.len())?;
        if dry_run {
            for path in paths {
                writeln!(printer.stderr(), "  {}", path.user_display())?;
            }
        }
        return Ok(ExitStatus::Success);
    }

    let removal_accounting = if preview.is_enabled(PreviewFeature::CachePhysicalSpace) {
        RemovalAccounting::Fine
    } else {
        RemovalAccounting::Coarse
    };
    let cache = cache.with_removal_accounting(removal_accounting);

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
