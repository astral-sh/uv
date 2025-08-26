use anyhow::{Context, Result, bail};

use uv_configuration::KeyringProviderType;
use uv_redacted::DisplaySafeUrl;

use crate::commands::ExitStatus;

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: String,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
) -> Result<ExitStatus> {
    let username = if let Some(username) = username {
        username
    } else {
        String::from("__token__")
    };
    let url = DisplaySafeUrl::parse(&service)?;

    let Some(keyring_provider) = keyring_provider.and_then(|p| p.to_provider()) else {
        bail!("`--keyring-provider native` is required for system keyring credential configuration")
    };

    keyring_provider
        .remove(&url, &username)
        .await
        .with_context(|| format!("Unable to remove credentials for {url}"))?;

    Ok(ExitStatus::Success)
}
