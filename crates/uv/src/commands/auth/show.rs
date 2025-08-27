use std::fmt::Write;

use anyhow::{Context, Result, bail};

use uv_auth::{Credentials, KeyringProvider};
use uv_configuration::{KeyringProviderType, Service};

use crate::{Printer, commands::ExitStatus};

/// Show credentials for a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn show(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    // Be helpful about incompatible `keyring-provider` settings
    if let Some(provider) = &keyring_provider {
        match provider {
            KeyringProviderType::Native => {}
            KeyringProviderType::Disabled => {
                bail!(
                    "Cannot show credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead"
                );
            }
            KeyringProviderType::Subprocess => {
                bail!(
                    "Cannot show credentials with `keyring-provider = subprocess`, use `keyring-provider = native` instead"
                );
            }
        }
    }

    let url = service.url();

    // Always use the native keyring provider
    let provider = KeyringProvider::native();

    let credentials = provider
        .fetch(url, username.as_deref())
        .await
        .with_context(|| format!("Failed to fetch credentials for {url}"))?;

    let Some(password) = credentials.password() else {
        bail!("No password found in credentials");
    };

    // Only show a username if it wasn't provided
    let username = username
        .is_none()
        .then(|| show_username(&credentials))
        .flatten();

    if let Some(username) = username {
        writeln!(printer.stdout(), "{username}:{password}")?;
    } else {
        writeln!(printer.stdout(), "{password}")?;
    }

    Ok(ExitStatus::Success)
}

/// Return the username to show.
///
/// If missing or set to `__token__`, returns `None`.
fn show_username(credentials: &Credentials) -> Option<&str> {
    let username = credentials.username()?;
    if username == "__token__" {
        return None;
    }
    Some(username)
}
