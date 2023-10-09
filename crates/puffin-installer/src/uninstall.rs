use anyhow::{anyhow, Result};
use tracing::info;

use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(name: &str, python: &PythonExecutable) -> Result<()> {
    // Index the current `site-packages` directory.
    let site_packages = puffin_interpreter::SitePackages::from_executable(python).await?;

    // Locate the package in the environment.
    let Some(dist_info) = site_packages.get(&PackageName::normalize(name)) else {
        return Err(anyhow!("Package not installed: {}", name));
    };

    // Uninstall the package from the environment.
    let uninstall = tokio::task::spawn_blocking({
        let path = dist_info.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    // Print a summary of the uninstallation.
    match (uninstall.file_count, uninstall.dir_count) {
        (0, 0) => info!("No files found"),
        (1, 0) => info!("Removed 1 file"),
        (0, 1) => info!("Removed 1 directory"),
        (1, 1) => info!("Removed 1 file and 1 directory"),
        (file_count, 0) => info!("Removed {file_count} files"),
        (0, dir_count) => info!("Removed {dir_count} directories"),
        (file_count, dir_count) => info!("Removed {file_count} files and {dir_count} directories"),
    }

    Ok(())
}
