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
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());

    let username = if let Some(username) = username {
        username
    } else if token.is_some() {
        String::from("__token__")
    } else {
        let term = Term::stderr();
        if term.is_term() {
            let prompt = "username: ";
            uv_console::username(prompt, &term)?
        } else {
            bail!("No username provided; did you mean to provide `--username` or `--token`?");
        }
    };

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

    // FIXME: It would be preferable to accept the value of --password or --token
    // from stdin, perhaps checking here for `-` as an indicator to read stdin. We
    // could then warn if the password is provided as a plaintext argument.
    let password = if let Some(password) = password {
        password
    } else if let Some(token) = token {
        token
    } else {
        let term = Term::stderr();
        if term.is_term() {
            let prompt = "password: ";
            uv_console::password(prompt, &term)?
        } else {
            bail!("No password provided; did you mean to provide `--password` or `--token`?");
        }
    };

    // TODO(zanieb): Add support for other authentication schemes here, e.g., `Credentials::Bearer`
    let credentials = Credentials::basic(Some(username), Some(password));
    provider.store(url, &credentials).await?;

    writeln!(printer.stderr(), "Logged in to {display_url}")?;

    Ok(ExitStatus::Success)
}
