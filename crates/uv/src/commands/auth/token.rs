use std::fmt::Write;

use anyhow::{Context, Result, bail};

use uv_configuration::{KeyringProviderType, Service};
use uv_preview::Preview;

use crate::{Printer, commands::ExitStatus};

/// Show the token that will be used for a service.
pub(crate) async fn token(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // Determine the keyring provider to use
    let Some(keyring_provider) = &keyring_provider else {
        bail!("Retrieving credentials requires setting a `keyring-provider`");
    };
    let Some(provider) = keyring_provider.to_provider(&preview) else {
        bail!("Cannot retrieve credentials with `keyring-provider = {keyring_provider}`");
    };

    let url = service.url();
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());

    let credentials = provider
        .fetch(url, Some(username.as_deref().unwrap_or("__token__")))
        .await
        .with_context(|| format!("Failed to fetch credentials for {display_url}"))?;

    let Some(password) = credentials.password() else {
        bail!(
            "No {} found for {display_url}",
            if username.is_some() {
                "password"
            } else {
                "token"
            }
        );
    };

    writeln!(printer.stdout(), "{password}")?;
    Ok(ExitStatus::Success)
}
