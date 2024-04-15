use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::credentials::Credentials;
use crate::NetLoc;
use reqwest::Request;
use tracing::debug;
use url::Url;

type CacheKey = (NetLoc, Option<String>);

pub struct CredentialsCache {
    store: Mutex<HashMap<CacheKey, Arc<Credentials>>>,
}

#[derive(Debug, Clone)]
pub enum CheckResponse {
    /// Credentials are already on the request.
    OnRequest(Arc<Credentials>),
    /// Credentials were found in the cache.
    Cached(Arc<Credentials>),
    // Credentials were not found in the cache or request.
    None,
}

impl CheckResponse {
    /// Retrieve the credentials, if any.
    pub fn get(&self) -> Option<&Credentials> {
        match self {
            Self::Cached(credentials) => Some(credentials.as_ref()),
            Self::OnRequest(credentials) => Some(credentials.as_ref()),
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

    /// Return the credentials that should be used for a request, if any.
    ///
    /// If complete credentials are already present on the request, they will be returned.
    /// If the credentials are partial, i.e. missing a password, the cache will be checked
    /// for a corresponding entry.
    pub(crate) fn check_request(&self, request: &Request) -> CheckResponse {
        let store = self.store.lock().unwrap();

        let credentials = Credentials::from_request(request).map(Arc::new);
        let key = CredentialsCache::key(
            request.url(),
            credentials
                .as_ref()
                .and_then(|credentials| credentials.username().map(str::to_string)),
        );

        if let Some(credentials) = credentials {
            if credentials.password().is_some() {
                debug!("Request already has password, skipping cache");
                // No need to look-up, we have a password already
                return CheckResponse::OnRequest(credentials);
            }
            debug!("No password found on request, checking cache");
            let existing = store.get(&key);
            existing
                .cloned()
                .map(CheckResponse::Cached)
                .unwrap_or(CheckResponse::OnRequest(credentials))
        } else {
            debug!("No credentials on request, checking cache");
            let credentials = store.get(&key).cloned();
            if credentials.is_some() {
                debug!("Found cached credentials: {credentials:?}");
            } else {
                debug!("No credentials in cache");
            }
            credentials
                .map(CheckResponse::Cached)
                .unwrap_or(CheckResponse::None)
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
}
