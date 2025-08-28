use anyhow::{Result, bail};
use std::fmt::Write;

use console::Term;
use uv_auth::Credentials;
use uv_configuration::{KeyringProviderType, Service};

use crate::{commands::ExitStatus, printer::Printer};

/// Login to a service.
pub(crate) async fn login(
    service: Service,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    let url = service.url();

    // Be helpful about incompatible `keyring-provider` settings
    let Some(keyring_provider) = &keyring_provider else {
        bail!(
            "Logging in requires setting `keyring-provider = {}` for credentials to be retrieved in subsequent commands",
            KeyringProviderType::Native
        );
    };
    let provider = match keyring_provider {
        KeyringProviderType::Native => keyring_provider.to_provider().unwrap(),
        KeyringProviderType::Disabled | KeyringProviderType::Subprocess => {
            bail!(
                "Cannot login with `keyring-provider = {keyring_provider}`, use `keyring-provider = {}` instead",
                KeyringProviderType::Native
            );
        }
    };

    // Extract credentials from URL if present
    let url_credentials = Credentials::from_url(url);
    let url_username = url_credentials.as_ref().and_then(|c| c.username());
    let url_password = url_credentials.as_ref().and_then(|c| c.password());

    let username = match (username, url_username) {
        (Some(cli), Some(url)) => {
            bail!(
                "Cannot specify a username both via the URL and CLI; found `--username {cli}` and `{url}`"
            );
        }
        (Some(cli), None) => Some(cli),
        (None, Some(url)) => Some(url.to_string()),
        (None, None) => {
            // When using `--token`, we'll use a `__token__` placeholder username
            if token.is_some() {
                Some("__token__".to_string())
            } else {
                None
            }
        }
    };

    // Ensure that a username is not provided when using a token
    if token.is_some() {
        if let Some(username) = &username {
            if username != "__token__" {
                bail!("When using `--token`, a username cannot not be provided; found: {username}");
            }
        }
    }

    // Prompt for a username if not provided
    let username = if let Some(username) = username {
        username
    } else {
        let term = Term::stderr();
        if term.is_term() {
            let prompt = "username: ";
            uv_console::username(prompt, &term)?
        } else {
            bail!("No username provided; did you mean to provide `--username` or `--token`?");
        }
    };

    let password = match (password, url_password, token) {
        (Some(_), Some(_), _) => {
            bail!("Cannot specify a password both via the URL and CLI");
        }
        (Some(_), None, Some(_)) => {
            bail!("Cannot specify a password via `--password` when using `--token`");
        }
        (None, Some(_), Some(_)) => {
            bail!("Cannot include a password in the URL when using `--token`")
        }
        (Some(cli), None, None) => cli,
        (None, Some(url), None) => url.to_string(),
        (None, None, Some(token)) => token,
        (None, None, None) => {
            let term = Term::stderr();
            if term.is_term() {
                let prompt = "password: ";
                uv_console::password(prompt, &term)?
            } else {
                bail!("No password provided; did you mean to provide `--password` or `--token`?");
            }
        }
    };

    let display_url = if username == "__token__" {
        url.without_credentials().to_string()
    } else {
        format!("{username}@{}", url.without_credentials())
    };

    // TODO(zanieb): Add support for other authentication schemes here, e.g., `Credentials::Bearer`
    let credentials = Credentials::basic(Some(username), Some(password));
    provider.store(url, &credentials).await?;

    writeln!(printer.stderr(), "Logged in to {display_url}")?;
    Ok(ExitStatus::Success)
}
