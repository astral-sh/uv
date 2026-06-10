#![cfg(target_os = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use uv_auth::{AuthBackend, Credentials};
use uv_preview::{Preview, PreviewFeature};
use uv_redacted::DisplaySafeUrl;

#[tokio::test]
async fn native_store_enumerates_many_credentials_in_one_realm()
-> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let provider =
        match AuthBackend::from_settings(Preview::new(&[PreviewFeature::NativeAuth])).await? {
            AuthBackend::System(provider) => provider,
            AuthBackend::TextStore(..) => {
                return Err(std::io::Error::other("expected native authentication backend").into());
            }
        };
    let entries = (0..16)
        .map(|index| {
            let url = DisplaySafeUrl::parse(&format!(
                "https://native-auth-{unique}.example.invalid/credential-{index}"
            ))?;
            let credentials = Credentials::basic(
                Some(format!("user-{index}")),
                Some(format!("{index:02}{}", "x".repeat(1_000))),
            );
            Ok::<_, uv_redacted::DisplaySafeUrlError>((url, credentials))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let result = async {
        for (url, credentials) in &entries {
            assert!(provider.store(url, credentials).await?);
        }
        for (url, credentials) in &entries {
            assert_eq!(
                provider.fetch(url, credentials.username()).await?,
                Some(credentials.clone())
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    for (url, credentials) in &entries {
        if let Some(username) = credentials.username() {
            let _ = provider.remove(url, username).await;
        }
    }

    result?;
    Ok(())
}
