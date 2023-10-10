use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Clear the cache.
pub(crate) async fn clean(cache: Option<&Path>, mut printer: Printer) -> Result<ExitStatus> {
    let Some(cache) = cache else {
        return Err(anyhow::anyhow!("No cache found"));
    };

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
            tokio::fs::remove_dir_all(entry.path())
                .await
                .with_context(|| format!("Failed to clear cache at {}", cache.display()))?;
        } else {
            tokio::fs::remove_file(entry.path())
                .await
                .with_context(|| format!("Failed to clear cache at {}", cache.display()))?;
        }
    }

    Ok(ExitStatus::Success)
}
