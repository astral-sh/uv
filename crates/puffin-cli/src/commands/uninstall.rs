use std::path::Path;

use anyhow::Result;
use tracing::debug;

use platform_host::Platform;
use puffin_interpreter::PythonExecutable;

use crate::commands::ExitStatus;

/// Uninstall a package from the current environment.
pub(crate) async fn uninstall(name: &str, cache: Option<&Path>) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Uninstall the package from the current environment.
    puffin_installer::uninstall(name, &python).await?;

    Ok(ExitStatus::Success)
}
