use std::fmt::Write;

use anyhow::{Result, bail};

use uv_auth::Credentials;
use uv_auth::Service;
use uv_configuration::KeyringProviderType;
use uv_preview::Preview;

use crate::commands::auth::AuthBackend;
use crate::{commands::ExitStatus, printer::Printer};

/// Show the token that will be used for a service.
pub(crate) async fn token(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let backend = AuthBackend::from_settings(keyring_provider.as_ref(), preview)?;
    let url = service.url();

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

    let credentials = match &backend {
        AuthBackend::Keyring(provider) => provider
            .fetch(url, Some(&username))
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to fetch credentials for {display_url}"))?,
        AuthBackend::TextStore(store, _lock) => store
            .get_credentials(url)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Failed to fetch credentials for {display_url}"))?,
    };

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
