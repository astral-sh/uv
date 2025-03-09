use anyhow::{Context, Ok, Result};
use console::Term;
use owo_colors::OwoColorize;
use std::fmt::Write;
use tracing::{debug, warn};
use uv_auth::{AuthConfig, ConfigFile};
use uv_configuration::KeyringProviderType;
use uv_distribution_types::Index;

use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn set_credentials(
    name: String,
    username: Option<String>,
    password: Option<String>,
    keyring_provider: KeyringProviderType,
    indexes: Vec<Index>,
) -> Result<()> {
    let index = indexes.iter().find(|idx| {
        idx.name
            .as_ref()
            .map(|n| n.to_string() == name)
            .unwrap_or(false)
    });

    let Some(index) = index else {
        panic!("No index found with the name '{name}'")
    };

    let username = match username {
        Some(n) => n,
        None => match prompt_username_input()? {
            Some(n) => n,
            None => panic!("No username provided and could not read username from input."),
        },
    };

    let password = match password {
        Some(p) => p,
        None => match prompt_password_input()? {
            Some(p) => p,
            None => panic!("Could not read password from user input"),
        },
    };

    let url = index.raw_url();
    debug!("Will store password for index {name} with URL {url} and user {username} in keyring");
    keyring_provider
        .to_provider()
        .expect("Keyring Provider is not available")
        .set(url, &username, &password)
        .await;

    debug!(
        "Will add index {name} and user {username} to index auth config in {:?}",
        AuthConfig::path()?
    );
    let mut auth_config =
        AuthConfig::load().inspect_err(|err| warn!("Could not load auth config due to: {err}"))?;
    auth_config.add_entry(index.raw_url(), username);
    auth_config
        .store()
        .inspect_err(|err| warn!("Could not save auth config due to: {err}"))?;

    Ok(())
}

pub(crate) async fn list_credentials(
    keyring_provider_type: KeyringProviderType,
    indexes: Vec<Index>,
    printer: Printer,
) -> Result<()> {
    let auth_config =
        AuthConfig::load().inspect_err(|err| warn!("Could not load auth config due to: {err}"))?;

    let keyring_provider = keyring_provider_type
        .to_provider()
        .expect("Keyring Provider is not available");

    let num_indexes = indexes.len();
    debug!("Found {num_indexes} indexes");
    for index in indexes {
        let index_url = index.raw_url();

        if let Some(auth_index) = auth_config.find_entry(index_url) {
            let username = auth_index.username.clone();
            let password = keyring_provider.fetch(&index.url, &username).await;

            let index_name = index.name.expect("Index should have a name").to_string();
            let _ = match password {
                Some(_) => writeln!(
                    printer.stderr(),
                    "{} authenticates with username {}",
                    format!("Index: {index_name}").bold(),
                    username,
                ),
                None => writeln!(
                    printer.stderr(),
                    "{} has no credentials.",
                    format!("Index: {index_name}").bold()
                ),
            };
        } else {
            debug!("Could not find the index with url {index_url} in auth config");
        }
    }

    Ok(())
}

pub(crate) async fn unset_credentials(
    name: String,
    username: Option<String>,
    keyring_provider: KeyringProviderType,
    indexes: Vec<Index>,
) -> Result<()> {
    let index = indexes.iter().find(|idx| {
        idx.name
            .as_ref()
            .map(|n| n.to_string() == name)
            .unwrap_or(false)
    });

    let Some(index) = index else {
        panic!("No index found with the name '{name}'")
    };

    let username = match username {
        Some(n) => n,
        None => match prompt_username_input()? {
            Some(n) => n,
            None => panic!("No username provided and could not read username from input."),
        },
    };

    keyring_provider
        .to_provider()
        .expect("Keyring Provider is not available")
        .unset(&index.url, &username)
        .await;

    let mut auth_config =
        AuthConfig::load().inspect_err(|err| warn!("Could not load auth config due to: {err}"))?;

    auth_config.delete_entry(index.raw_url());
    auth_config.store()?;

    Ok(())
}

fn prompt_username_input() -> Result<Option<String>> {
    let term = Term::stderr();
    if !term.is_term() {
        return Ok(None);
    }
    let username_prompt = "Enter username: ";

    let username = uv_console::input(username_prompt, &term).context("Failed to read username")?;
    Ok(Some(username))
}

fn prompt_password_input() -> Result<Option<String>> {
    let term = Term::stderr();
    if !term.is_term() {
        return Ok(None);
    }
    let password_prompt = "Enter password: ";
    let password =
        uv_console::password(password_prompt, &term).context("Failed to read password")?;
    Ok(Some(password))
}
