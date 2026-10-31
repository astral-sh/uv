use std::fmt::Write;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;
use uv_redacted::SafeUrl;

use uv_auth::{AuthBackend, Credentials, LookupError, Service, TextCredentialStore, Username};
use uv_command_support::{ExitStatus, Printer};
use uv_distribution_types::IndexUrl;
use uv_pep508::VerbatimUrl;
use uv_preview::Preview;

/// Logout from a service.
///
/// If no username is provided, uv removes token credentials first, then username and password
/// credentials if only one matching entry exists.
pub(crate) async fn logout(
    service: Service,
    username: Option<String>,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let backend = AuthBackend::from_settings(preview).await?;

    // TODO(zanieb): Use a shared abstraction across `login` and `logout`?
    let url = service.url().clone();
    let (service, url) = match IndexUrl::from(VerbatimUrl::from_url(url.clone())).root() {
        Some(root) => (Service::try_from(root.clone())?, root),
        None => (service, url),
    };

    // Extract credentials from URL if present
    let url_credentials = Credentials::from_url(&url)?;
    let url_username = url_credentials.as_ref().and_then(|c| c.username());

    let username = match (username, url_username) {
        (Some(cli), Some(url)) => {
            bail!(
                "Cannot specify a username both via the URL and CLI; found `--username {cli}` and `{url}`"
            );
        }
        (Some(cli), None) => Some(cli),
        (None, Some(url)) => Some(url.to_string()),
        (None, None) => None,
    };
    if username.as_ref().is_some_and(String::is_empty) {
        bail!("Username cannot be empty");
    }

    let url_without_credentials = url.without_credentials();

    // TODO(zanieb): Consider exhaustively logging out from all backends
    let display_url = match backend {
        AuthBackend::System(provider) => {
            if let Some(username) = username.as_deref() {
                let display_url = format_display_url(&url_without_credentials, Some(username));
                provider
                    .remove(&url, username)
                    .await
                    .with_context(|| format!("Unable to remove credentials for `{display_url}`"))?;
                display_url
            } else if provider.fetch(&url, Some("__token__")).await.is_some() {
                provider.remove(&url, "__token__").await.with_context(|| {
                    format!("Unable to remove credentials for {url_without_credentials}")
                })?;
                url_without_credentials.to_string()
            } else {
                bail!(
                    "{}",
                    missing_token_credentials_hint(&url_without_credentials)
                );
            }
        }
        AuthBackend::TextStore(mut store, _lock) => {
            let display_url = if let Some(username) = username {
                let display_url = format_display_url(&url_without_credentials, Some(&username));
                if store
                    .remove(service.clone(), Username::from(Some(username)))
                    .is_none()
                {
                    bail!("No matching entry found for {display_url}");
                }
                display_url
            } else if store
                .remove(
                    service.clone(),
                    Username::from(Some("__token__".to_string())),
                )
                .is_some()
            {
                url_without_credentials.to_string()
            } else {
                let lookup = store.get_credential_entry(&url, None).map(|entry| {
                    entry.map(|(matched_service, credentials)| {
                        (
                            matched_service.into_owned(),
                            credentials.username().map(ToString::to_string),
                        )
                    })
                });
                match lookup {
                    Ok(Some((matched_service, matched_username))) => {
                        let display_url = format_display_url(
                            &url_without_credentials,
                            matched_username.as_deref(),
                        );
                        if store
                            .remove(matched_service, Username::from(matched_username))
                            .is_none()
                        {
                            bail!("No matching entry found for `{display_url}`");
                        }
                        display_url
                    }
                    Ok(None) => bail!("{}", missing_credentials_hint(&url_without_credentials)),
                    Err(LookupError::AmbiguousUsername(_, usernames)) => {
                        bail!(
                            "{}",
                            ambiguous_username_hint(&url_without_credentials, &usernames)
                        )
                    }
                }
            };
            store
                .write(TextCredentialStore::default_file()?, _lock)
                .with_context(|| "Failed to persist changes to credentials after removal")?;
            display_url
        }
    };

    writeln!(
        printer.stderr(),
        "Removed credentials for {}",
        display_url.bold().cyan()
    )?;

    Ok(ExitStatus::Success)
}

/// Format a URL for display, including the username if it's present (and not the default token
/// entry).
fn format_display_url(url: &SafeUrl, username: Option<&str>) -> String {
    if let Some(username) = username.filter(|username| *username != "__token__") {
        format!("{username}@{url}")
    } else {
        url.to_string()
    }
}

fn missing_credentials_hint(display_url: &SafeUrl) -> String {
    format!("No matching credentials found for {display_url}")
}

fn missing_token_credentials_hint(display_url: &SafeUrl) -> String {
    format!(
        "No token credentials found for {display_url}. To remove username and password credentials, pass `--username`."
    )
}

fn ambiguous_username_hint(display_url: &SafeUrl, usernames: &[String]) -> String {
    if usernames.is_empty() {
        return format!(
            "Multiple credentials found for {display_url}. Pass `--username` to `uv auth logout` to select which credentials to remove."
        );
    }

    format!(
        "Multiple credentials found for {display_url}. Pass `--username` to `uv auth logout` to select which credentials to remove: {}",
        usernames.join(", ")
    )
}
