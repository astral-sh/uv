use crate::commands::ExitStatus;
use anyhow::Result;

/// Sync the project environment.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn sync(python: Option<String>) -> Result<ExitStatus> {
    Ok(ExitStatus::Success)
}
