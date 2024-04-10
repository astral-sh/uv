use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use url::Url;

use crate::credentials::Credentials;
use crate::NetLoc;

pub struct AuthenticationStore {
    // `None` is used to track that a fetch was attempted for a `NetLoc` but no credentials were found
    store: Mutex<HashMap<NetLoc, Option<Arc<Credentials>>>>,
}

impl Default for AuthenticationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthenticationStore {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    /// Retrieve stored credentials for a URL, if any.
    pub(crate) fn get(&self, url: &Url) -> Option<Arc<Credentials>> {
        let netloc = NetLoc::from(url);
        let store = self.store.lock().unwrap();
        if let Some(fetched) = store.get(&netloc) {
            fetched.clone()
        } else {
            None
        }
    }

    /// Set the stored credentials for a URL.
    pub(crate) fn set(&self, url: &Url, credentials: Credentials) {
        let netloc = NetLoc::from(url);
        let mut store = self.store.lock().unwrap();
        store.insert(netloc, Some(Arc::new(credentials)));
    }

    /// Populate the store with credentials on a URL, if there are any.
    ///
    /// If there are no credentials on the URL, the store will not be updated.
    ///
    /// Returns `true` if the store was updated.
    pub(crate) fn set_from_url(&self, url: &Url) -> bool {
        let netloc = NetLoc::from(url);
        let mut store = self.store.lock().unwrap();
        if let Some(credentials) = Credentials::from_url(url) {
            store.insert(netloc, Some(Arc::new(credentials)));
            true
        } else {
            false
        }
    }

    /// Populate the store with credentials in a request, if there are any.
    ///
    /// If there are no credentials in the request, the store will not be updated.
    /// If there are already credentials in the store, they will not be updated.
    /// If the attached credentials do not conform to the HTTP Basic Authentication scheme,
    /// they will be ignored.
    ///
    /// Returns `true` if the store was updated.
    pub(crate) fn set_default_from_request(&self, request: &reqwest::Request) -> bool {
        let netloc = NetLoc::from(request.url());
        let mut store = self.store.lock().unwrap();
        if store.contains_key(&netloc) {
            return false;
        }
        if let Some(credentials) = Credentials::from_request(request) {
            store.insert(netloc, Some(Arc::new(credentials)));
            true
        } else {
            false
        }
    }

    /// Returns `true` if we do not have credentials for the given URL and we not
    /// have attempted to fetch them before.
    pub(crate) fn should_attempt_fetch(&self, url: &Url) -> bool {
        let netloc = NetLoc::from(url);
        let store = self.store.lock().unwrap();
        store.get(&netloc).is_none()
    }

    /// Track that fetching credentials for the given URL was attempted.
    ///
    /// If already set or if credentials have been populated for this URL, returns `false`.
    pub(crate) fn set_fetch_attempted(&self, url: &Url) -> bool {
        let netloc = NetLoc::from(url);
        let mut store = self.store.lock().unwrap();
        if store.get(&netloc).is_none() {
            store.insert(netloc, None);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_does_not_exist() {
        let store = AuthenticationStore::new();

        // An empty store should return `None`
        let url = Url::parse("https://example.com/simple/").unwrap();
        assert!(store.get(&url).is_none());
    }

    #[test]
    fn set_get_username() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let credentials = Credentials::new("u".to_string(), None);
        store.set(url, credentials.clone());
        assert_eq!(store.get(url).as_deref(), Some(&credentials));
    }

    #[test]
    fn set_get_password() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let credentials = Credentials::new("".to_string(), Some("p".to_string()));
        store.set(url, credentials.clone());
        assert_eq!(store.get(url).as_deref(), Some(&credentials));
    }

    #[test]
    fn set_get_username_and_password() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let credentials = Credentials::new("".to_string(), Some("p".to_string()));
        store.set(url, credentials.clone());
        assert_eq!(store.get(url).as_deref(), Some(&credentials));
    }

    #[test]
    fn set_from_url_username_and_password() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("u").unwrap();
        auth_url.set_password(Some("p")).unwrap();
        store.set_from_url(&auth_url);
        assert_eq!(
            store.get(url).as_deref(),
            Some(&Credentials::new("u".to_string(), Some("p".to_string())))
        );
    }

    #[test]
    fn set_from_url_password() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_password(Some("p")).unwrap();
        store.set_from_url(&auth_url);
        assert_eq!(
            store.get(url).as_deref(),
            Some(&Credentials::new("".to_string(), Some("p".to_string())))
        );
    }

    #[test]
    fn set_from_url_username() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("u").unwrap();
        store.set_from_url(&auth_url);
        assert_eq!(
            store.get(url).as_deref(),
            Some(&Credentials::new("u".to_string(), None,))
        );
    }

    #[test]
    fn set_from_url_no_credentials() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        assert!(!store.set_from_url(url));
        assert_eq!(store.get(url), None);
    }

    #[test]
    fn set_from_url_does_not_clear_existing() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let credentials = Credentials::new("".to_string(), Some("p".to_string()));
        store.set(url, credentials.clone());
        assert_eq!(store.get(url).as_deref(), Some(&credentials));

        // Set from a url with no credentials, should return `false`
        assert!(!store.set_from_url(url));
        // The credentials should not be removed
        assert_eq!(store.get(url).as_deref(), Some(&credentials));
    }

    #[test]
    fn should_attempt_fetch() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let credentials = Credentials::new("u".to_string(), None);
        assert!(store.should_attempt_fetch(url));
        store.set(url, credentials.clone());
        assert!(!store.should_attempt_fetch(url));
    }
    #[test]
    fn set_fetch_attempted() {
        let store = AuthenticationStore::new();
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        assert!(store.should_attempt_fetch(url));
        assert!(store.set_fetch_attempted(url));
        assert!(!store.should_attempt_fetch(url));
    }
}
