use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;
use std::fmt::Write;

use uv_auth::{Credentials, TokenStore};
use uv_client::BaseClientBuilder;
use uv_configuration::{KeyringProviderType, Service};
use uv_redacted::DisplaySafeUrl;

use crate::commands::auth::login::is_pyx_url;
use crate::settings::NetworkSettings;
use crate::{commands::ExitStatus, printer::Printer};

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: Service,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    network_settings: &NetworkSettings,
    printer: Printer,
) -> Result<ExitStatus> {
    let url = service.url();
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());

    if is_pyx_url(&url) {
        return pyx_logout(network_settings, printer).await;
    }

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

async fn pyx_logout(
    network_settings: &NetworkSettings,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let store = TokenStore::from_settings()?;

    // Initialize the client.
    let client = BaseClientBuilder::default()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .build();

    // Retrieve the token store.
    let Some(tokens) = store.read().await? else {
        writeln!(
            printer.stderr(),
            "{}",
            format_args!("No credentials found for {}", store.api().bold().cyan())
        )?;
        return Ok(ExitStatus::Success);
    };

    // Add the token to the request.
    let url = {
        let mut url = store.api().clone();
        url.set_path("auth/cli/logout");
        url
    };

    // Build a basic request first, then authenticate it
    let request = reqwest::Request::new(reqwest::Method::GET, url.into());
    let request = Credentials::from(tokens).authenticate(request);

    // Hit the logout endpoint using the client's execute method
    let response = client.execute(request).await?;
    match response.error_for_status_ref() {
        Ok(..) => {}
        Err(err) if matches!(err.status(), Some(reqwest::StatusCode::UNAUTHORIZED)) => {
            tracing::debug!(
                "Received 401 (Unauthorized) response from logout endpoint; removing tokens..."
            );
        }
        Err(err) => {
            return Err(err.into());
        }
    }

    // Remove the tokens from the store.
    match store.delete().await {
        Ok(..) => {}
        Err(err) if matches!(err.kind(), std::io::ErrorKind::NotFound) => {}
        Err(err) => return Err(err.into()),
    }

    writeln!(
        printer.stderr(),
        "{}",
        format_args!("Logged out from {}", store.api().bold().cyan())
    )?;

    Ok(ExitStatus::Success)
}
