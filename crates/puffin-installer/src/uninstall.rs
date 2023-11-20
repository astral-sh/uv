use anyhow::Result;

use distribution_types::InstalledDist;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(dist: &InstalledDist) -> Result<install_wheel_rs::Uninstall> {
    let uninstall = tokio::task::spawn_blocking({
        let path = dist.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    Ok(uninstall)
}
