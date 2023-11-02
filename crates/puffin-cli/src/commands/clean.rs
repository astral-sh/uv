use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use fs_err as fs;
use tracing::debug;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Clear the cache.
pub(crate) fn clean(cache: &Path, mut printer: Printer) -> Result<ExitStatus> {
    if !cache.exists() {
        writeln!(printer, "No cache found at: {}", cache.display())?;
        return Ok(ExitStatus::Success);
    }

    debug!("Clearing cache at: {}", cache.display());

    for entry in cache
        .read_dir()
        .with_context(|| {
            format!(
                "Failed to read directory contents while clearing {}",
                cache.display()
            )
        })?
        .flatten()
    {
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())
                .with_context(|| format!("Failed to clear cache at {}", cache.display()))?;
        } else {
            fs::remove_file(entry.path())
                .with_context(|| format!("Failed to clear cache at {}", cache.display()))?;
        }
    }

    Ok(ExitStatus::Success)
}
