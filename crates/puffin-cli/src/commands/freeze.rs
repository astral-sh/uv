use anyhow::Result;
use platform_host::Platform;
use puffin_interpreter::{PythonExecutable, SitePackages};
use tracing::debug;

use crate::commands::ExitStatus;

/// Enumerate the installed packages in the current environment.
pub(crate) async fn freeze() -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&python).await?;
    for (name, version) in site_packages.iter() {
        #[allow(clippy::print_stdout)]
        {
            println!("{name}=={version}");
        }
    }

    Ok(ExitStatus::Success)
}
