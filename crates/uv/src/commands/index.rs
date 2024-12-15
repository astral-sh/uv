use console::Term;
use uv_configuration::KeyringProviderType;
use uv_distribution_types::Index;
use uv_auth::{AuthConfig, ConfigFile};
use tracing::{debug, warn};
use anyhow::{Context, Result};

/// Add one or more packages to the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn add_credentials(name: String, username: Option<String>, keyring_provider: KeyringProviderType, index: Vec<Index>) -> Result<()> {
    let index_for_name = index.iter().find(
        |idx| idx.name.as_ref().map(|n| n.to_string() == name).unwrap_or(false)
    );

    let index_for_name = match index_for_name {
        Some(obj) => obj,
        None => panic!("No index found with the name '{}'", name),
    };
    
    
    let username = match username {
        Some(n) => n,
        None => match prompt_username_input()? {
            Some(n) => n,
            None => panic!("No username provided and could not read username from input.")
        },
    }; 

    let password = match prompt_password_input()? {
        Some(p) => p,
        None => panic!("Could not read password from user input")
    };

    let url = index_for_name.raw_url();
    debug!("Will store password for index {} with URL {} and user {} in keyring", name, url, username);
    keyring_provider.to_provider().expect("Keyring Provider is not available").set(&url, &username, &password).await;

    debug!("Will add index {} and user {} to index auth config in {:?}", name, url, AuthConfig::path()?);
    let mut auth_config = AuthConfig::load().inspect_err(|err| warn!("Could not load auth config due to: {err}"))?;
    auth_config.add_entry(name, username);
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