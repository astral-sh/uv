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
