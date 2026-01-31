use anyhow::Result;

use crate::commands::ExitStatus;

/// do pip index versions but with uv
pub(crate) async fn pip_index_versions() -> Result<ExitStatus> {
    print!("hello from uv pip index-versions");
    return Ok(ExitStatus::Success);
}
