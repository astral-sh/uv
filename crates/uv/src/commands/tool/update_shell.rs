use anyhow::Result;

use uv_command_support::{ExitStatus, Printer, update_shell};
use uv_tool::tool_executable_dir;

/// Ensure that the tool executable directory is in PATH.
pub(crate) async fn update_shell(printer: Printer) -> Result<ExitStatus> {
    update_shell::update_shell(&tool_executable_dir()?, printer).await
}
