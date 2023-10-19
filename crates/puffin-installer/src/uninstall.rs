use anyhow::Result;

use puffin_interpreter::Distribution;

/// Uninstall a package from the specified Python environment.
pub async fn uninstall(distribution: &Distribution) -> Result<install_wheel_rs::Uninstall> {
    let uninstall = tokio::task::spawn_blocking({
        let path = distribution.path().to_owned();
        move || install_wheel_rs::uninstall_wheel(&path)
    })
    .await??;

    Ok(uninstall)
}
