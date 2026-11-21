use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalAccounting};
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::reporters::{RemovalTense, print_removal_summary};

/// How to handle the cache entries that no environment references.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum UnusedEntries {
    /// Keep the entries.
    Keep,
    /// Report the entries that would be removed, without removing anything.
    Report,
    /// Remove the entries.
    Remove,
}

impl UnusedEntries {
    /// Whether to consider the cache entries that no environment references.
    fn enabled(self) -> bool {
        match self {
            Self::Keep => false,
            Self::Report | Self::Remove => true,
        }
    }

    /// Whether to report the entries, rather than remove them.
    fn dry_run(self) -> bool {
        match self {
            Self::Keep | Self::Remove => false,
            Self::Report => true,
        }
    }
}

/// Prune dangling cache entries and cached environments.
pub(crate) async fn cache_prune(
    ci: bool,
    force: bool,
    unused: UnusedEntries,
    environments: Vec<PathBuf>,
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

    let dry_run = unused.dry_run();
    if dry_run {
        writeln!(
            printer.stderr(),
            "Checking for unused cache entries in: {}",
            cache.root().user_display().cyan()
        )?;
    } else {
        writeln!(
            printer.stderr(),
            "Pruning cache at: {}",
            cache.root().user_display().cyan()
        )?;
    }

    let mut summary = cache.removal();

    // Prune the cache entries that no environment references.
    if unused.enabled() {
        summary += cache
            .prune_unused(&environments, dry_run)
            .with_context(|| {
                format!("Failed to prune cache at: {}", cache.root().user_display())
            })?;
    }

    if !dry_run {
        // Prune the source distribution cache, which is tightly coupled to the builder crate.
        summary += uv_distribution::prune(&cache).with_context(|| {
            format!("Failed to prune cache at: {}", cache.root().user_display())
        })?;

        // Prune the remaining cache buckets.
        summary += cache.prune(ci).with_context(|| {
            format!("Failed to prune cache at: {}", cache.root().user_display())
        })?;
    }

    let tense = if dry_run {
        RemovalTense::Conditional
    } else {
        RemovalTense::Past
    };
    print_removal_summary(&summary, "No unused entries found", tense, printer)?;
    Ok(ExitStatus::Success)
}
