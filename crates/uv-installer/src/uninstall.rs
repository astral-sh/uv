use distribution_types::InstalledDist;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(
    dist: &InstalledDist,
) -> Result<install_wheel_rs::Uninstall, UninstallError> {
    let uninstall = tokio::task::spawn_blocking({
        let dist = dist.clone();
        move || match dist {
            InstalledDist::Registry(_) | InstalledDist::Url(_) => {
                install_wheel_rs::uninstall_wheel(dist.path())
            }
            InstalledDist::EggInfo(_) => install_wheel_rs::uninstall_egg(dist.path()),
            InstalledDist::LegacyEditable(dist) => {
                install_wheel_rs::uninstall_legacy_editable(&dist.egg_link)
            }
        }
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
