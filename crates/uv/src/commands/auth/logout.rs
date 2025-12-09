use std::fmt::Write;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;

use uv_auth::{AuthBackend, Credentials, PyxTokenStore, Service, TextCredentialStore, Username};
use uv_client::BaseClientBuilder;
use uv_distribution_types::IndexUrl;
use uv_pep508::VerbatimUrl;
use uv_preview::Preview;

use crate::{commands::ExitStatus, printer::Printer};

/// Logout from a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn logout(
    service: Service,
    username: Option<String>,
    client_builder: BaseClientBuilder<'_>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let pyx_store = PyxTokenStore::from_settings()?;
    if pyx_store.is_known_domain(service.url()) {
        return pyx_logout(&pyx_store, client_builder, printer).await;
    }

    let backend = AuthBackend::from_settings(preview).await?;

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
    if username.is_empty() {
        bail!("Username cannot be empty");
    }

    let display_url = if username == "__token__" {
        url.without_credentials().to_string()
    } else {
        format!("{username}@{}", url.without_credentials())
    };

    // TODO(zanieb): Consider exhaustively logging out from all backends
    match backend {
        AuthBackend::System(provider) => {
            provider
                .remove(&url, &username)
                .await
                .with_context(|| format!("Unable to remove credentials for {display_url}"))?;
        }
        AuthBackend::TextStore(mut store, _lock) => {
            if store
                .remove(&service, Username::from(Some(username.clone())))
                .is_none()
            {
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

/// Log out via the [`PyxTokenStore`], invalidating the existing tokens.
async fn pyx_logout(
    store: &PyxTokenStore,
    client_builder: BaseClientBuilder<'_>,
    printer: Printer,
) -> Result<ExitStatus> {
    // Initialize the client.
    let client = client_builder.build();

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
