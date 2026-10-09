use anyhow::Result;

use uv_python_managed::python_executable_dir;

use uv_command_support::Printer;
use uv_command_support::{ExitStatus, update_shell};

/// Ensure that the Python executable directory is in PATH.
pub async fn update_shell(printer: Printer) -> Result<ExitStatus> {
    update_shell::update_shell(&python_executable_dir()?, printer).await
}
