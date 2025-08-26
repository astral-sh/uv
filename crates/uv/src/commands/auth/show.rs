use std::fmt::Write;

use anyhow::{Context, Result, bail};

use uv_configuration::KeyringProviderType;
use uv_redacted::DisplaySafeUrl;

use crate::{Printer, commands::ExitStatus};

/// Show credentials for a service.
///
/// If no username is provided, defaults to `__token__`.
pub(crate) async fn show(
    service: String,
    username: Option<String>,
    keyring_provider: Option<KeyringProviderType>,
    printer: Printer,
) -> Result<ExitStatus> {
    let url = DisplaySafeUrl::parse(&service)?;

    let Some(keyring_provider) = keyring_provider.and_then(|p| p.to_provider()) else {
        bail!(
            "A keyring provider is required to retrieve credentials, e.g., use `--keyring-provider native`"
        )
    };

    let credentials = keyring_provider
        .fetch(&url, username.as_deref())
        .await
        .with_context(|| format!("Failed to fetch credentials for {url}"))?;

    if let Some(password) = credentials.password() {
        writeln!(printer.stdout(), "{password}")?;
    } else {
        bail!("No password found in credentials");
    }

    Ok(ExitStatus::Success)
}
