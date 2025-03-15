use uv_distribution_types::{InstalledDist, InstalledEggInfoFile};

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(
    dist: &InstalledDist,
) -> Result<uv_install_wheel::Uninstall, UninstallError> {
    let uninstall = tokio::task::spawn_blocking({
        let dist = dist.clone();
        move || match dist {
            InstalledDist::Registry(_) | InstalledDist::Url(_) => {
                Ok(uv_install_wheel::uninstall_wheel(dist.install_path())?)
            }
            InstalledDist::EggInfoDirectory(_) => {
                Ok(uv_install_wheel::uninstall_egg(dist.install_path())?)
            }
            InstalledDist::LegacyEditable(dist) => {
                Ok(uv_install_wheel::uninstall_legacy_editable(&dist.egg_link)?)
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
    Uninstall(#[from] uv_install_wheel::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}
