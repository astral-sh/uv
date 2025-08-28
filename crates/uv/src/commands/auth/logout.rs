use anyhow::{Context, Result, bail};
use std::fmt::Write;
use uv_auth::Credentials;
use uv_configuration::{KeyringProviderType, Service};
use uv_preview::Preview;

use crate::{commands::ExitStatus, printer::Printer};

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
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

    // Unlike login, we'll default to the native provider if none is requested since it's the only
    // valid option and it doesn't matter if the credentials are available in subsequent commands.
    let keyring_provider = keyring_provider.unwrap_or(KeyringProviderType::Native);

    // Be helpful about incompatible `keyring-provider` settings
    let provider = match keyring_provider {
        KeyringProviderType::Native => keyring_provider.to_provider(&preview).unwrap(),
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
