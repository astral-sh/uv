use anyhow::Result;

use uv_tool::tool_executable_dir;

use crate::commands::{ExitStatus, update_shell};
use crate::printer::Printer;

/// Ensure that the tool executable directory is in PATH.
pub(crate) async fn update_shell(printer: Printer) -> Result<ExitStatus> {
    update_shell::update_shell(&tool_executable_dir()?, printer).await
}
