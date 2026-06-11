use tracing::trace;
use zeroize::Zeroizing;

use super::{Error, NATIVE_SERVICE_PREFIX, RealmGuardRef, RealmWriteGuard, ensure_service_realm};
use crate::{Service, Username, persistent::PersistentCredential};

/// Fixed keyring username used for an aggregate realm entry.
const AGGREGATE_USERNAME: &str = "_uv_";

/// Return the keyring entry containing the aggregate for a realm.
fn realm_entry(guard: RealmGuardRef<'_>) -> Result<uv_keyring::Entry, Error> {
    uv_keyring::Entry::new(
        &format!("{NATIVE_SERVICE_PREFIX}{}", guard.realm()),
        AGGREGATE_USERNAME,
    )
    .map_err(Error::Keyring)
}

/// Decode an aggregate for a read operation.
fn decode_for_read(json: &str) -> Result<Vec<PersistentCredential>, Error> {
    serde_json::from_str(json).map_err(Error::CorruptStoredCredentials)
}

/// Insert or replace one exact service and username while preserving other credentials.
fn upsert_credential(
    credentials: &mut Vec<PersistentCredential>,
    credential: &PersistentCredential,
) {
    let username = credential.credentials.to_username();
    credentials.retain(|stored| {
        stored.service != credential.service || stored.credentials.to_username() != username
    });
    credentials.push(credential.clone());
}

/// Remove one exact service and username, returning whether a credential was removed.
fn remove_credential(
    credentials: &mut Vec<PersistentCredential>,
    service: &Service,
    username: &Username,
) -> bool {
    let initial = credentials.len();
    credentials.retain(|stored| {
        stored.service != *service || stored.credentials.to_username() != *username
    });
    credentials.len() != initial
}

/// Load all current-format credentials in the locked realm.
pub(super) async fn load_current(
    guard: RealmGuardRef<'_>,
) -> Result<Vec<PersistentCredential>, Error> {
    let entry = realm_entry(guard)?;
    match entry.get_password().await {
        Ok(json) => decode_for_read(&Zeroizing::new(json)),
        Err(uv_keyring::Error::NoEntry) => Ok(Vec::new()),
        Err(err) => Err(Error::Keyring(err)),
    }
}

/// Store one current-format credential while holding its realm write lock.
pub(super) async fn store_current(
    guard: &RealmWriteGuard,
    credential: &PersistentCredential,
) -> Result<(), Error> {
    ensure_service_realm(guard.realm(), &credential.service)?;
    let entry = realm_entry(RealmGuardRef::Write(guard))?;
    let mut credentials = match entry.get_password().await {
        Ok(json) => decode_for_read(&Zeroizing::new(json))?,
        Err(uv_keyring::Error::NoEntry) => Vec::new(),
        Err(err) => return Err(Error::Keyring(err)),
    };
    upsert_credential(&mut credentials, credential);
    let json = Zeroizing::new(
        serde_json::to_string(&credentials).map_err(Error::SerializeStoredCredentials)?,
    );
    entry.set_password(&json).await?;
    trace!("Stored native credentials for realm {}", guard.realm);
    Ok(())
}

/// Remove one current-format credential while holding its realm write lock.
pub(super) async fn remove_current(
    guard: &RealmWriteGuard,
    service: &Service,
    username: &Username,
) -> Result<bool, Error> {
    ensure_service_realm(guard.realm(), service)?;
    let entry = realm_entry(RealmGuardRef::Write(guard))?;
    let json = match entry.get_password().await {
        Ok(json) => json,
        Err(uv_keyring::Error::NoEntry) => return Ok(false),
        Err(err) => return Err(Error::Keyring(err)),
    };
    let mut credentials = decode_for_read(&Zeroizing::new(json))?;
    if !remove_credential(&mut credentials, service, username) {
        return Ok(false);
    }
    if credentials.is_empty() {
        entry.delete_credential().await?;
    } else {
        let json = Zeroizing::new(
            serde_json::to_string(&credentials).map_err(Error::SerializeStoredCredentials)?,
        );
        entry.set_password(&json).await?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{decode_for_read, remove_credential, upsert_credential};
    use crate::{Credentials, Service, Username, persistent::PersistentCredential};

    fn credential(service: &str, username: &str, password: &str) -> PersistentCredential {
        PersistentCredential {
            service: Service::from_str(service).expect("service URL should be valid"),
            credentials: Credentials::basic(Some(username.to_string()), Some(password.to_string())),
        }
    }

    #[test]
    fn corrupt_aggregate_is_rejected_without_data_loss() {
        assert!(decode_for_read("not JSON").is_err());
    }

    #[test]
    fn upsert_replaces_only_the_exact_service_and_username() {
        let first = credential("https://example.com/first", "user", "old");
        let sibling_service = credential("https://example.com/second", "user", "sibling");
        let sibling_username = credential("https://example.com/first", "other", "other");
        let replacement = credential("https://example.com/first", "user", "new");
        let mut credentials = vec![first, sibling_service.clone(), sibling_username.clone()];

        upsert_credential(&mut credentials, &replacement);

        assert_eq!(credentials.len(), 3);
        assert!(credentials.iter().any(|stored| {
            stored.service == replacement.service && stored.credentials.password() == Some("new")
        }));
        assert!(credentials.iter().any(|stored| {
            stored.service == sibling_service.service
                && stored.credentials.password() == Some("sibling")
        }));
        assert!(credentials.iter().any(|stored| {
            stored.service == sibling_username.service
                && stored.credentials.username() == Some("other")
        }));
    }

    #[test]
    fn remove_deletes_only_the_exact_service_and_username() {
        let removed = credential("https://example.com/first", "user", "removed");
        let retained = credential("https://example.com/second", "user", "retained");
        let mut credentials = vec![removed.clone(), retained.clone()];

        assert!(remove_credential(
            &mut credentials,
            &removed.service,
            &Username::from("user".to_string()),
        ));
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].service, retained.service);
    }
}
