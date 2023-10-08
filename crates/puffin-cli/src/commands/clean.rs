use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::commands::ExitStatus;

/// Clear the cache.
pub(crate) async fn clean(cache: Option<&Path>) -> Result<ExitStatus> {
    let Some(cache) = cache else {
        return Err(anyhow::anyhow!("No cache found"));
    };

    if !cache.exists() {
        return Ok(ExitStatus::Success);
    }

    info!("Clearing cache at {}", cache.display());

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
