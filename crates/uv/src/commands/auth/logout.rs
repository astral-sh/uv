use std::fmt::Write;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;

use uv_auth::Service;
use uv_auth::{Credentials, TextCredentialStore};
use uv_configuration::KeyringProviderType;
use uv_distribution_types::IndexUrl;
use uv_pep508::VerbatimUrl;
use uv_preview::Preview;

use crate::commands::auth::AuthBackend;
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
    let backend = AuthBackend::from_settings(keyring_provider.as_ref(), preview)?;

    // TODO(zanieb): Use a shared abstraction across `login` and `logout`?
    let url = service.url().clone();
    let (service, url) = match IndexUrl::from(VerbatimUrl::from_url(url.clone())).root() {
        Some(root) => (Service::try_from(root.clone())?, root),
        None => (service, url),
    };

    // Extract credentials from URL if present
    let url_credentials = Credentials::from_url(&url);
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

    // TODO(zanieb): Consider exhaustively logging out from all backends
    match backend {
        AuthBackend::Keyring(provider) => {
            provider
                .remove(&url, &username)
                .await
                .with_context(|| format!("Unable to remove credentials for {display_url}"))?;
        }
        AuthBackend::TextStore(mut store, _lock) => {
            if store.remove(&service).is_none() {
                bail!("No matching entry found for {display_url}");
            }
            store
                .write(TextCredentialStore::default_file()?, _lock)
                .with_context(|| "Failed to persist changes to credentials after removal")?;
        }
    }

    writeln!(
        printer.stderr(),
        "Removed credentials for {}",
        display_url.bold().cyan()
    )?;

    Ok(ExitStatus::Success)
}
