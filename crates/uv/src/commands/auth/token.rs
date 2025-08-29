use std::fmt::Write;

use anyhow::{Context, Result, bail};

use uv_auth::Credentials;
use uv_auth::Service;
use uv_configuration::KeyringProviderType;
use uv_preview::Preview;

use crate::{commands::ExitStatus, printer::Printer};

/// Show the token that will be used for a service.
pub(crate) async fn token(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let url = service.url();
    // Determine the keyring provider to use
    let Some(keyring_provider) = &keyring_provider else {
        bail!("Retrieving credentials requires setting a `keyring-provider`");
    };
    let Some(provider) = keyring_provider.to_provider(&preview) else {
        bail!("Cannot retrieve credentials with `keyring-provider = {keyring_provider}`");
    };

    // Extract credentials from URL if present
    let url_credentials = Credentials::from_url(url);
    let url_username = url_credentials.as_ref().and_then(|c| c.username());

    let username = match (username, url_username) {
        (Some(cli), Some(url)) => {
            bail!(
                "Cannot specify a username both via the URL and CLI; found `--username {cli}` and `{url}`"
            );
        }
        (Some(cli), None) => cli,
        (None, Some(url)) => url.to_string(),
        (None, None) => "__token__".to_string(),
    };

    let display_url = if username == "__token__" {
        url.without_credentials().to_string()
    } else {
        format!("{username}@{}", url.without_credentials())
    };

    let credentials = provider
        .fetch(url, Some(&username))
        .await
        .with_context(|| format!("Failed to fetch credentials for {display_url}"))?;

    let Some(password) = credentials.password() else {
        bail!(
            "No {} found for {display_url}",
            if username != "__token__" {
                "password"
            } else {
                "token"
            }
        );
    };

    writeln!(printer.stdout(), "{password}")?;
    Ok(ExitStatus::Success)
}
