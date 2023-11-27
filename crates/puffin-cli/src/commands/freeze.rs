use anyhow::Result;
use tracing::debug;

use platform_host::Platform;
use puffin_cache::Cache;
use puffin_installer::SitePackages;
use puffin_interpreter::Virtualenv;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn freeze(cache: &Cache, _printer: Printer) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = Virtualenv::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.python_executable().display()
    );

    // Build the installed index.
    let site_packages = SitePackages::try_from_executable(&python)?;
    for dist in site_packages.distributions() {
        #[allow(clippy::print_stdout)]
        {
            println!("{dist}");
        }
    }

    Ok(ExitStatus::Success)
}
