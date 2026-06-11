use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock, Mutex, Weak},
};

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock as AsyncRwLock};
use tracing::{instrument, trace, warn};
use uv_cache_key::cache_digest;
use uv_fs::{LockedFile, LockedFileMode};
use uv_redacted::DisplaySafeUrl;
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

use super::Error;
use crate::{Credentials, Realm, Service, Username, matching, persistent::PersistentCredential};

#[cfg(not(target_os = "windows"))]
use collection as platform;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(not(target_os = "windows"))]
mod collection;
#[cfg(target_os = "windows")]
mod windows;

/// Prefix for legacy native credentials stored as individual passwords.
pub(super) const LEGACY_SERVICE_PREFIX: &str = "uv:";

/// Namespace for credentials using the JSON native-auth storage format.
pub(super) const NATIVE_SERVICE_PREFIX: &str = "uv:native-auth:";

/// Process-local locks for native credential operations.
static NATIVE_LOCKS: LazyLock<Mutex<HashMap<String, Weak<AsyncRwLock<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A read lock for the current native credential format in one realm.
pub(super) struct RealmReadGuard {
    realm: Realm,
    _process: OwnedRwLockReadGuard<()>,
    _file: LockedFile,
}

/// A write lock for the current native credential format in one realm.
pub(super) struct RealmWriteGuard {
    realm: Realm,
    _process: OwnedRwLockWriteGuard<()>,
    _file: LockedFile,
}

/// A read lock for one exact legacy keyring service name.
pub(super) struct LegacyReadGuard {
    service_name: String,
    _process: OwnedRwLockReadGuard<()>,
    _file: LockedFile,
}

/// A write lock for one exact legacy keyring service name.
pub(super) struct LegacyWriteGuard {
    service_name: String,
    _process: OwnedRwLockWriteGuard<()>,
    _file: LockedFile,
}

/// A borrowed realm lock accepted by read-only backend operations.
#[derive(Clone, Copy)]
pub(super) enum RealmGuardRef<'a> {
    Read(&'a RealmReadGuard),
    Write(&'a RealmWriteGuard),
}

impl RealmGuardRef<'_> {
    /// Return the realm protected by this guard.
    pub(super) fn realm(&self) -> &Realm {
        match self {
            Self::Read(guard) => &guard.realm,
            Self::Write(guard) => &guard.realm,
        }
    }
}

impl RealmWriteGuard {
    /// Return the realm protected by this guard.
    pub(super) fn realm(&self) -> &Realm {
        &self.realm
    }
}

/// A borrowed legacy lock accepted by read-only backend operations.
#[derive(Clone, Copy)]
pub(super) enum LegacyGuardRef<'a> {
    Read(&'a LegacyReadGuard),
    Write(&'a LegacyWriteGuard),
}

impl LegacyGuardRef<'_> {
    /// Return the exact legacy service name protected by this guard.
    pub(super) fn service_name(&self) -> &str {
        match self {
            Self::Read(guard) => &guard.service_name,
            Self::Write(guard) => &guard.service_name,
        }
    }
}

/// Store credentials for an exact service in the platform keyring.
#[instrument(skip(credentials))]
pub(super) async fn store(service: &Service, credentials: &Credentials) -> Result<(), Error> {
    let realm = Realm::from(service.url());
    let guard = acquire_realm_write(&realm).await?;
    platform::store_current(
        &guard,
        &PersistentCredential {
            service: service.clone(),
            credentials: credentials.clone(),
        },
    )
    .await
}

/// Remove credentials for an exact service and username from the platform keyring.
#[instrument]
pub(super) async fn remove(service: &Service, username: &str) -> Result<(), Error> {
    let realm = Realm::from(service.url());
    let realm_guard = acquire_realm_write(&realm).await?;

    let mut removed_legacy = false;
    for service_name in legacy_removal_service_names(service.url()) {
        let legacy_guard = acquire_legacy_write(&service_name).await?;
        removed_legacy |= system_remove_legacy(&legacy_guard, username).await?;
    }

    let username = Username::from(Some(username.to_string()));
    let removed_current = platform::remove_current(&realm_guard, service, &username).await?;

    if removed_current || removed_legacy {
        Ok(())
    } else {
        Err(Error::Keyring(uv_keyring::Error::NoEntry))
    }
}

/// Fetch the best matching credentials, migrating a legacy entry when safe.
#[instrument]
pub(super) async fn fetch(
    url: &DisplaySafeUrl,
    username: Option<&str>,
) -> Result<Option<Credentials>, Error> {
    let realm = Realm::from(url);
    let legacy_match = {
        let realm_guard = acquire_realm_read(&realm).await?;
        let credentials = platform::load_current(RealmGuardRef::Read(&realm_guard)).await?;

        if let Some(credentials) = select_credentials(&credentials, url, username)? {
            return Ok(Some(credentials.clone()));
        }

        let mut legacy_match = None;
        for service_name in legacy_service_names(url) {
            let Some(username) = username else {
                break;
            };
            let legacy_guard = acquire_legacy_read(&service_name).await?;
            if let Some(password) =
                system_fetch_legacy(LegacyGuardRef::Read(&legacy_guard), username).await?
            {
                legacy_match = Some((
                    service_name.clone(),
                    Credentials::basic(Some(username.to_string()), Some(password)),
                    should_migrate_legacy_service(url, &service_name),
                ));
                break;
            }
        }
        legacy_match
    };

    let Some((service_name, credentials, should_migrate)) = legacy_match else {
        return Ok(None);
    };

    if should_migrate
        && let Err(err) = migrate_legacy_credential(&realm, &service_name, &credentials).await
    {
        warn!("Failed to migrate legacy credentials in realm {realm}: {err}");
    }

    Ok(Some(credentials))
}

/// Migrate an unchanged legacy credential into the current format.
#[instrument(skip(service_name, credentials), fields(realm = %realm))]
async fn migrate_legacy_credential(
    realm: &Realm,
    service_name: &str,
    credentials: &Credentials,
) -> Result<(), Error> {
    let Some(username) = credentials.username() else {
        return Ok(());
    };

    let service = if service_name.contains("://") {
        Service::from_str(service_name)
    } else {
        Service::from_str(&realm.to_string())
    };
    let Ok(service) = service else {
        warn!("Failed to parse a legacy credential service during migration");
        return Ok(());
    };
    if Realm::from(service.url()) != *realm {
        trace!("Skipping migration for a legacy credential outside realm `{realm}`");
        return Ok(());
    }

    let realm_guard = acquire_realm_write(realm).await?;
    let legacy_guard = acquire_legacy_write(service_name).await?;
    let Some(current_password) =
        system_fetch_legacy(LegacyGuardRef::Write(&legacy_guard), username).await?
    else {
        return Ok(());
    };
    if credentials.password() != Some(current_password.as_str()) {
        return Ok(());
    }

    let credential = PersistentCredential {
        service,
        credentials: Credentials::basic(Some(username.to_string()), Some(current_password)),
    };
    let current_credentials = platform::load_current(RealmGuardRef::Write(&realm_guard)).await?;
    if current_credentials.iter().any(|current| {
        current.service == credential.service
            && current.credentials.to_username() == credential.credentials.to_username()
    }) {
        system_remove_legacy(&legacy_guard, username).await?;
        return Ok(());
    }
    platform::store_current(&realm_guard, &credential).await?;
    system_remove_legacy(&legacy_guard, username).await?;
    Ok(())
}

/// Select the most specific matching credential.
fn select_credentials<'a>(
    credentials: &'a [PersistentCredential],
    url: &DisplaySafeUrl,
    username: Option<&str>,
) -> Result<Option<&'a Credentials>, Error> {
    matching::select_credential(
        credentials.iter().map(|credential| {
            (
                &credential.service,
                credential.credentials.username(),
                &credential.credentials,
            )
        }),
        url,
        username,
    )
    .map_err(|_| Error::AmbiguousUsername(url.clone()))
}

/// Return legacy service names in lookup order.
fn legacy_service_names(url: &DisplaySafeUrl) -> Vec<String> {
    let mut service_names = vec![url.as_str().to_string()];
    if let Some(host) = legacy_host_service_name(url) {
        if url.scheme() != "https" {
            service_names.push(format!("{}://{host}", url.scheme()));
        }
        service_names.push(host);
    }
    service_names.dedup();
    service_names
}

/// Return legacy service names that an exact logout may safely remove.
fn legacy_removal_service_names(url: &DisplaySafeUrl) -> Vec<String> {
    let mut service_names = vec![url.as_str().to_string()];
    if let Some(host) = legacy_host_service_name(url) {
        if url.scheme() == "https" {
            service_names.push(host);
        } else {
            service_names.push(format!("{}://{host}", url.scheme()));
        }
    }
    service_names.dedup();
    service_names
}

/// Return the host and optional explicit port used by legacy entries.
fn legacy_host_service_name(url: &DisplaySafeUrl) -> Option<String> {
    let host = url.host_str()?;
    Some(if let Some(port) = url.port() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    })
}

/// Return whether a legacy entry can be permanently scoped to the request realm.
fn should_migrate_legacy_service(url: &DisplaySafeUrl, service_name: &str) -> bool {
    service_name != url.as_str()
        && !(url.scheme() != "https"
            && legacy_host_service_name(url).as_deref() == Some(service_name))
}

/// Ensure that a service belongs to the realm protected by a lock guard.
pub(super) fn ensure_service_realm(realm: &Realm, service: &Service) -> Result<(), Error> {
    if Realm::from(service.url()) == *realm {
        Ok(())
    } else {
        Err(Error::MismatchedRealm)
    }
}

/// Return the configured directory containing native credential lock files.
fn native_lock_directory() -> Result<PathBuf, Error> {
    if let Some(directory) = std::env::var_os(EnvVars::UV_CREDENTIALS_DIR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(directory.join("native"));
    }

    Ok(StateStore::from_settings(None)
        .map_err(Error::NativeLockDirectory)?
        .bucket(StateBucket::Credentials)
        .join("native"))
}

/// Return the lock path for a stable credential operation key.
fn native_lock_path(directory: &Path, key: &str) -> PathBuf {
    directory.join(format!("{}.lock", cache_digest(&key)))
}

/// Return the process-local lock for a stable credential operation key.
fn process_lock(key: &str) -> Arc<AsyncRwLock<()>> {
    let key = cache_digest(&key);
    let mut locks = match NATIVE_LOCKS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }

    let lock = Arc::new(AsyncRwLock::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

/// Create and return the directory containing native credential lock files.
fn create_native_lock_directory() -> Result<PathBuf, Error> {
    let directory = native_lock_directory()?;
    fs_err::create_dir_all(&directory).map_err(Error::NativeLockDirectory)?;
    Ok(directory)
}

/// Acquire a shared lock for one realm.
async fn acquire_realm_read(realm: &Realm) -> Result<RealmReadGuard, Error> {
    let key = format!("realm:{realm}");
    let process = process_lock(&key).read_owned().await;
    let directory = create_native_lock_directory()?;
    let file = LockedFile::acquire(
        native_lock_path(&directory, &key),
        LockedFileMode::Shared,
        format!("native credential store for {realm}"),
    )
    .await
    .map_err(Error::NativeLock)?;
    Ok(RealmReadGuard {
        realm: realm.clone(),
        _process: process,
        _file: file,
    })
}

/// Acquire an exclusive lock for one realm.
async fn acquire_realm_write(realm: &Realm) -> Result<RealmWriteGuard, Error> {
    let key = format!("realm:{realm}");
    let process = process_lock(&key).write_owned().await;
    let directory = create_native_lock_directory()?;
    let file = LockedFile::acquire(
        native_lock_path(&directory, &key),
        LockedFileMode::Exclusive,
        format!("native credential store for {realm}"),
    )
    .await
    .map_err(Error::NativeLock)?;
    Ok(RealmWriteGuard {
        realm: realm.clone(),
        _process: process,
        _file: file,
    })
}

/// Acquire a shared lock for one exact legacy service name.
async fn acquire_legacy_read(service_name: &str) -> Result<LegacyReadGuard, Error> {
    let key = legacy_lock_key(service_name);
    let process = process_lock(&key).read_owned().await;
    let directory = create_native_lock_directory()?;
    let file = LockedFile::acquire(
        native_lock_path(&directory, &key),
        LockedFileMode::Shared,
        "legacy native credential",
    )
    .await
    .map_err(Error::NativeLock)?;
    Ok(LegacyReadGuard {
        service_name: service_name.to_string(),
        _process: process,
        _file: file,
    })
}

/// Acquire an exclusive lock for one exact legacy service name.
async fn acquire_legacy_write(service_name: &str) -> Result<LegacyWriteGuard, Error> {
    let key = legacy_lock_key(service_name);
    let process = process_lock(&key).write_owned().await;
    let directory = create_native_lock_directory()?;
    let file = LockedFile::acquire(
        native_lock_path(&directory, &key),
        LockedFileMode::Exclusive,
        "legacy native credential",
    )
    .await
    .map_err(Error::NativeLock)?;
    Ok(LegacyWriteGuard {
        service_name: service_name.to_string(),
        _process: process,
        _file: file,
    })
}

/// Return a bounded lock identity for a legacy service.
///
/// URL-based legacy entries are already scoped to a realm. Bare-host entries use a host lock
/// shared by HTTP and HTTPS so cross-scheme fallbacks cannot race one another.
fn legacy_lock_key(service_name: &str) -> String {
    if let Ok(url) = DisplaySafeUrl::parse(service_name) {
        format!("legacy-realm:{}", Realm::from(&url))
    } else {
        format!("legacy-host:{service_name}")
    }
}

/// Fetch a legacy password from the system keyring.
pub(super) async fn system_fetch_legacy(
    guard: LegacyGuardRef<'_>,
    username: &str,
) -> Result<Option<String>, Error> {
    let entry = uv_keyring::Entry::new(
        &format!("{LEGACY_SERVICE_PREFIX}{}", guard.service_name()),
        username,
    )?;
    match entry.get_password().await {
        Ok(password) => Ok(Some(password)),
        Err(uv_keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(Error::Keyring(err)),
    }
}

/// Remove a legacy password from the system keyring.
pub(super) async fn system_remove_legacy(
    guard: &LegacyWriteGuard,
    username: &str,
) -> Result<bool, Error> {
    let entry = uv_keyring::Entry::new(
        &format!("{LEGACY_SERVICE_PREFIX}{}", guard.service_name),
        username,
    )?;
    match entry.delete_credential().await {
        Ok(()) => Ok(true),
        Err(uv_keyring::Error::NoEntry) => Ok(false),
        Err(err) => Err(Error::Keyring(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_path_is_pure() {
        let directory = Path::new("/credentials/native");
        assert_eq!(
            native_lock_path(directory, "realm:https://example.com"),
            directory.join(format!(
                "{}.lock",
                cache_digest(&"realm:https://example.com")
            ))
        );
    }

    #[test]
    fn legacy_service_names_preserve_lookup_order() {
        let url = DisplaySafeUrl::parse("http://localhost:8080/path").unwrap();
        assert_eq!(
            legacy_service_names(&url),
            [
                "http://localhost:8080/path",
                "http://localhost:8080",
                "localhost:8080",
            ]
        );
    }

    #[test]
    fn legacy_migration_preserves_service_scope() {
        let https_url =
            DisplaySafeUrl::parse("https://example.com/path").expect("HTTPS URL should be valid");
        assert!(!should_migrate_legacy_service(
            &https_url,
            https_url.as_str()
        ));
        assert!(should_migrate_legacy_service(&https_url, "example.com"));

        let http_url =
            DisplaySafeUrl::parse("http://localhost:8080/path").expect("HTTP URL should be valid");
        assert!(should_migrate_legacy_service(
            &http_url,
            "http://localhost:8080"
        ));
        assert!(!should_migrate_legacy_service(&http_url, "localhost:8080"));
    }

    #[test]
    fn legacy_removal_does_not_cross_schemes() {
        let https_url =
            DisplaySafeUrl::parse("https://example.com/path").expect("HTTPS URL should be valid");
        assert_eq!(
            legacy_removal_service_names(&https_url),
            ["https://example.com/path", "example.com"]
        );

        let http_url =
            DisplaySafeUrl::parse("http://localhost:8080/path").expect("HTTP URL should be valid");
        assert_eq!(
            legacy_removal_service_names(&http_url),
            ["http://localhost:8080/path", "http://localhost:8080"]
        );
    }
}
