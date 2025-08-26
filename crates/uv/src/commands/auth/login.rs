use anyhow::{Result, bail};

use console::Term;
use tracing::debug;
use uv_auth::{Credentials, KeyringProvider};
use uv_configuration::KeyringProviderType;
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;

/// Login to a service.
pub(crate) async fn login(
    service: String,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    keyring_provider: KeyringProviderType,
) -> Result<ExitStatus> {
    let username = if let Some(username) = username {
        username
    } else if token.is_some() {
        String::from("__token__")
    } else {
        bail!(
            "`uv auth login` requires either a `--token` or a username, e.g., `uv auth login https://example.com user`"
        );
    };

    // Be helpful about incompatible `keyring-provider` settings
    if let Some(provider) = &keyring_provider {
        match provider {
            KeyringProviderType::Native => {}
            KeyringProviderType::Disabled => {
                bail!("Cannot login with `keyring-provider = disabled`");
            }
            KeyringProviderType::Subprocess => {
                warn_user_once!(
                    "Login is not supported with `keyring-provider = subprocess`, the `native` provider will be used instead"
                );
            }
        }
    }

    // ALways use the native keyring provider
    let provider = KeyringProvider::native();

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
            bail!("`uv auth login` requires `--password` when not in a terminal.");
        }
    };

    let url = DisplaySafeUrl::parse(&service)?;
    let credentials = Credentials::basic(Some(username), Some(password));
    provider.store(&url, &credentials).await;

    Ok(ExitStatus::Success)
}
