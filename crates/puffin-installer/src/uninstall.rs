use anyhow::Result;

use crate::InstalledDistribution;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(
    distribution: &InstalledDistribution,
) -> Result<install_wheel_rs::Uninstall> {
    let uninstall = tokio::task::spawn_blocking({
        let path = distribution.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    Ok(uninstall)
}
