use anyhow::{Context, Result, bail};
use std::fmt::Write;
use uv_configuration::{KeyringProviderType, Service};

use crate::{commands::ExitStatus, printer::Printer};

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    let url = service.url();
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());
    let username = username.unwrap_or_else(|| String::from("__token__"));

    // Unlike login, we'll default to the native provider if none is requested since it's the only
    // valid option and it doesn't matter if the credentials are available in subsequent commands.
    let keyring_provider = keyring_provider.unwrap_or(KeyringProviderType::Native);

    // Be helpful about incompatible `keyring-provider` settings
    let provider = match keyring_provider {
        KeyringProviderType::Native => keyring_provider.to_provider().unwrap(),
        KeyringProviderType::Disabled | KeyringProviderType::Subprocess => {
            bail!(
                "Cannot logout with `keyring-provider = {keyring_provider}`, use `keyring-provider = {}` instead",
                KeyringProviderType::Native
            );
        }
    };

    provider
        .remove(url, &username)
        .await
        .with_context(|| format!("Unable to remove credentials for {display_url}"))?;

    writeln!(printer.stderr(), "Logged out of {display_url}")?;

    Ok(ExitStatus::Success)
}
