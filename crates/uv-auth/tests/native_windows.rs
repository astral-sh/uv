#![cfg(target_os = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use uv_auth::{AuthBackend, Credentials, Realm};
use uv_keyring::windows::WinCredential;
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
    let mut entries = (0..16)
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

    let case_collision_url = DisplaySafeUrl::parse(&format!(
        "https://native-auth-{unique}.example.invalid/case-collision"
    ))?;
    entries.push((
        case_collision_url.clone(),
        Credentials::basic(Some("aa".to_string()), Some("first".to_string())),
    ));
    entries.push((
        case_collision_url,
        Credentials::basic(Some("aG".to_string()), Some("second".to_string())),
    ));

    entries.push((
        DisplaySafeUrl::parse(&format!(
            "https://native-auth-{unique}.example.invalid/signed?X-Amz-Signature=one"
        ))?,
        Credentials::basic(Some("signed".to_string()), Some("first".to_string())),
    ));
    entries.push((
        DisplaySafeUrl::parse(&format!(
            "https://native-auth-{unique}.example.invalid/signed?X-Amz-Signature=two"
        ))?,
        Credentials::basic(Some("signed".to_string()), Some("second".to_string())),
    ));

    let realm = Realm::from(&*entries[0].0);
    let malformed = uv_keyring::Entry::new_with_credential(Box::new(WinCredential {
        username: "malformed".to_string(),
        target_name: format!("uv:native-auth:v2:{realm}:{}", "0".repeat(64)),
        target_alias: String::new(),
        comment: "uv native authentication credential".to_string(),
    }));

    let result = async {
        for (url, credentials) in &entries {
            assert!(provider.store(url, credentials).await?);
        }
        malformed.set_secret(b"not JSON").await?;
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
    let _ = malformed.delete_credential().await;

    result?;
    Ok(())
}
