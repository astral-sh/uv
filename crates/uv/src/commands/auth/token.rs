use std::fmt::Write;

use anyhow::{Context, Result, bail};

use uv_auth::KeyringProvider;
use uv_configuration::{KeyringProviderType, Service};

use crate::{Printer, commands::ExitStatus};

/// Show the token that would be used for a service.
pub(crate) async fn token(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    let name = if username.is_some() {
        "password"
    } else {
        "token"
    };

    // Be helpful about incompatible `keyring-provider` settings
    if let Some(provider) = &keyring_provider {
        match provider {
            KeyringProviderType::Native => {}
            KeyringProviderType::Disabled => {
                bail!(
                    "Cannot retrieve credentials with `keyring-provider = disabled`, use `keyring-provider = native` instead"
                );
            }
            KeyringProviderType::Subprocess => {
                bail!(
                    "Cannot retrieve credentials with `keyring-provider = subprocess`, use `keyring-provider = native` instead"
                );
            }
        }
    }

    let url = service.url();
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());

    // Always use the native keyring provider
    let provider = KeyringProvider::native();

    let credentials = provider
        .fetch(url, Some(username.as_deref().unwrap_or("__token__")))
        .await
        .with_context(|| format!("Failed to fetch credentials for {display_url}"))?;

    let Some(password) = credentials.password() else {
        bail!("No {name} found for {display_url}");
    };

    writeln!(printer.stdout(), "{password}")?;
    Ok(ExitStatus::Success)
}
