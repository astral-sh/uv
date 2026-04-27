use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    process::Stdio,
    str::FromStr,
    sync::{Arc, LazyLock, Mutex},
};

use tokio::{
    process::Command,
    sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock as AsyncRwLock},
};
use tracing::{debug, instrument, trace, warn};
use uv_cache_key::cache_digest;
use uv_fs::{LockedFile, LockedFileError, LockedFileMode};
use uv_redacted::DisplaySafeUrl;
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::credentials::Credentials;
use crate::matching;
use crate::realm::Realm;
use crate::service::Service;
use crate::store::PersistentCredential;

/// Service name prefix for storing credentials in a keyring.
static UV_SERVICE_PREFIX: &str = "uv:";

/// Process-local locks for native keyring access, keyed by realm.
static NATIVE_KEYRING_LOCKS: LazyLock<Mutex<HashMap<Realm, Arc<AsyncRwLock<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeCredentialLockMode {
    Read,
    Write,
}

impl NativeCredentialLockMode {
    fn file_mode(self) -> LockedFileMode {
        match self {
            Self::Read => LockedFileMode::Shared,
            Self::Write => LockedFileMode::Exclusive,
        }
    }
}

struct NativeCredentialLock {
    _read_guard: Option<OwnedRwLockReadGuard<()>>,
    _write_guard: Option<OwnedRwLockWriteGuard<()>>,
    _file_lock: LockedFile,
}

fn native_keyring_lock(realm: &Realm) -> Arc<AsyncRwLock<()>> {
    let mut locks = match NATIVE_KEYRING_LOCKS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    Arc::clone(
        locks
            .entry(realm.clone())
            .or_insert_with(|| Arc::new(AsyncRwLock::new(()))),
    )
}

fn native_credentials_lock_directory() -> Result<PathBuf, Error> {
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

fn native_credentials_lock_path(realm: &Realm) -> Result<PathBuf, Error> {
    let directory = native_credentials_lock_directory()?;
    fs_err::create_dir_all(&directory).map_err(Error::NativeLockDirectory)?;
    Ok(directory.join(format!("{}.lock", cache_digest(&realm.to_string()))))
}

#[instrument(skip_all, fields(realm = %realm, mode = ?mode))]
async fn native_credential_lock(
    realm: &Realm,
    mode: NativeCredentialLockMode,
) -> Result<NativeCredentialLock, Error> {
    let (read_guard, write_guard) = match mode {
        NativeCredentialLockMode::Read => {
            (Some(native_keyring_lock(realm).read_owned().await), None)
        }
        NativeCredentialLockMode::Write => {
            (None, Some(native_keyring_lock(realm).write_owned().await))
        }
    };
    let lock_path = native_credentials_lock_path(realm)?;
    let file_lock = LockedFile::acquire(
        &lock_path,
        mode.file_mode(),
        format!("native credential store for {realm}"),
    )
    .await
    .map_err(Error::NativeLock)?;

    native_credential_test_hook(mode).await;

    Ok(NativeCredentialLock {
        _read_guard: read_guard,
        _write_guard: write_guard,
        _file_lock: file_lock,
    })
}

#[cfg(test)]
#[derive(Debug)]
struct NativeCredentialTestHook {
    mode: NativeCredentialLockMode,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    block_next: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl NativeCredentialTestHook {
    fn blocking_next_lock(mode: NativeCredentialLockMode) -> Arc<Self> {
        Arc::new(Self {
            mode,
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            block_next: std::sync::atomic::AtomicBool::new(true),
        })
    }

    async fn on_lock_acquired(&self, mode: NativeCredentialLockMode) {
        if self.mode == mode
            && self
                .block_next
                .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.entered.notify_waiters();
            self.release.notified().await;
        }
    }

    async fn wait_until_entered(&self) {
        self.entered.notified().await;
    }

    fn release(&self) {
        self.release.notify_waiters();
    }
}

#[cfg(test)]
static NATIVE_CREDENTIAL_TEST_HOOK: LazyLock<Mutex<Option<Arc<NativeCredentialTestHook>>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(test)]
fn set_native_credential_test_hook(hook: Option<Arc<NativeCredentialTestHook>>) {
    let mut state = match NATIVE_CREDENTIAL_TEST_HOOK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *state = hook;
}

#[cfg(test)]
async fn native_credential_test_hook(mode: NativeCredentialLockMode) {
    let hook = {
        let state = match NATIVE_CREDENTIAL_TEST_HOOK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.clone()
    };

    if let Some(hook) = hook {
        hook.on_lock_acquired(mode).await;
    }
}

#[cfg(not(test))]
fn native_credential_test_hook(_mode: NativeCredentialLockMode) -> std::future::Ready<()> {
    std::future::ready(())
}

/// Return the legacy keyring service names that may contain credentials for `url`.
///
/// Earlier native-auth implementations stored credentials as plain passwords keyed by
/// the full URL, the host, or `scheme://host:port` for non-HTTPS URLs.
fn legacy_service_names(url: &DisplaySafeUrl) -> Vec<String> {
    let mut service_names = vec![url.as_str().to_string()];

    if let Some(host) = url.host_str() {
        let host = if let Some(port) = url.port() {
            format!("{host}:{port}")
        } else {
            host.to_string()
        };
        service_names.push(host.clone());

        if url.scheme() != "https" {
            service_names.push(format!("{}://{host}", url.scheme()));
        }
    }

    service_names.dedup();
    service_names
}

/// A backend for retrieving credentials from a keyring.
///
/// See pip's implementation for reference
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
#[derive(Debug)]
pub struct KeyringProvider {
    backend: KeyringProviderBackend,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Keyring(#[from] uv_keyring::Error),

    #[error("Stored credentials in the system keyring are corrupt")]
    CorruptStoredCredentials(#[source] serde_json::Error),

    #[error("Failed to serialize credentials for the system keyring")]
    SerializeStoredCredentials(#[source] serde_json::Error),

    #[error("Failed to prepare lock directory for native credential store")]
    NativeLockDirectory(#[source] std::io::Error),

    #[error("Failed to acquire lock for native credential store")]
    NativeLock(#[source] LockedFileError),

    #[error("Multiple credentials found for URL '{0}', specify which username to use")]
    AmbiguousUsername(DisplaySafeUrl),

    #[error("The '{0}' keyring provider does not support storing credentials")]
    StoreUnsupported(KeyringProviderBackend),

    #[error("The '{0}' keyring provider does not support removing credentials")]
    RemoveUnsupported(KeyringProviderBackend),
}

#[derive(Debug, Clone)]
pub enum KeyringProviderBackend {
    /// Use a native system keyring integration for credentials.
    Native,
    /// Use the external `keyring` command for credentials.
    Subprocess,
    #[cfg(test)]
    Dummy(Vec<(String, &'static str, &'static str)>),
}

impl std::fmt::Display for KeyringProviderBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native => write!(f, "native"),
            Self::Subprocess => write!(f, "subprocess"),
            #[cfg(test)]
            Self::Dummy(_) => write!(f, "dummy"),
        }
    }
}

impl KeyringProvider {
    /// Create a new [`KeyringProvider::Native`].
    pub fn native() -> Self {
        Self {
            backend: KeyringProviderBackend::Native,
        }
    }

    /// Create a new [`KeyringProvider::Subprocess`].
    pub fn subprocess() -> Self {
        Self {
            backend: KeyringProviderBackend::Subprocess,
        }
    }

    /// Store credentials for the given [`DisplaySafeUrl`] to the keyring.
    ///
    /// Only [`KeyringProviderBackend::Native`] is supported at this time.
    #[instrument(skip_all, fields(url = % url.to_string(), username))]
    pub async fn store(
        &self,
        url: &DisplaySafeUrl,
        credentials: &Credentials,
    ) -> Result<bool, Error> {
        // Ensure we have username and password
        if credentials.username().is_none() {
            trace!("Unable to store credentials in keyring for {url} due to missing username");
            return Ok(false);
        }
        if credentials.password().is_none() {
            trace!("Unable to store credentials in keyring for {url} due to missing password");
            return Ok(false);
        }

        // Ensure we strip credentials from the URL before storing.
        let clean_url = url.without_credentials().into_owned();
        let clean_url = match DisplaySafeUrl::parse(clean_url.as_str()) {
            Ok(url) => url,
            Err(err) => {
                trace!("Unable to re-parse URL: {err}");
                return Ok(false);
            }
        };

        let service = match Service::try_from(clean_url) {
            Ok(service) => service,
            Err(err) => {
                trace!("Unable to create service from URL: {err}");
                return Ok(false);
            }
        };

        match &self.backend {
            KeyringProviderBackend::Native => {
                self.store_native(&service, credentials).await?;
                Ok(true)
            }
            KeyringProviderBackend::Subprocess => {
                Err(Error::StoreUnsupported(self.backend.clone()))
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(_) => Err(Error::StoreUnsupported(self.backend.clone())),
        }
    }

    /// Store credentials to the system keyring.
    ///
    /// Uses realm-based storage where the keyring service name is the realm
    /// (scheme://host:port) and all credentials for that realm are stored as a JSON
    /// array in the password field. Each entry in the array contains the full Service
    /// URL + credentials.
    ///
    /// This supports multiple users per realm by storing them all in a single keyring
    /// entry as an array.
    #[instrument(skip(self, credentials))]
    async fn store_native(
        &self,
        service: &Service,
        credentials: &Credentials,
    ) -> Result<(), Error> {
        // Get the realm for the service name.
        let realm = Realm::from(service.url());
        let _lock = native_credential_lock(&realm, NativeCredentialLockMode::Write).await?;
        let realm_str = realm.to_string();
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{realm_str}");

        // Use a fixed username for the realm entry
        let keyring_username = "_uv_";
        let entry = uv_keyring::Entry::new(&prefixed_service, keyring_username)?;

        // Fetch existing credentials for this realm.
        let mut credentials_list: Vec<PersistentCredential> = match entry.get_password().await {
            Ok(json_data) => {
                serde_json::from_str(&json_data).map_err(Error::CorruptStoredCredentials)?
            }
            Err(uv_keyring::Error::NoEntry) => {
                trace!("No existing credentials for realm {realm_str}");
                Vec::new()
            }
            Err(err) => return Err(Error::Keyring(err)),
        };

        // Create the new credential entry
        let new_credential = PersistentCredential {
            service: service.clone(),
            credentials: credentials.clone(),
        };

        let new_username = credentials.to_username();

        // Remove any existing credential with the same service URL and username
        credentials_list.retain(|cred| {
            let matches_service = cred.service.url() == service.url();
            let matches_username = cred.credentials.to_username() == new_username;
            !(matches_service && matches_username)
        });

        // Add the new credential
        credentials_list.push(new_credential);

        // Serialize the updated list.
        let json_data =
            serde_json::to_string(&credentials_list).map_err(Error::SerializeStoredCredentials)?;

        entry.set_password(&json_data).await?;

        trace!("Stored credentials for realm {realm_str} in system keyring");
        Ok(())
    }

    /// Remove credentials for the given [`DisplaySafeUrl`] from the keyring.
    ///
    /// Only [`KeyringProviderBackend::Native`] is supported at this time.
    #[instrument(skip_all, fields(url = % url.to_string(), username))]
    pub async fn remove(&self, url: &DisplaySafeUrl, username: &str) -> Result<(), Error> {
        // Ensure we strip credentials from the URL before storing.
        let url = url.without_credentials().into_owned();
        let url = DisplaySafeUrl::parse(url.as_str()).map_err(|err| {
            trace!("Unable to re-parse URL for removal: {err}");
            Error::Keyring(uv_keyring::Error::NoEntry)
        })?;
        let service = Service::try_from(url).map_err(|err| {
            trace!("Unable to create service from URL for removal: {err}");
            Error::Keyring(uv_keyring::Error::NoEntry)
        })?;

        match &self.backend {
            KeyringProviderBackend::Native => {
                self.remove_native(&service, username).await?;
                Ok(())
            }
            KeyringProviderBackend::Subprocess => {
                Err(Error::RemoveUnsupported(self.backend.clone()))
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(_) => Err(Error::RemoveUnsupported(self.backend.clone())),
        }
    }

    /// Remove credentials from the system keyring for the given service and username.
    ///
    /// Removes matching entries from both the new realm-based JSON format and the legacy
    /// plain-password format. If the last credential is removed from a realm entry, the keyring
    /// entry is deleted entirely.
    #[instrument(skip(self))]
    async fn remove_native(&self, service: &Service, username: &str) -> Result<(), Error> {
        let realm = Realm::from(service.url());
        let _lock = native_credential_lock(&realm, NativeCredentialLockMode::Write).await?;

        let removed_from_realm = self.remove_native_realm_entry(service, username).await?;
        let removed_from_legacy = self.remove_native_legacy(service.url(), username).await?;

        if removed_from_realm || removed_from_legacy {
            Ok(())
        } else {
            debug!("No credential found for {username}@{service}");
            Err(Error::Keyring(uv_keyring::Error::NoEntry))
        }
    }

    /// Remove credentials from the new realm-based JSON keyring entry.
    ///
    /// Removal prefers an exact service match, but falls back to a realm-root credential when the
    /// requested URL only resolved via host-level matching.
    #[instrument(skip(self))]
    async fn remove_native_realm_entry(
        &self,
        service: &Service,
        username: &str,
    ) -> Result<bool, Error> {
        let realm = Realm::from(service.url());
        let realm_str = realm.to_string();
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{realm_str}");
        let keyring_username = "_uv_";
        let entry = uv_keyring::Entry::new(&prefixed_service, keyring_username)?;

        // Fetch existing credentials for this realm.
        let json_data = match entry.get_password().await {
            Ok(json_data) => json_data,
            Err(uv_keyring::Error::NoEntry) => return Ok(false),
            Err(err) => return Err(Error::Keyring(err)),
        };

        let mut credentials_list: Vec<PersistentCredential> =
            serde_json::from_str(&json_data).map_err(Error::CorruptStoredCredentials)?;
        let mut removed_service = service.clone();

        // Find and remove the credential matching the requested service and username.
        let initial_len = credentials_list.len();
        credentials_list.retain(|credential| {
            let matches_service = credential.service == *service;
            let matches_username =
                credential.credentials.to_username().as_deref() == Some(username);
            !(matches_service && matches_username)
        });

        if credentials_list.len() == initial_len {
            if let Ok(root_service) = Service::from_str(&realm_str)
                && root_service != *service
            {
                credentials_list.retain(|credential| {
                    let matches_service = credential.service == root_service;
                    let matches_username =
                        credential.credentials.to_username().as_deref() == Some(username);
                    !(matches_service && matches_username)
                });
                removed_service = root_service;
            }
        }

        // Check if we actually removed something.
        if credentials_list.len() == initial_len {
            return Ok(false);
        }

        // If this was the last credential, delete the entire entry.
        if credentials_list.is_empty() {
            entry.delete_credential().await?;
            trace!("Removed last credential for realm {realm_str}, deleted keyring entry");
        } else {
            // Otherwise, update with the remaining credentials.
            let json_data = serde_json::to_string(&credentials_list)
                .map_err(Error::SerializeStoredCredentials)?;
            entry.set_password(&json_data).await?;
            trace!(
                "Removed credentials for {username}@{removed_service}, {} credentials remaining",
                credentials_list.len()
            );
        }

        Ok(true)
    }

    /// Remove credentials from the legacy plain-password keyring entries.
    #[instrument(skip(self, url))]
    async fn remove_native_legacy(
        &self,
        url: &DisplaySafeUrl,
        username: &str,
    ) -> Result<bool, Error> {
        let mut removed = false;

        for service_name in legacy_service_names(url) {
            removed |= self
                .remove_native_legacy_entry(&service_name, username)
                .await?;
        }

        Ok(removed)
    }

    /// Remove credentials from a single legacy plain-password keyring entry.
    #[instrument(skip(self))]
    async fn remove_native_legacy_entry(
        &self,
        service_name: &str,
        username: &str,
    ) -> Result<bool, Error> {
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{service_name}");
        let entry = uv_keyring::Entry::new(&prefixed_service, username)?;

        match entry.delete_credential().await {
            Ok(()) => {
                trace!("Removed legacy credentials for {username}@{service_name}");
                Ok(true)
            }
            Err(uv_keyring::Error::NoEntry) => Ok(false),
            Err(err) => Err(Error::Keyring(err)),
        }
    }

    /// Fetch credentials for the given [`Url`] from the keyring.
    ///
    /// Returns [`Ok(None)`] if no password was found for the username.
    ///
    /// For the native backend, this uses realm-based storage with JSON serialization.
    /// It checks the realm (scheme://host:port) for matching credentials and performs
    /// prefix matching on paths, returning the most specific match.
    #[instrument(skip_all, fields(url = % url.to_string(), username))]
    pub async fn fetch(
        &self,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Result<Option<Credentials>, Error> {
        // Validate the request
        debug_assert!(
            url.host_str().is_some(),
            "Should only use keyring for URLs with host"
        );
        debug_assert!(
            url.password().is_none(),
            "Should only use keyring for URLs without a password"
        );
        debug_assert!(
            !username.map(str::is_empty).unwrap_or(false),
            "Should only use keyring with a non-empty username"
        );

        match self.backend {
            KeyringProviderBackend::Native => {
                self.fetch_native_with_prefix_matching(url, username).await
            }
            KeyringProviderBackend::Subprocess => {
                // For subprocess backend, keep the old logic.
                trace!("Checking keyring for URL {url}");
                let mut credentials = self.fetch_subprocess(url.as_str(), username).await;
                if credentials.is_none() {
                    let Some(host) = url.host_str() else {
                        return Ok(None);
                    };
                    let host = if let Some(port) = url.port() {
                        format!("{host}:{port}")
                    } else {
                        host.to_string()
                    };
                    trace!("Checking keyring for host {host}");
                    credentials = self.fetch_subprocess(&host, username).await;

                    // For non-HTTPS URLs, also try `scheme://host:port` to avoid
                    // cross-scheme credential leaks.
                    if credentials.is_none() && url.scheme() != "https" {
                        let scheme_host = format!("{}://{host}", url.scheme());
                        trace!("Checking keyring for scheme+host {scheme_host}");
                        credentials = self.fetch_subprocess(&scheme_host, username).await;
                    }
                }
                Ok(credentials
                    .map(|(username, password)| Credentials::basic(Some(username), Some(password))))
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(ref store) => {
                trace!("Checking keyring for URL {url}");
                let mut credentials = Self::fetch_dummy(store, url.as_str(), username);
                if credentials.is_none() {
                    let Some(host) = url.host_str() else {
                        return Ok(None);
                    };
                    let host = if let Some(port) = url.port() {
                        format!("{host}:{port}")
                    } else {
                        host.to_string()
                    };
                    trace!("Checking keyring for host {host}");
                    credentials = Self::fetch_dummy(store, &host, username);

                    // For non-HTTPS URLs, also try `scheme://host:port` to avoid
                    // cross-scheme credential leaks.
                    if credentials.is_none() && url.scheme() != "https" {
                        let scheme_host = format!("{}://{host}", url.scheme());
                        trace!("Checking keyring for scheme+host {scheme_host}");
                        credentials = Self::fetch_dummy(store, &scheme_host, username);
                    }
                }
                Ok(credentials
                    .map(|(username, password)| Credentials::basic(Some(username), Some(password))))
            }
        }
    }

    /// Fetch credentials from the native keyring with prefix matching.
    ///
    /// This mimics the behavior of the text credential store by:
    /// 1. Fetching all credentials stored for the realm
    /// 2. Filtering and matching based on service URL path prefix and username
    /// 3. Returning the most specific match (longest path prefix)
    /// 4. Falling back to the legacy plain-password format and migrating successful lookups
    #[instrument(skip(self))]
    async fn fetch_native_with_prefix_matching(
        &self,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Result<Option<Credentials>, Error> {
        let request_realm = Realm::from(&**url);

        let legacy_match = {
            let _lock =
                native_credential_lock(&request_realm, NativeCredentialLockMode::Read).await?;

            trace!("Checking keyring for realm {request_realm}");

            // Try to fetch from the realm-based entry (new format with JSON array).
            if let Some(credentials_list) = self.fetch_native_json_array(&request_realm).await? {
                // Find all matching credentials and pick the most specific one.
                let mut best: Option<(usize, &PersistentCredential)> = None;

                for persistent_credential in &credentials_list {
                    let service = &persistent_credential.service;
                    let credentials = &persistent_credential.credentials;
                    let stored_username = credentials.to_username();

                    // Check if this credential matches using shared matching logic.
                    if let Some(specificity) = matching::match_specificity(
                        service,
                        &stored_username,
                        url,
                        &request_realm,
                        username,
                    ) {
                        if best.is_none_or(|(best_specificity, _)| specificity > best_specificity) {
                            best = Some((specificity, persistent_credential));
                        } else if best
                            .is_some_and(|(best_specificity, _)| specificity == best_specificity)
                        {
                            return Err(Error::AmbiguousUsername(url.clone()));
                        }
                    }
                }

                if let Some((_, persistent_credential)) = best {
                    trace!("Found matching credentials in new format for {url}");
                    return Ok(Some(persistent_credential.credentials.clone()));
                }
            }

            // Fall back to old format: try all legacy service names in lookup order.
            trace!("Checking keyring for legacy plain password format");
            let mut legacy_match = None;
            for service_name in legacy_service_names(url) {
                trace!("Checking legacy keyring entry {service_name}");
                if let Some((username, password)) =
                    self.fetch_native_legacy(&service_name, username).await?
                {
                    legacy_match = Some((service_name, username, password));
                    break;
                }
            }
            legacy_match
        };

        if let Some((service_name, username, password)) = legacy_match {
            let credentials = Credentials::basic(Some(username), Some(password));
            if let Err(err) = self
                .migrate_native_legacy_credential(&request_realm, &service_name, &credentials)
                .await
            {
                warn!(
                    "Failed to migrate legacy credentials for {service_name} in realm {request_realm}: {err}"
                );
            }
            return Ok(Some(credentials));
        }

        Ok(None)
    }

    /// Fetch and parse JSON credentials array from the native keyring for a given realm.
    #[instrument(skip(self))]
    async fn fetch_native_json_array(
        &self,
        realm: &Realm,
    ) -> Result<Option<Vec<PersistentCredential>>, Error> {
        let realm_str = realm.to_string();
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{realm_str}");
        let keyring_username = "_uv_";

        let Ok(entry) = uv_keyring::Entry::new(&prefixed_service, keyring_username) else {
            return Ok(None);
        };

        match entry.get_password().await {
            Ok(json_data) => {
                // Try to parse as JSON array.
                let credentials_list =
                    serde_json::from_str::<Vec<PersistentCredential>>(&json_data)
                        .map_err(Error::CorruptStoredCredentials)?;
                trace!(
                    "Successfully parsed {} credentials from keyring for realm {realm_str}",
                    credentials_list.len()
                );
                Ok(Some(credentials_list))
            }
            Err(uv_keyring::Error::NoEntry) => {
                trace!("No entry found in system keyring for realm {realm_str}");
                Ok(None)
            }
            Err(err) => Err(Error::Keyring(err)),
        }
    }

    /// Fetch credentials from the native keyring using the legacy format.
    ///
    /// This maintains backward compatibility with credentials stored before
    /// the JSON-based storage was implemented.
    #[instrument(skip(self))]
    async fn fetch_native_legacy(
        &self,
        service: &str,
        username: Option<&str>,
    ) -> Result<Option<(String, String)>, Error> {
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{service}");
        let Some(username) = username else {
            return Ok(None);
        };
        let Ok(entry) = uv_keyring::Entry::new(&prefixed_service, username) else {
            return Ok(None);
        };
        match entry.get_password().await {
            Ok(password) => {
                trace!("Found legacy format credentials for {service}");
                Ok(Some((username.to_string(), password)))
            }
            Err(uv_keyring::Error::NoEntry) => {
                trace!("No legacy entry found in system keyring for {service}");
                Ok(None)
            }
            Err(err) => Err(Error::Keyring(err)),
        }
    }

    /// Migrate a legacy native keyring entry into the realm-based JSON format.
    ///
    /// This preserves the legacy entry scope by converting the matched legacy service name into a
    /// [`Service`] and removing only the legacy entry that satisfied the lookup.
    #[instrument(skip(self, credentials), fields(realm = %request_realm, service = service_name))]
    async fn migrate_native_legacy_credential(
        &self,
        request_realm: &Realm,
        service_name: &str,
        credentials: &Credentials,
    ) -> Result<(), Error> {
        let Some(username) = credentials.username() else {
            return Ok(());
        };

        let Ok(service) = Service::from_str(service_name) else {
            warn!(
                "Failed to parse legacy native credential service `{service_name}` during migration"
            );
            return Ok(());
        };

        if Realm::from(service.url()) != *request_realm {
            warn!(
                "Skipping migration for legacy native credential `{service_name}` because it does not match realm `{request_realm}`"
            );
            return Ok(());
        }

        let _lock = native_credential_lock(request_realm, NativeCredentialLockMode::Write).await?;
        let realm_str = request_realm.to_string();
        let prefixed_service = format!("{UV_SERVICE_PREFIX}{realm_str}");
        let keyring_username = "_uv_";
        let entry = uv_keyring::Entry::new(&prefixed_service, keyring_username)?;

        let mut credentials_list: Vec<PersistentCredential> = match entry.get_password().await {
            Ok(json_data) => {
                serde_json::from_str(&json_data).map_err(Error::CorruptStoredCredentials)?
            }
            Err(uv_keyring::Error::NoEntry) => Vec::new(),
            Err(err) => return Err(Error::Keyring(err)),
        };

        let has_matching_credential = credentials_list.iter().any(|credential| {
            credential.service == service
                && credential.credentials.to_username().as_deref() == Some(username)
        });

        if !has_matching_credential {
            credentials_list.push(PersistentCredential {
                service,
                credentials: credentials.clone(),
            });

            let json_data = serde_json::to_string(&credentials_list)
                .map_err(Error::SerializeStoredCredentials)?;
            entry.set_password(&json_data).await?;
            trace!(
                "Migrated legacy credentials for {username}@{service_name} into realm {realm_str}"
            );
        }

        self.remove_native_legacy_entry(service_name, username)
            .await?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn fetch_subprocess(
        &self,
        service_name: &str,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        // https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/auth.py#L136-L141
        let mut command = Command::new("keyring");
        command.arg("get").arg(service_name);

        if let Some(username) = username {
            command.arg(username);
        } else {
            command.arg("--mode").arg("creds");
        }

        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // If we're using `--mode creds`, we need to capture the output in order to avoid
            // showing users an "unrecognized arguments: --mode" error; otherwise, we stream stderr
            // so the user has visibility into keyring's behavior if it's doing something slow
            .stderr(if username.is_some() {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .spawn()
            .inspect_err(|err| warn!("Failure running `keyring` command: {err}"))
            .ok()?;

        let output = child
            .wait_with_output()
            .await
            .inspect_err(|err| warn!("Failed to wait for `keyring` output: {err}"))
            .ok()?;

        if output.status.success() {
            // If we captured stderr, display it in case it's helpful to the user
            // TODO(zanieb): This was done when we added `--mode creds` support for parity with the
            // existing behavior, but it might be a better UX to hide this on success? It also
            // might be problematic that we're not streaming it. We could change this given some
            // user feedback.
            std::io::stderr().write_all(&output.stderr).ok();

            // On success, parse the newline terminated credentials
            let output = String::from_utf8(output.stdout)
                .inspect_err(|err| warn!("Failed to parse response from `keyring` command: {err}"))
                .ok()?;

            let (username, password) = if let Some(username) = username {
                // We're only expecting a password
                let password = output.trim_end();
                (username, password)
            } else {
                // We're expecting a username and password
                let mut lines = output.lines();
                let username = lines.next()?;
                let Some(password) = lines.next() else {
                    warn!(
                        "Got username without password for `{service_name}` from `keyring` command"
                    );
                    return None;
                };
                (username, password)
            };

            if password.is_empty() {
                // We allow this for backwards compatibility, but it might be better to return
                // `None` instead if there's confusion from users — we haven't seen this in practice
                // yet.
                warn!("Got empty password for `{username}@{service_name}` from `keyring` command");
            }

            Some((username.to_string(), password.to_string()))
        } else {
            // On failure, no password was available
            let stderr = std::str::from_utf8(&output.stderr).ok()?;
            if stderr.contains("unrecognized arguments: --mode") {
                // N.B. We do not show the `service_name` here because we'll show the warning twice
                //      otherwise, once for the URL and once for the realm.
                warn_user_once!(
                    "Attempted to fetch credentials using the `keyring` command, but it does not support `--mode creds`; upgrade to `keyring>=v25.2.1` or provide a username"
                );
            } else if username.is_none() {
                // If we captured stderr, display it in case it's helpful to the user
                std::io::stderr().write_all(&output.stderr).ok();
            }
            None
        }
    }

    #[cfg(test)]
    fn fetch_dummy(
        store: &Vec<(String, &'static str, &'static str)>,
        service_name: &str,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        store.iter().find_map(|(service, user, password)| {
            if service == service_name && username.is_none_or(|username| username == *user) {
                Some(((*user).to_string(), (*password).to_string()))
            } else {
                None
            }
        })
    }

    /// Create a new provider with [`KeyringProviderBackend::Dummy`].
    #[cfg(test)]
    pub fn dummy<S: Into<String>, T: IntoIterator<Item = (S, &'static str, &'static str)>>(
        iter: T,
    ) -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(
                iter.into_iter()
                    .map(|(service, username, password)| (service.into(), username, password))
                    .collect(),
            ),
        }
    }

    /// Create a new provider with no credentials available.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        collections::HashMap,
        sync::{Arc, LazyLock, Mutex},
    };

    use super::*;
    use futures::FutureExt;
    use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
    use url::Url;
    use uv_keyring::credential::{
        Credential, CredentialApi, CredentialBuilderApi, CredentialPersistence,
    };

    static NATIVE_KEYRING_TEST_LOCK: LazyLock<AsyncMutex<()>> =
        LazyLock::new(|| AsyncMutex::new(()));

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct InMemoryCredentialKey {
        target: Option<String>,
        service: String,
        user: String,
    }

    #[derive(Debug)]
    struct InMemoryCredential {
        key: InMemoryCredentialKey,
        entries: Arc<Mutex<HashMap<InMemoryCredentialKey, Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl CredentialApi for InMemoryCredential {
        async fn set_secret(&self, secret: &[u8]) -> uv_keyring::Result<()> {
            let mut entries = self.entries.lock().unwrap();
            entries.insert(self.key.clone(), secret.to_vec());
            Ok(())
        }

        async fn get_secret(&self) -> uv_keyring::Result<Vec<u8>> {
            let entries = self.entries.lock().unwrap();
            let Some(secret) = entries.get(&self.key) else {
                return Err(uv_keyring::Error::NoEntry);
            };
            Ok(secret.clone())
        }

        async fn delete_credential(&self) -> uv_keyring::Result<()> {
            let mut entries = self.entries.lock().unwrap();
            if entries.remove(&self.key).is_some() {
                Ok(())
            } else {
                Err(uv_keyring::Error::NoEntry)
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug)]
    struct InMemoryCredentialBuilder {
        entries: Arc<Mutex<HashMap<InMemoryCredentialKey, Vec<u8>>>>,
    }

    impl CredentialBuilderApi for InMemoryCredentialBuilder {
        fn build(
            &self,
            target: Option<&str>,
            service: &str,
            user: &str,
        ) -> uv_keyring::Result<Box<Credential>> {
            Ok(Box::new(InMemoryCredential {
                key: InMemoryCredentialKey {
                    target: target.map(ToString::to_string),
                    service: service.to_string(),
                    user: user.to_string(),
                },
                entries: Arc::clone(&self.entries),
            }))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn persistence(&self) -> CredentialPersistence {
            CredentialPersistence::UntilDelete
        }
    }

    struct NativeTestKeyring {
        _guard: AsyncMutexGuard<'static, ()>,
    }

    impl NativeTestKeyring {
        async fn install() -> Self {
            let guard = NATIVE_KEYRING_TEST_LOCK.lock().await;
            let entries = Arc::new(Mutex::new(HashMap::new()));
            uv_keyring::set_default_credential_builder(Box::new(InMemoryCredentialBuilder {
                entries,
            }));
            Self { _guard: guard }
        }

        async fn insert_legacy(&self, service_name: &str, username: &str, password: &str) {
            let entry =
                uv_keyring::Entry::new(&format!("{UV_SERVICE_PREFIX}{service_name}"), username)
                    .unwrap();
            entry.set_password(password).await.unwrap();
        }

        async fn assert_legacy_absent(&self, service_name: &str, username: &str) {
            let entry =
                uv_keyring::Entry::new(&format!("{UV_SERVICE_PREFIX}{service_name}"), username)
                    .unwrap();
            match entry.get_password().await {
                Err(uv_keyring::Error::NoEntry) => {}
                Ok(password) => {
                    panic!(
                        "expected no legacy credential for {username}@{service_name}, found {password}"
                    );
                }
                Err(err) => {
                    panic!("expected no legacy credential for {username}@{service_name}: {err}");
                }
            }
        }
    }

    impl Drop for NativeTestKeyring {
        fn drop(&mut self) {
            uv_keyring::set_default_credential_builder(uv_keyring::default_credential_builder());
        }
    }

    struct NativeCredentialHookGuard;

    impl NativeCredentialHookGuard {
        fn install(hook: Arc<NativeCredentialTestHook>) -> Self {
            set_native_credential_test_hook(Some(hook));
            Self
        }
    }

    impl Drop for NativeCredentialHookGuard {
        fn drop(&mut self) {
            set_native_credential_test_hook(None);
        }
    }

    #[tokio::test]
    async fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let fetch = keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some("user"));
        if cfg!(debug_assertions) {
            let result = std::panic::AssertUnwindSafe(fetch).catch_unwind().await;
            assert!(result.is_err());
        } else {
            assert_eq!(fetch.await.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let fetch = keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some(url.username()));
        if cfg!(debug_assertions) {
            let result = std::panic::AssertUnwindSafe(fetch).catch_unwind().await;
            assert!(result.is_err());
        } else {
            assert_eq!(fetch.await.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn fetch_url_with_empty_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let fetch = keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some(url.username()));
        if cfg!(debug_assertions) {
            let result = std::panic::AssertUnwindSafe(fetch).catch_unwind().await;
            assert!(result.is_err());
        } else {
            assert_eq!(fetch.await.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn fetch_url_no_auth() {
        let url = Url::parse("https://example.com").unwrap();
        let url = DisplaySafeUrl::ref_cast(&url);
        let keyring = KeyringProvider::empty();
        let credentials = keyring.fetch(url, Some("user"));
        assert!(credentials.await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fetch_url() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        assert_eq!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await
                .unwrap(),
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("test").unwrap()),
                    Some("user")
                )
                .await
                .unwrap(),
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([("other.com", "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await
            .unwrap();
        assert_eq!(credentials, None);
    }

    #[tokio::test]
    async fn fetch_url_prefers_url_to_host() {
        let url = Url::parse("https://example.com/").unwrap();
        let keyring = KeyringProvider::dummy([
            (url.join("foo").unwrap().as_str(), "user", "password"),
            (url.host_str().unwrap(), "user", "other-password"),
        ]);
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("foo").unwrap()),
                    Some("user")
                )
                .await
                .unwrap(),
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await
                .unwrap(),
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("bar").unwrap()),
                    Some("user")
                )
                .await
                .unwrap(),
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await
            .unwrap();
        assert_eq!(
            credentials,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_no_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), None)
            .await
            .unwrap();
        assert_eq!(
            credentials,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "foo", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("bar"))
            .await
            .unwrap();
        assert_eq!(credentials, None);

        // Still fails if we have `foo` in the URL itself
        let url = Url::parse("https://foo@example.com").unwrap();
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("bar"))
            .await
            .unwrap();
        assert_eq!(credentials, None);
    }

    #[tokio::test]
    async fn fetch_http_scheme_host_fallback() {
        // When credentials are stored with scheme included (e.g., `http://host:port`),
        // the fetch should find them via the `scheme://host:port` fallback.
        let url = Url::parse("http://127.0.0.1:8080/basic-auth/simple/anyio/").unwrap();
        let keyring = KeyringProvider::dummy([("http://127.0.0.1:8080", "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await
            .unwrap();
        assert_eq!(
            credentials,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_http_scheme_host_no_cross_scheme() {
        // Credentials stored under `http://` should not be returned for `https://` requests.
        let url = Url::parse("https://127.0.0.1:8080/basic-auth/simple/anyio/").unwrap();
        let keyring = KeyringProvider::dummy([("http://127.0.0.1:8080", "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await
            .unwrap();
        assert_eq!(credentials, None);
    }

    #[tokio::test]
    async fn native_store_serializes_concurrent_updates() {
        let _keyring = NativeTestKeyring::install().await;
        let hook = NativeCredentialTestHook::blocking_next_lock(NativeCredentialLockMode::Write);
        let _hook = NativeCredentialHookGuard::install(Arc::clone(&hook));

        let url1 = DisplaySafeUrl::parse("https://example.com/first").unwrap();
        let url2 = DisplaySafeUrl::parse("https://example.com/second").unwrap();
        let credentials1 = Credentials::basic(Some("user1".to_string()), Some("pass1".to_string()));
        let credentials2 = Credentials::basic(Some("user2".to_string()), Some("pass2".to_string()));

        let first = tokio::spawn({
            let url = url1.clone();
            let credentials = credentials1.clone();
            async move { KeyringProvider::native().store(&url, &credentials).await }
        });

        hook.wait_until_entered().await;

        let mut second = tokio::spawn({
            let url = url2.clone();
            let credentials = credentials2.clone();
            async move { KeyringProvider::native().store(&url, &credentials).await }
        });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
                .await
                .is_err(),
            "second store should wait for the realm write lock"
        );

        hook.release();

        assert!(first.await.unwrap().unwrap());
        assert!(second.await.unwrap().unwrap());

        let provider = KeyringProvider::native();
        assert_eq!(
            provider.fetch(&url1, Some("user1")).await.unwrap(),
            Some(credentials1)
        );
        assert_eq!(
            provider.fetch(&url2, Some("user2")).await.unwrap(),
            Some(credentials2)
        );
    }

    #[tokio::test]
    async fn native_fetch_waits_for_concurrent_write() {
        let _keyring = NativeTestKeyring::install().await;
        let hook = NativeCredentialTestHook::blocking_next_lock(NativeCredentialLockMode::Write);
        let _hook = NativeCredentialHookGuard::install(Arc::clone(&hook));

        let url = DisplaySafeUrl::parse("https://example.com/first").unwrap();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        let store = tokio::spawn({
            let url = url.clone();
            let credentials = credentials.clone();
            async move { KeyringProvider::native().store(&url, &credentials).await }
        });

        hook.wait_until_entered().await;

        let mut fetch = tokio::spawn({
            let url = url.clone();
            async move { KeyringProvider::native().fetch(&url, Some("user")).await }
        });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut fetch)
                .await
                .is_err(),
            "fetch should wait for the realm write lock"
        );

        hook.release();

        assert!(store.await.unwrap().unwrap());
        assert_eq!(fetch.await.unwrap().unwrap(), Some(credentials));
    }

    #[tokio::test]
    async fn native_fetch_migrates_legacy_host_entry_on_first_use() {
        let test_keyring = NativeTestKeyring::install().await;
        let provider = KeyringProvider::native();
        let first_url = DisplaySafeUrl::parse("https://legacy-auth.example.test/path").unwrap();
        let second_url = DisplaySafeUrl::parse("https://legacy-auth.example.test/other").unwrap();
        let username = "legacy-user";
        let credentials =
            Credentials::basic(Some(username.to_string()), Some("legacy-pass".to_string()));

        test_keyring
            .insert_legacy("legacy-auth.example.test", username, "legacy-pass")
            .await;

        assert_eq!(
            provider.fetch(&first_url, Some(username)).await.unwrap(),
            Some(credentials.clone())
        );

        test_keyring
            .assert_legacy_absent("legacy-auth.example.test", username)
            .await;

        assert_eq!(
            provider.fetch(&second_url, Some(username)).await.unwrap(),
            Some(credentials)
        );
    }

    #[tokio::test]
    async fn native_remove_removes_legacy_host_entry() {
        let test_keyring = NativeTestKeyring::install().await;
        let provider = KeyringProvider::native();
        let url = DisplaySafeUrl::parse("https://legacy-auth.example.test/first").unwrap();
        let username = "legacy-user";

        test_keyring
            .insert_legacy("legacy-auth.example.test", username, "legacy-pass")
            .await;

        assert_eq!(
            provider.fetch(&url, Some(username)).await.unwrap(),
            Some(Credentials::basic(
                Some(username.to_string()),
                Some("legacy-pass".to_string())
            ))
        );

        provider.remove(&url, username).await.unwrap();

        test_keyring
            .assert_legacy_absent("legacy-auth.example.test", username)
            .await;
        assert_eq!(provider.fetch(&url, Some(username)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn native_remove_removes_all_matching_legacy_entries() {
        let test_keyring = NativeTestKeyring::install().await;
        let provider = KeyringProvider::native();
        let url = DisplaySafeUrl::parse("https://legacy-auth.example.test/path").unwrap();
        let username = "legacy-user";

        test_keyring
            .insert_legacy("legacy-auth.example.test", username, "host-pass")
            .await;
        test_keyring
            .insert_legacy(url.as_str(), username, "url-pass")
            .await;

        assert_eq!(
            provider.fetch(&url, Some(username)).await.unwrap(),
            Some(Credentials::basic(
                Some(username.to_string()),
                Some("url-pass".to_string())
            ))
        );

        provider.remove(&url, username).await.unwrap();

        test_keyring
            .assert_legacy_absent(url.as_str(), username)
            .await;
        test_keyring
            .assert_legacy_absent("legacy-auth.example.test", username)
            .await;
        assert_eq!(provider.fetch(&url, Some(username)).await.unwrap(), None);
    }

    #[test]
    fn legacy_service_names_https_include_url_and_host() {
        let url = DisplaySafeUrl::parse("https://example.com/api").unwrap();
        assert_eq!(
            legacy_service_names(&url),
            vec![
                "https://example.com/api".to_string(),
                "example.com".to_string(),
            ]
        );
    }

    #[test]
    fn legacy_service_names_http_include_scheme_host() {
        let url = DisplaySafeUrl::parse("http://127.0.0.1:8080/api").unwrap();
        assert_eq!(
            legacy_service_names(&url),
            vec![
                "http://127.0.0.1:8080/api".to_string(),
                "127.0.0.1:8080".to_string(),
                "http://127.0.0.1:8080".to_string(),
            ]
        );
    }
}
