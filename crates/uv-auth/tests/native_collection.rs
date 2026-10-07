#![cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
))]

use std::assert_matches;
use std::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};

use uv_auth::{AuthBackend, Credentials};
use uv_preview::{MaybePreviewFeature, Preview, PreviewFeature};
use uv_redacted::DisplaySafeUrl;

/// Return the native provider used by the preview authentication backend.
async fn native_provider() -> Result<uv_auth::KeyringProvider, Box<dyn std::error::Error>> {
    let preview =
        Preview::from_feature_names(&[MaybePreviewFeature::Known(PreviewFeature::NativeAuth)]);
    match AuthBackend::from_settings(preview).await? {
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

#[tokio::test]
async fn native_store_migrates_overlapping_legacy_password_in_place()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let service = format!("http://{}", listener.local_addr()?);
    let root = DisplaySafeUrl::parse(&service)?;
    let request = DisplaySafeUrl::parse(&format!("{service}/first"))?;
    let legacy = uv_keyring::Entry::new(&format!("uv:{service}"), "uv")?;
    legacy.set_password("legacy-password").await?;
    let provider = native_provider().await?;

    let result = async {
        let expected = Some(Credentials::basic(
            Some("uv".to_string()),
            Some("legacy-password".to_string()),
        ));
        assert_eq!(provider.fetch(&request, Some("uv")).await?, expected);
        let collection: Vec<serde_json::Value> =
            serde_json::from_str(&legacy.get_password().await?)?;
        assert_eq!(collection.len(), 1);
        assert_eq!(collection[0]["service"], root.as_str());
        assert_eq!(collection[0]["username"], "uv");
        assert_eq!(collection[0]["password"], "legacy-password");
        assert_eq!(provider.fetch(&request, Some("uv")).await?, expected);

        // Logging out of a child service must not delete a realm-wide credential.
        assert!(provider.remove(&request, "uv").await.is_err());
        assert_eq!(provider.fetch(&root, Some("uv")).await?, expected);
        provider.remove(&root, "uv").await?;
        assert!(provider.fetch(&root, Some("uv")).await?.is_none());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let _ = legacy.delete_credential().await;
    result
}

#[tokio::test]
async fn native_store_keeps_overlapping_legacy_password_when_adding_an_account()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let service = format!("http://{}", listener.local_addr()?);
    let root = DisplaySafeUrl::parse(&service)?;
    let request = DisplaySafeUrl::parse(&format!("{service}/first"))?;
    let legacy = uv_keyring::Entry::new(&format!("uv:{service}"), "uv")?;
    legacy.set_password("{}").await?;
    let provider = native_provider().await?;

    let result = async {
        let added = Credentials::basic(Some("other".to_string()), Some("new-password".to_string()));
        provider.store(&request, &added).await?;
        assert_eq!(provider.fetch(&request, Some("other")).await?, Some(added));
        let expected = Some(Credentials::basic(
            Some("uv".to_string()),
            Some("{}".to_string()),
        ));
        assert_eq!(provider.fetch(&root, Some("uv")).await?, expected);
        provider.remove(&request, "other").await?;
        assert_eq!(provider.fetch(&root, Some("uv")).await?, expected);
        provider.remove(&root, "uv").await?;
        assert!(provider.fetch(&root, Some("uv")).await?.is_none());
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let _ = legacy.delete_credential().await;
    result
}

#[tokio::test]
async fn native_store_removes_overlapping_legacy_password() -> Result<(), Box<dyn std::error::Error>>
{
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let service = format!("http://{}", listener.local_addr()?);
    let root = DisplaySafeUrl::parse(&service)?;
    let legacy = uv_keyring::Entry::new(&format!("uv:{service}"), "uv")?;
    legacy.set_password("legacy-password").await?;
    let provider = native_provider().await?;

    let result = async {
        provider.remove(&root, "uv").await?;
        assert!(provider.fetch(&root, Some("uv")).await?.is_none());
        assert_matches!(legacy.get_password().await, Err(uv_keyring::Error::NoEntry));
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    let _ = legacy.delete_credential().await;
    result
}
