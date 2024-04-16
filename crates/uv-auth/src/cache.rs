use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::credentials::Credentials;
use crate::NetLoc;

use tracing::trace;
use url::Url;

type CacheKey = (NetLoc, Option<String>);

pub struct CredentialsCache {
    store: Mutex<HashMap<CacheKey, Arc<Credentials>>>,
}

#[derive(Debug, Clone)]
pub enum CheckResponse {
    /// The given credentials should be used and are not present in the cache.
    Uncached(Arc<Credentials>),
    /// Credentials were found in the cache.
    Cached(Arc<Credentials>),
    // Credentials were not found in the cache and none were provided.
    None,
}

impl CheckResponse {
    /// Retrieve the credentials, if any.
    pub fn get(&self) -> Option<&Credentials> {
        match self {
            Self::Cached(credentials) => Some(credentials.as_ref()),
            Self::Uncached(credentials) => Some(credentials.as_ref()),
            Self::None => None,
        }
    }

    /// Returns true if there are credentials with a password.
    pub fn is_authenticated(&self) -> bool {
        self.get()
            .is_some_and(|credentials| credentials.password().is_some())
    }
}

impl Default for CredentialsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialsCache {
    /// Create a new cache.
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    /// Create an owned cache key.
    fn key(url: &Url, username: Option<String>) -> CacheKey {
        (NetLoc::from(url), username)
    }

    /// Return the credentials that should be used for a URL, if any.
    ///
    /// The [`Url`] is not checked for credentials. Existing credentials should be extracted and passed
    /// separately.
    ///
    /// If complete credentials are provided, they will be returned as [`CheckResponse::Existing`]
    /// If the credentials are partial, i.e. missing a password, the cache will be checked
    /// for a corresponding entry.
    pub(crate) fn check(&self, url: &Url, credentials: Option<Credentials>) -> CheckResponse {
        let store = self.store.lock().unwrap();

        let credentials = credentials.map(Arc::new);
        let key = CredentialsCache::key(
            url,
            credentials
                .as_ref()
                .and_then(|credentials| credentials.username().map(str::to_string)),
        );

        if let Some(credentials) = credentials {
            if credentials.password().is_some() {
                trace!("Existing credentials include password, skipping cache");
                // No need to look-up, we have a password already
                return CheckResponse::Uncached(credentials);
            }
            trace!("Existing credentials missing password, checking cache");
            let existing = store.get(&key);
            existing
                .cloned()
                .map(CheckResponse::Cached)
                .inspect(|_| trace!("Found cached credentials."))
                .unwrap_or_else(|| {
                    trace!("No credentials in cache, using existing credentials");
                    CheckResponse::Uncached(credentials)
                })
        } else {
            trace!("No credentials on request, checking cache...");
            store
                .get(&key)
                .cloned()
                .map(CheckResponse::Cached)
                .inspect(|_| trace!("Found cached credentials."))
                .unwrap_or_else(|| {
                    trace!("No credentials in cache.");
                    CheckResponse::None
                })
        }
    }

    /// Update the cache with the given credentials if none exist.
    pub(crate) fn set_default(&self, url: &Url, credentials: Arc<Credentials>) {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return;
        }

        // Insert an entry for requests including the username
        if let Some(username) = credentials.username() {
            let key = CredentialsCache::key(url, Some(username.to_string()));
            if !self.contains_key(&key) {
                self.insert_entry(key, credentials.clone());
            }
        }

        // Insert an entry for requests with no username
        let key = CredentialsCache::key(url, None);
        if !self.contains_key(&key) {
            self.insert_entry(key, credentials.clone());
        }
    }

    /// Update the cache with the given credentials.
    pub(crate) fn insert(&self, url: &Url, credentials: Arc<Credentials>) {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return;
        }

        // Insert an entry for requests including the username
        if let Some(username) = credentials.username() {
            self.insert_entry(
                CredentialsCache::key(url, Some(username.to_string())),
                credentials.clone(),
            );
        }

        // Insert an entry for requests with no username
        self.insert_entry(CredentialsCache::key(url, None), credentials.clone());
    }

    /// Private interface to update a cache entry.
    fn insert_entry(&self, key: (NetLoc, Option<String>), credentials: Arc<Credentials>) -> bool {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return false;
        }

        let mut store = self.store.lock().unwrap();

        // Always replace existing entries if we have a password
        if credentials.password().is_some() {
            store.insert(key, credentials.clone());
            return true;
        }

        // If we only have a username, add a new entry or replace an existing entry if it doesn't have a password
        let existing = store.get(&key);
        if existing.is_none()
            || existing.is_some_and(|credentials| credentials.password().is_none())
        {
            store.insert(key, credentials.clone());
            return true;
        }

        false
    }

    /// Returns true if a key is in the cache.
    fn contains_key(&self, key: &(NetLoc, Option<String>)) -> bool {
        let store = self.store.lock().unwrap();
        store.contains_key(key)
    }
}
