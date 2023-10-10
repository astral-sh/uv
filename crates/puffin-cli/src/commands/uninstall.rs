use std::fmt::Write;
use std::path::Path;

use anyhow::{anyhow, Result};
use tracing::debug;

use platform_host::Platform;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall a package from the current environment.
pub(crate) async fn uninstall(
    name: &str,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Index the current `site-packages` directory.
    let site_packages = puffin_interpreter::SitePackages::from_executable(&python).await?;

    // Locate the package in the environment.
    let Some(dist_info) = site_packages.get(&PackageName::normalize(name)) else {
        return Err(anyhow!("Package not installed: {}", name));
    };

    // Uninstall the package from the environment.
    let uninstall = puffin_installer::uninstall(dist_info).await?;

    // Print a summary of the uninstallation.
    match (uninstall.file_count, uninstall.dir_count) {
        (0, 0) => writeln!(printer, "No files found")?,
        (1, 0) => writeln!(printer, "Removed 1 file")?,
        (0, 1) => writeln!(printer, "Removed 1 directory")?,
        (1, 1) => writeln!(printer, "Removed 1 file and 1 directory")?,
        (file_count, 0) => writeln!(printer, "Removed {file_count} files")?,
        (0, dir_count) => writeln!(printer, "Removed {dir_count} directories")?,
        (file_count, dir_count) => writeln!(
            printer,
            "Removed {file_count} files and {dir_count} directories"
        )?,
    }

    Ok(ExitStatus::Success)
}
