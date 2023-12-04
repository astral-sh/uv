use std::fmt::Write;

use anyhow::{Context, Result};
use fs_err as fs;
use tracing::debug;

use puffin_cache::Cache;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Clear the cache.
pub(crate) fn clean(cache: &Cache, mut printer: Printer) -> Result<ExitStatus> {
    if !cache.root().exists() {
        writeln!(printer, "No cache found at: {}", cache.root().display())?;
        return Ok(ExitStatus::Success);
    }

    debug!("Clearing cache at: {}", cache.root().display());

    for entry in cache
        .root()
        .read_dir()
        .with_context(|| {
            format!(
                "Failed to read directory contents while clearing {}",
                cache.root().display()
            )
        })?
        .flatten()
    {
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())
                .with_context(|| format!("Failed to clear cache at: {}", cache.root().display()))?;
        } else {
            fs::remove_file(entry.path())
                .with_context(|| format!("Failed to clear cache at: {}", cache.root().display()))?;
        }
    }

    Ok(ExitStatus::Success)
}
