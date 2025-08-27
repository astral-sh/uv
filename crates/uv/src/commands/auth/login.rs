use std::fmt::Write;

use anyhow::{Result, bail};
use console::Term;
use owo_colors::OwoColorize;
use url::Url;
use uuid::Uuid;

use uv_auth::{AccessToken, Credentials, OAuthTokens, PyxTokenStore, Tokens};
use uv_client::{AuthIntegration, BaseClient, BaseClientBuilder};
use uv_configuration::{KeyringProviderType, Service};
use uv_redacted::DisplaySafeUrl;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Login to a service.
pub(crate) async fn login(
    service: Service,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    network_settings: &NetworkSettings,
    printer: Printer,
) -> Result<ExitStatus> {
    let url = service.url();
    let display_url = username
        .as_ref()
        .map(|username| format!("{username}@{url}"))
        .unwrap_or_else(|| url.to_string());

    if is_pyx_url(url) {
        let store = PyxTokenStore::from_settings()?;
        let client = BaseClientBuilder::default()
            .connectivity(network_settings.connectivity)
            .native_tls(network_settings.native_tls)
            .allow_insecure_host(network_settings.allow_insecure_host.clone())
            .auth_integration(AuthIntegration::NoAuthMiddleware)
            .build();

        pyx_login_with_browser(&store, &client, &printer).await?;
        writeln!(printer.stderr(), "Logged in to {display_url}")?;
        return Ok(ExitStatus::Success);
    }

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

    let credentials = Credentials::basic(Some(username), Some(password));
    provider.store(url, &credentials).await?;

    writeln!(printer.stderr(), "Logged in to {display_url}")?;

    Ok(ExitStatus::Success)
}

pub(crate) fn is_pyx_url(url: &DisplaySafeUrl) -> bool {
    let Some(domain) = url.domain() else {
        return false;
    };
    domain == "pyx.dev" || domain.ends_with(".pyx.dev")
}

async fn pyx_login_with_browser(
    store: &PyxTokenStore,
    client: &BaseClient,
    printer: &Printer,
) -> anyhow::Result<AccessToken> {
    // Generate a login code, like `67e55044-10b1-426f-9247-bb680e5fe0c8`.
    let cli_token = Uuid::new_v4();
    let url = {
        let mut url = store.api().clone();
        url.set_path(&format!("auth/cli/login/{cli_token}"));
        url
    };
    match open::that(url.as_ref()) {
        Ok(()) => {
            writeln!(printer.stderr(), "Logging in with {}", url.cyan().bold())?;
        }
        Err(..) => {
            writeln!(
                printer.stderr(),
                "Open the following URL in your browser: {}",
                url.cyan().bold()
            )?;
        }
    }

    // Poll the server for the login code.
    let url = {
        let mut url = store.api().clone();
        url.set_path(&format!("auth/cli/status/{cli_token}"));
        url
    };

    let credentials = loop {
        let response = client
            .for_host(store.api())
            .get(Url::from(url.clone()))
            .send()
            .await?;
        match response.status() {
            // Retry on 404.
            reqwest::StatusCode::NOT_FOUND => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            // Parse the credentials on success.
            _ if response.status().is_success() => {
                let credentials = response.json::<OAuthTokens>().await?;
                break Ok::<Tokens, anyhow::Error>(Tokens::OAuth(credentials));
            }
            // Fail on any other status code (like a 500).
            status => {
                break Err(anyhow::anyhow!("Failed to login with code `{status}`"));
            }
        }
    }?;

    store.write(&credentials).await?;

    Ok(AccessToken::from(credentials))
}
