use anyhow::Result;

use uv_command_support::{ExitStatus, Printer, update_shell};
use uv_python::managed::python_executable_dir;

/// Ensure that the Python executable directory is in PATH.
pub(crate) async fn update_shell(printer: Printer) -> Result<ExitStatus> {
    update_shell::update_shell(&python_executable_dir()?, printer).await
}
