use distribution_types::{InstalledDist, InstalledEggInfoFile};

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(
    dist: &InstalledDist,
) -> Result<install_wheel_rs::Uninstall, UninstallError> {
    let uninstall = tokio::task::spawn_blocking({
        let dist = dist.clone();
        move || match dist {
            InstalledDist::Registry(_) | InstalledDist::Url(_) => {
                Ok(install_wheel_rs::uninstall_wheel(dist.path())?)
            }
            InstalledDist::EggInfoDirectory(_) => Ok(install_wheel_rs::uninstall_egg(dist.path())?),
            InstalledDist::LegacyEditable(dist) => {
                Ok(install_wheel_rs::uninstall_legacy_editable(&dist.egg_link)?)
            }
            InstalledDist::EggInfoFile(dist) => Err(UninstallError::Distutils(dist)),
        }
    })
    .await??;

    Ok(uninstall)
}

#[derive(thiserror::Error, Debug)]
pub enum UninstallError {
    #[error("Unable to uninstall `{0}`. distutils-installed distributions do not include the metadata required to uninstall safely.")]
    Distutils(InstalledEggInfoFile),
    #[error(transparent)]
    Uninstall(#[from] install_wheel_rs::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}
