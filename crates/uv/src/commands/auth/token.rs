use std::fmt::Write;

use anyhow::{Result, bail};
use tracing::debug;

use uv_auth::{AuthBackend, Service};
use uv_auth::{Credentials, PyxTokenStore};
use uv_client::{AuthIntegration, BaseClient, BaseClientBuilder};
use uv_preview::Preview;

use crate::commands::ExitStatus;
use crate::commands::auth::login;
use crate::printer::Printer;

/// Show the token that will be used for a service.
pub(crate) async fn token(
    service: Service,
    username: Option<String>,
    client_builder: BaseClientBuilder<'_>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let pyx_store = PyxTokenStore::from_settings()?;
    if pyx_store.is_known_domain(service.url()) {
        if username.is_some() {
            bail!("Cannot specify a username when logging in to pyx");
        }
        let client = client_builder
            .auth_integration(AuthIntegration::NoAuthMiddleware)
            .build();

        pyx_refresh(&pyx_store, &client, printer).await?;
        return Ok(ExitStatus::Success);
    }

    let backend = AuthBackend::from_settings(preview).await?;
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
    if username.is_empty() {
        bail!("Username cannot be empty");
    }

    let display_url = if username == "__token__" {
        url.without_credentials().to_string()
    } else {
        format!("{username}@{}", url.without_credentials())
    };

    let credentials = match &backend {
        AuthBackend::System(provider) => provider
            .fetch(url, Some(&username))
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to fetch credentials for {display_url}"))?,
        AuthBackend::TextStore(store, _lock) => store
            .get_credentials(url, Some(&username))
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

/// Refresh the authentication tokens in the [`PyxTokenStore`], prompting for login if necessary.
async fn pyx_refresh(store: &PyxTokenStore, client: &BaseClient, printer: Printer) -> Result<()> {
    // Retrieve the token store.
    let token = match store
        .access_token(client.for_host(store.api()).raw_client(), 0)
        .await
    {
        // If the tokens were successfully refreshed, return them.
        Ok(Some(token)) => token,

        // If the token store is empty, prompt for login.
        Ok(None) => {
            debug!("Token store is empty; prompting for login...");
            login::pyx_login_with_browser(store, client, &printer).await?
        }

        // Similarly, if the refresh token expired, prompt for login.
        Err(err) if err.is_unauthorized() => {
            if store.has_auth_token() {
                return Err(
                    anyhow::Error::from(err).context("Failed to authenticate with access token")
                );
            } else if store.has_api_key() {
                return Err(anyhow::Error::from(err).context("Failed to authenticate with API key"));
            }
            debug!(
                "Received 401 (Unauthorized) response from refresh endpoint; prompting for login..."
            );
            login::pyx_login_with_browser(store, client, &printer).await?
        }

        Err(err) => {
            return Err(err.into());
        }
    };

    writeln!(printer.stdout(), "{}", token.as_str())?;
    Ok(())
}
