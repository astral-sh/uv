use anyhow::{Result, bail};

use console::Term;
use tracing::debug;
use uv_auth::Credentials;
use uv_configuration::KeyringProviderType;
use uv_redacted::DisplaySafeUrl;

use crate::commands::ExitStatus;

/// Set credentials for a service.
pub(crate) async fn set(
    service: Option<String>,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    keyring_provider: KeyringProviderType,
) -> Result<ExitStatus> {
    let Some(service) = service else {
        bail!(
            "`uv auth set` requires a service and username, e.g., `uv auth set https://example.com user`"
        );
    };
    let username = if let Some(username) = username {
        username
    } else if token.is_some() {
        String::from("__token__")
    } else {
        bail!(
            "`uv auth set` requires `--token` or a username, e.g., `uv auth set https://example.com user`"
        );
    };

    let Some(keyring_provider) = keyring_provider.to_provider() else {
        bail!("`--keyring-provider native` is required for system keyring credential configuration")
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
            bail!("`uv auth set` requires `--password` when not in a terminal.");
        }
    };

    let url = DisplaySafeUrl::parse(&service)?;
    let credentials = Credentials::basic(Some(username), Some(password));
    keyring_provider.store_if_native(&url, &credentials).await;

    Ok(ExitStatus::Success)
}

/// Unset credentials for a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn unset(
    service: Option<String>,
    username: Option<String>,
    keyring_provider: KeyringProviderType,
) -> Result<ExitStatus> {
    let Some(service) = service else {
        bail!(
            "`uv auth unset` requires a service and username, e.g., `uv auth set https://example.com user`"
        );
    };
    let username = if let Some(username) = username {
        username
    } else {
        debug!("No username provided. Using `__token__`");
        String::from("__token__")
    };

    let Some(keyring_provider) = keyring_provider.to_provider() else {
        bail!("`--keyring-provider native` is required for system keyring credential configuration")
    };

    let url = DisplaySafeUrl::parse(&service)?;
    if let Err(err) = keyring_provider.remove_if_native(&url, &username).await {
        bail!("Unable to remove credentials for {url}: {err}");
    }

    Ok(ExitStatus::Success)
}
