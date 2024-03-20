use anyhow::Result;

use distribution_types::InstalledDist;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(
    dist: &InstalledDist,
) -> Result<install_wheel_rs::Uninstall, UninstallError> {
    let uninstall = tokio::task::spawn_blocking({
        let path = dist.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    Ok(uninstall)
}

#[derive(thiserror::Error, Debug)]
pub enum UninstallError {
    #[error(transparent)]
    Uninstall(#[from] install_wheel_rs::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}
