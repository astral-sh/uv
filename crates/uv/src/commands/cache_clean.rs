use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::{Cache, RemovalAccounting};
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};

use crate::commands::reporters::{
    CleaningDirectoryReporter, CleaningPackageReporter, print_removal_summary,
};

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

    let removal_accounting = if preview.is_enabled(PreviewFeature::CachePhysicalSpace) {
        RemovalAccounting::Fine
    } else {
        RemovalAccounting::Coarse
    };
    let cache = cache.with_removal_accounting(removal_accounting);

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

    print_removal_summary(&summary, "No cache entries found", printer)?;
    Ok(ExitStatus::Success)
}
