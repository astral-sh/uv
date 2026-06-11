#![cfg(target_os = "windows")]

use std::time::{SystemTime, UNIX_EPOCH};

use uv_auth::{AuthBackend, Credentials};
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

    let corrupt_url = DisplaySafeUrl::parse(&format!(
        "https://native-auth-{unique}.example.invalid/corrupt"
    ))?;
    let corrupt_credentials =
        Credentials::basic(Some("corrupt".to_string()), Some("password".to_string()));

    let result = async {
        for (url, credentials) in &entries {
            provider.store(url, credentials).await?;
        }
        provider.store(&corrupt_url, &corrupt_credentials).await?;

        let target_prefix = format!("uv:https://native-auth-{unique}.example.invalid:");
        let corrupt_credential = WinCredential::enumerate(&target_prefix)
            .await?
            .into_iter()
            .find(|credential| {
                serde_json::from_slice::<serde_json::Value>(credential.secret())
                    .ok()
                    .and_then(|value| {
                        value
                            .get("service")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .as_deref()
                    == Some(corrupt_url.as_str())
            })
            .ok_or_else(|| std::io::Error::other("failed to enumerate test credential"))?;
        let corrupt_entry = uv_keyring::Entry::new_with_credential(Box::new(
            corrupt_credential.credential().clone(),
        ));
        corrupt_entry.set_secret(b"not JSON").await?;

        for (url, credentials) in &entries {
            let actual = provider.fetch(url, credentials.username()).await?;
            if actual != Some(credentials.clone()) {
                return Err(std::io::Error::other(format!(
                    "unexpected credentials returned for {url}"
                ))
                .into());
            }
        }

        let (removed_url, removed_credentials) = &entries[0];
        let removed_username = removed_credentials
            .username()
            .ok_or_else(|| std::io::Error::other("test credential must have a username"))?;
        provider.remove(removed_url, removed_username).await?;
        if provider
            .fetch(removed_url, Some(removed_username))
            .await?
            .is_some()
        {
            return Err(std::io::Error::other("removed credential was still returned").into());
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    for (url, credentials) in &entries {
        if let Some(username) = credentials.username() {
            let _ = provider.remove(url, username).await;
        }
    }
    if let Some(username) = corrupt_credentials.username() {
        let _ = provider.remove(&corrupt_url, username).await;
    }

    result
}
