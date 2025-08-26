use anyhow::{Context, Result, bail};
use std::fmt::Write;

use uv_auth::KeyringProvider;
use uv_configuration::KeyringProviderType;
use uv_redacted::DisplaySafeUrl;

use crate::{commands::ExitStatus, printer::Printer};

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: String,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    let username = if let Some(username) = username {
        username
    } else {
        String::from("__token__")
    };
    let url = DisplaySafeUrl::parse(&service)?;

    // Be helpful about incompatible `keyring-provider` settings
    if let Some(provider) = &keyring_provider {
        match provider {
            KeyringProviderType::Native => {}
            KeyringProviderType::Disabled => {
                bail!(
                    "Cannot login with `keyring-provider = disabled`, use `keyring-provider = native` instead"
                );
            }
            KeyringProviderType::Subprocess => {
                bail!(
                    "Cannot login with `keyring-provider = subprocess`, use `keyring-provider = native` instead"
                );
            }
        }
    }

    // Always use the native keyring provider
    let provider = KeyringProvider::native();

    provider
        .remove(&url, &username)
        .await
        .with_context(|| format!("Unable to remove credentials for {url}"))?;

    writeln!(printer.stderr(), "Logged out of {url}")?;

    Ok(ExitStatus::Success)
}
