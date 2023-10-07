use std::path::Path;

use anyhow::Result;
use tracing::info;

use crate::commands::ExitStatus;

/// Clear the cache.
pub(crate) async fn clean(cache: Option<&Path>) -> Result<ExitStatus> {
    let Some(cache) = cache else {
        return Err(anyhow::anyhow!("No cache found"));
    };

    info!("Clearing cache at {}", cache.display());
    cacache::clear(cache).await?;
    Ok(ExitStatus::Success)
}
