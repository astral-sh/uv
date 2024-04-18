use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::credentials::{Credentials, Username};
use crate::NetLoc;

use tracing::trace;
use url::Url;

pub struct CredentialsCache {
    realms: Mutex<HashMap<(NetLoc, Username), Arc<Credentials>>>,
    #[allow(clippy::type_complexity)]
    urls: Mutex<Vec<((Url, Username), Arc<Credentials>)>>,
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
            realms: Mutex::new(HashMap::new()),
            urls: Mutex::new(Vec::new()),
        }
    }

    /// Create an owned cache key for the realm
    fn realm(url: &Url, username: Username) -> (NetLoc, Username) {
        (NetLoc::from(url), username)
    }

    /// Return the credentials that should be used for a realm, if any.
    pub(crate) fn get_realm(&self, netloc: NetLoc, username: Username) -> Option<Arc<Credentials>> {
        let realms = self.realms.lock().unwrap();
        let name = if let Some(username) = username.as_deref() {
            format!("{username}@{netloc}")
        } else {
            netloc.to_string()
        };
        let key = (netloc, username);

        realms
            .get(&key)
            .cloned()
            .map(Some)
            .inspect(|_| trace!("Found cached credentials for realm {name}"))
            .unwrap_or_else(|| {
                trace!("No credentials in cache for realm {name}");
                None
            })
    }

    /// Return the cached credentials for a URL, if any.
    ///
    /// The caller must not pass a URL with a username attached.
    pub(crate) fn get_url(&self, url: &Url, username: Username) -> Option<Arc<Credentials>> {
        debug_assert!(url.username().is_empty());
        let urls = self.urls.lock().unwrap();
        for ((cached_url, cached_username), credentials) in urls.iter() {
            if cached_username == &username && url.as_str().starts_with(cached_url.as_str()) {
                trace!("Found cached credentials with prefix {cached_url}");
                return Some(credentials.clone());
            }
        }
        let name = if let Some(username) = username.as_deref() {
            format!("{username}@{url}")
        } else {
            url.to_string()
        };
        trace!("No credentials in cache for {name}");
        None
    }

    /// Update the cache with the given credentials.
    pub(crate) fn insert(&self, url: &Url, credentials: Arc<Credentials>) {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return;
        }

        // Insert an entry for requests including the username
        let username = credentials.to_username();
        if username.is_some() {
            let realm = (NetLoc::from(url), username.clone());
            self.insert_entry(realm, credentials.clone());
        }

        // Insert an entry for requests with no username
        self.insert_entry(
            CredentialsCache::realm(url, Username::none()),
            credentials.clone(),
        );

        // Insert an entry for the URL
        let mut urls = self.urls.lock().unwrap();
        let mut cache_url = url.clone();
        cache_url.set_query(None);
        let _ = cache_url.set_password(None);
        let _ = cache_url.set_username("");
        urls.push(((cache_url, username), credentials.clone()));
    }

    /// Private interface to update a cache entry.
    fn insert_entry(&self, key: (NetLoc, Username), credentials: Arc<Credentials>) -> bool {
        // Do not cache empty credentials
        if credentials.is_empty() {
            return false;
        }

        let mut realms = self.realms.lock().unwrap();

        // Always replace existing entries if we have a password
        if credentials.password().is_some() {
            realms.insert(key, credentials.clone());
            return true;
        }

        // If we only have a username, add a new entry or replace an existing entry if it doesn't have a password
        let existing = realms.get(&key);
        if existing.is_none()
            || existing.is_some_and(|credentials| credentials.password().is_none())
        {
            realms.insert(key, credentials.clone());
            return true;
        }

        false
    }
}
