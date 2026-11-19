use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalAccounting};
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::reporters::print_removal_summary;

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

    print_removal_summary(&summary, "No unused entries found", printer)?;
    Ok(ExitStatus::Success)
}
