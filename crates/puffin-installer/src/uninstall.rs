use anyhow::Result;

use puffin_interpreter::DistInfo;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(dist_info: &DistInfo) -> Result<install_wheel_rs::Uninstall> {
    let uninstall = tokio::task::spawn_blocking({
        let path = dist_info.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    Ok(uninstall)
}
