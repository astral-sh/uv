#![cfg(target_os = "macos")]

use std::time::{SystemTime, UNIX_EPOCH};

use uv_auth::{AuthBackend, Credentials};
use uv_preview::{Preview, PreviewFeature};
use uv_redacted::DisplaySafeUrl;

/// Return the native provider used by the preview authentication backend.
async fn native_provider() -> Result<uv_auth::KeyringProvider, Box<dyn std::error::Error>> {
    match AuthBackend::from_settings(Preview::new(&[PreviewFeature::NativeAuth])).await? {
        AuthBackend::System(provider) => Ok(provider),
        AuthBackend::TextStore(..) => {
            Err(std::io::Error::other("expected native authentication backend").into())
        }
    }
}

#[tokio::test]
async fn native_store_migrates_https_legacy_host_credentials()
-> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let host = format!("native-legacy-{unique}.example.invalid");
    let username = "legacy-user";
    let password = "legacy-password";
    let legacy = uv_keyring::Entry::new(&format!("uv:{host}"), username)?;
    legacy.set_password(password).await?;
    let provider = native_provider().await?;
    let request = DisplaySafeUrl::parse(&format!("https://{host}/first"))?;
    let migrated = DisplaySafeUrl::parse(&format!("https://{host}/"))?;

    let result = async {
        let expected = Some(Credentials::basic(
            Some(username.to_string()),
            Some(password.to_string()),
        ));
        if provider.fetch(&request, Some(username)).await? != expected {
            return Err(std::io::Error::other("legacy credential was not returned").into());
        }
        if !matches!(legacy.get_password().await, Err(uv_keyring::Error::NoEntry)) {
            return Err(std::io::Error::other("legacy credential was not removed").into());
        }
        if provider.fetch(&migrated, Some(username)).await?.is_none() {
            return Err(std::io::Error::other("migrated credential was not stored").into());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let _ = provider.remove(&migrated, username).await;
    let _ = legacy.delete_credential().await;
    result
}

#[tokio::test]
async fn native_store_does_not_migrate_http_legacy_host_credentials()
-> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let port = 10_000 + (unique % 50_000) as u16;
    let service = format!("localhost:{port}");
    let username = "legacy-user";
    let password = "legacy-password";
    let legacy = uv_keyring::Entry::new(&format!("uv:{service}"), username)?;
    legacy.set_password(password).await?;
    let provider = native_provider().await?;
    let request = DisplaySafeUrl::parse(&format!("http://{service}/first"))?;

    let result = async {
        if provider.fetch(&request, Some(username)).await?.is_none() {
            return Err(std::io::Error::other("legacy credential was not returned").into());
        }
        if legacy.get_password().await? != password {
            return Err(std::io::Error::other("legacy credential changed unexpectedly").into());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let _ = legacy.delete_credential().await;
    result
}
