use sha2::{Digest, Sha256};
use tracing::warn;
use zeroize::Zeroizing;

use super::{Error, NATIVE_SERVICE_PREFIX, RealmGuardRef, RealmWriteGuard, ensure_service_realm};
use crate::{Realm, Service, Username, persistent::PersistentCredential};

/// Description attached to uv credentials in Windows Credential Manager.
const CREDENTIAL_COMMENT: &str = "uv native authentication credential";

/// Return the target prefix used to enumerate credentials in a realm.
fn target_prefix(realm: &Realm) -> String {
    format!("{NATIVE_SERVICE_PREFIX}{realm}:")
}

/// Return a collision-resistant target for one service and username.
fn target(realm: &Realm, service: &Service, username: &Username) -> String {
    let service_url = service.url().as_str();
    let username = username.as_deref().unwrap_or_default();
    let identity = format!("{}:{service_url}{username}", service_url.len());
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    format!("{}{digest}", target_prefix(realm))
}

/// Return whether an enumerated target has the expected prefix and digest shape.
fn target_has_valid_shape(target_name: &str, prefix: &str) -> bool {
    let Some(stored_prefix) = target_name.get(..prefix.len()) else {
        return false;
    };
    let Some(digest) = target_name.get(prefix.len()..) else {
        return false;
    };
    stored_prefix.eq_ignore_ascii_case(prefix)
        && digest.len() == 64
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Deserialize one credential returned by Windows Credential Manager.
fn decode_credential(secret: &[u8]) -> Result<PersistentCredential, serde_json::Error> {
    serde_json::from_slice(secret)
}

/// Return whether a credential is stored under its canonical target in a realm.
fn credential_matches_target(
    realm: &Realm,
    target_name: &str,
    credential: &PersistentCredential,
) -> bool {
    Realm::from(credential.service.url()) == *realm
        && target_name.eq_ignore_ascii_case(&target(
            realm,
            &credential.service,
            &credential.credentials.to_username(),
        ))
}

/// Return a Windows keyring entry for one service and username.
fn entry(
    guard: &RealmWriteGuard,
    service: &Service,
    username: &Username,
) -> Result<uv_keyring::Entry, Error> {
    ensure_service_realm(guard.realm(), service)?;
    Ok(uv_keyring::Entry::new_with_credential(Box::new(
        uv_keyring::windows::WinCredential {
            username: String::new(),
            target_name: target(guard.realm(), service, username),
            target_alias: String::new(),
            comment: CREDENTIAL_COMMENT.to_string(),
        },
    )))
}

/// Load all current-format credentials in the locked realm.
pub(super) async fn load_current(
    guard: RealmGuardRef<'_>,
) -> Result<Vec<PersistentCredential>, Error> {
    let realm = guard.realm();
    let prefix = target_prefix(realm);
    let entries = uv_keyring::windows::WinCredential::enumerate(&prefix).await?;
    let mut credentials = Vec::with_capacity(entries.len());
    for enumerated in entries {
        let target_name = &enumerated.credential().target_name;
        if !target_has_valid_shape(target_name, &prefix) {
            warn!("Ignoring native credential with an invalid target in realm {realm}");
            continue;
        }
        let credential = match decode_credential(enumerated.secret()) {
            Ok(credential) => credential,
            Err(err) => {
                warn!("Ignoring corrupt native credential in realm {realm}: {err}");
                continue;
            }
        };
        if !credential_matches_target(realm, target_name, &credential) {
            warn!("Ignoring native credential stored under the wrong target");
            continue;
        }
        credentials.push(credential);
    }
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{credential_matches_target, decode_credential, target, target_has_valid_shape};
    use crate::{Credentials, Realm, Service, Username, persistent::PersistentCredential};

    #[test]
    fn target_identity_distinguishes_case_collisions_and_signed_urls() {
        let service =
            Service::from_str("https://example.com/path").expect("service URL should be valid");
        let realm = Realm::from(service.url());
        assert_ne!(
            target(&realm, &service, &Username::from("aa".to_string())),
            target(&realm, &service, &Username::from("aG".to_string()))
        );

        let first = Service::from_str("https://example.com/path?X-Amz-Signature=one")
            .expect("first signed URL should be valid");
        let second = Service::from_str("https://example.com/path?X-Amz-Signature=two")
            .expect("second signed URL should be valid");
        let username = Username::from("signed".to_string());
        assert_ne!(
            target(&realm, &first, &username),
            target(&realm, &second, &username)
        );
    }

    #[test]
    fn target_validation_rejects_corrupt_and_misplaced_credentials() {
        let service =
            Service::from_str("https://example.com/path").expect("service URL should be valid");
        let realm = Realm::from(service.url());
        let username = Username::from("user".to_string());
        let target = target(&realm, &service, &username);
        let prefix_length = target.len() - 64;
        assert!(target_has_valid_shape(&target, &target[..prefix_length]));
        assert!(!target_has_valid_shape(
            &format!("{}not-a-digest", &target[..prefix_length]),
            &target[..prefix_length]
        ));
        assert!(decode_credential(b"not JSON").is_err());

        let credential = PersistentCredential {
            service,
            credentials: Credentials::basic(Some("user".to_string()), Some("password".to_string())),
        };
        assert!(credential_matches_target(&realm, &target, &credential));
        assert!(!credential_matches_target(
            &realm,
            &format!("{}{}", &target[..prefix_length], "0".repeat(64)),
            &credential
        ));
    }
}

/// Store one current-format credential while holding its realm write lock.
pub(super) async fn store_current(
    guard: &RealmWriteGuard,
    credential: &PersistentCredential,
) -> Result<(), Error> {
    let entry = entry(
        guard,
        &credential.service,
        &credential.credentials.to_username(),
    )?;
    let json =
        Zeroizing::new(serde_json::to_vec(credential).map_err(Error::SerializeStoredCredentials)?);
    entry.set_secret(&json).await?;
    Ok(())
}

/// Remove one current-format credential while holding its realm write lock.
pub(super) async fn remove_current(
    guard: &RealmWriteGuard,
    service: &Service,
    username: &Username,
) -> Result<bool, Error> {
    match entry(guard, service, username)?.delete_credential().await {
        Ok(()) => Ok(true),
        Err(uv_keyring::Error::NoEntry) => Ok(false),
        Err(err) => Err(Error::Keyring(err)),
    }
}
