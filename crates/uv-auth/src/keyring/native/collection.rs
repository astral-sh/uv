use tracing::trace;
use zeroize::Zeroizing;

use super::{
    Error, PersistedCredentials, RealmGuardRef, RealmWriteGuard, SERVICE_PREFIX,
    ensure_service_realm,
};
use crate::{Realm, Service, Username, persistent::PersistentCredential};

/// Username for the single JSON entry that stores every credential in a realm.
const PERSISTED_CREDENTIALS_USERNAME: &str = "uv";

impl PersistedCredentials {
    /// Return whether the collection is empty.
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Insert or replace one exact service and username.
    fn upsert(&mut self, credential: &PersistentCredential) {
        let username = credential.credentials.to_username();
        self.0.retain(|stored| {
            stored.service != credential.service || stored.credentials.to_username() != username
        });
        self.0.push(credential.clone());
    }

    /// Remove one exact service and username.
    fn remove(&mut self, service: &Service, username: &Username) -> bool {
        let initial = self.0.len();
        self.0.retain(|stored| {
            stored.service != *service || stored.credentials.to_username() != *username
        });
        self.0.len() != initial
    }
}

/// Return whether a legacy key identifies the persisted realm entry.
pub(super) fn is_persisted_entry(realm: &Realm, service_name: &str, username: &str) -> bool {
    username == PERSISTED_CREDENTIALS_USERNAME && service_name == realm.to_string()
}

/// Return the keyring entry containing all persisted credentials for a realm.
fn realm_entry(guard: RealmGuardRef<'_>) -> Result<uv_keyring::Entry, Error> {
    uv_keyring::Entry::new(
        &format!("{SERVICE_PREFIX}{}", guard.realm()),
        PERSISTED_CREDENTIALS_USERNAME,
    )
    .map_err(Error::Keyring)
}

/// Decode the JSON entry containing persisted credentials.
fn decode_persisted_credentials(value: &str) -> Result<PersistedCredentials, Error> {
    serde_json::from_str(value).map_err(Error::CorruptStoredCredentials)
}

/// Load the persisted credentials in the locked realm.
pub(super) async fn load_persisted_credentials(
    guard: RealmGuardRef<'_>,
) -> Result<PersistedCredentials, Error> {
    let entry = realm_entry(guard)?;
    match entry.get_password().await {
        Ok(value) => decode_persisted_credentials(&Zeroizing::new(value)),
        Err(uv_keyring::Error::NoEntry) => Ok(PersistedCredentials::default()),
        Err(err) => Err(Error::Keyring(err)),
    }
}

/// Store one credential while holding its realm write lock.
pub(super) async fn store_persisted_credential(
    guard: &RealmWriteGuard,
    credential: &PersistentCredential,
) -> Result<(), Error> {
    ensure_service_realm(guard.realm(), &credential.service)?;
    let entry = realm_entry(RealmGuardRef::Write(guard))?;
    let mut credentials = match entry.get_password().await {
        Ok(value) => decode_persisted_credentials(&Zeroizing::new(value))?,
        Err(uv_keyring::Error::NoEntry) => PersistedCredentials::default(),
        Err(err) => return Err(Error::Keyring(err)),
    };
    credentials.upsert(credential);
    let json = Zeroizing::new(
        serde_json::to_string(&credentials).map_err(Error::SerializeStoredCredentials)?,
    );
    entry.set_password(&json).await?;
    trace!("Stored native credentials for realm {}", guard.realm);
    Ok(())
}

/// Remove one persisted credential while holding its realm write lock.
pub(super) async fn remove_persisted_credential(
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
    let mut credentials = decode_persisted_credentials(&Zeroizing::new(json))?;
    if !credentials.remove(service, username) {
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

    use super::{
        PERSISTED_CREDENTIALS_USERNAME, PersistedCredentials, decode_persisted_credentials,
        is_persisted_entry,
    };
    use crate::{Credentials, Realm, Service, Username, persistent::PersistentCredential};

    fn credential(service: &str, username: &str, password: &str) -> PersistentCredential {
        PersistentCredential {
            service: Service::from_str(service).expect("service URL should be valid"),
            credentials: Credentials::basic(Some(username.to_string()), Some(password.to_string())),
        }
    }

    #[test]
    fn corrupt_persisted_credentials_are_rejected() {
        assert!(decode_persisted_credentials("not JSON").is_err());
        assert!(decode_persisted_credentials("{}").is_err());
    }

    #[test]
    fn http_scheme_host_persisted_entry_is_not_treated_as_legacy() {
        let url = Service::from_str("http://localhost:1234")
            .expect("localhost service URL should be valid");
        let realm = Realm::from(url.url());
        assert!(is_persisted_entry(
            &realm,
            "http://localhost:1234",
            PERSISTED_CREDENTIALS_USERNAME
        ));
        assert!(!is_persisted_entry(
            &realm,
            "localhost:1234",
            PERSISTED_CREDENTIALS_USERNAME
        ));
        assert!(!is_persisted_entry(&realm, "http://localhost:1234", "user"));
    }

    #[test]
    fn upsert_replaces_only_the_exact_service_and_username() {
        let first = credential("https://example.com/first", "user", "old");
        let sibling_service = credential("https://example.com/second", "user", "sibling");
        let sibling_username = credential("https://example.com/first", "other", "other");
        let replacement = credential("https://example.com/first", "user", "new");
        let mut credentials = PersistedCredentials(vec![
            first,
            sibling_service.clone(),
            sibling_username.clone(),
        ]);

        credentials.upsert(&replacement);

        assert_eq!(credentials.iter().count(), 3);
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
        let mut credentials = PersistedCredentials(vec![removed.clone(), retained.clone()]);

        assert!(credentials.remove(&removed.service, &Username::from("user".to_string()),));
        let remaining = credentials
            .iter()
            .next()
            .expect("one credential should remain");
        assert_eq!(credentials.iter().count(), 1);
        assert_eq!(remaining.service, retained.service);
    }
}
