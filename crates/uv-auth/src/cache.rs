use std::sync::Arc;
use std::{collections::HashMap, sync::Mutex};

use crate::credentials::Credentials;
use crate::NetLoc;
use reqwest::Request;
use tracing::debug;

pub struct CredentialsCache {
    store: Mutex<HashMap<(NetLoc, Option<String>), Arc<Credentials>>>,
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

    /// Return the credentials that should be used for a request, if any.
    ///
    /// If the request contains credentials, they may be added to the cache.
    ///
    /// Generally this prefers the credentials already attached to the request, but
    /// if the request has credentials with just a username we may still attempt to
    /// find a password.
    pub(crate) fn credentials_for_request(&self, request: &Request) -> Option<Arc<Credentials>> {
        let netloc = NetLoc::from(request.url());
        let mut store = self.store.lock().unwrap();

        let credentials = Credentials::from_request(request);

        if let Some(credentials) = credentials {
            debug!("Found credentials on request, checking if cache should be updated...");

            if credentials.username().is_some() {
                let existing = store.get(&(netloc.clone(), None));

                // Replace the existing entry for the "no username" case if
                //  - There is no entry
                //  - The entry exists but has no password (and we have a password now)
                if existing.is_none()
                    || (credentials.password().is_some()
                        && existing.is_some_and(|credentials| credentials.password().is_none()))
                {
                    debug!("Updating cached credentials for {netloc:?} with no username");
                    store.insert((netloc.clone(), None), Arc::new(credentials.clone()));
                }
            }

            let existing = store.get(&(netloc.clone(), credentials.username().map(str::to_string)));
            if existing.is_none()
                || (credentials.password().is_some()
                    && existing.is_some_and(|credentials| credentials.password().is_none()))
            {
                debug!(
                    "Updating cached credentials for {netloc:?} with username {:?}",
                    credentials.username()
                );
                store.insert(
                    (netloc.clone(), credentials.username().map(str::to_string)),
                    Arc::new(credentials.clone()),
                );
                Some(Arc::new(credentials))
            } else if credentials.password().is_none() {
                debug!("Using cached credentials for request {existing:?}");
                existing.cloned()
            } else {
                Some(Arc::new(credentials))
            }
        } else {
            debug!("No credentials on request, checking cache...");
            let credentials = store.get(&(netloc.clone(), None)).cloned();
            if credentials.is_some() {
                debug!("Found cached credentials: {credentials:?}");
            } else {
                debug!("No credentials in cache");
            }
            credentials
        }
    }
}

#[cfg(test)]
mod test {

    // #[test]
    // fn get_does_not_exist() {
    //     let store = AuthenticationStore::new();

    //     // An empty store should return `None`
    //     let url = Url::parse("https://example.com/simple/").unwrap();
    //     assert!(store.get(&url, None).is_none());
    // }

    // #[test]
    // fn set_get_username() {
    //     let store = AuthenticationStore::new();
    //     let url = &Url::parse("https://example.com/simple/first/").unwrap();
    //     let credentials = Credentials::new(Some("user".to_string()), None);
    //     store.set(url, credentials.clone());
    //     assert_eq!(
    //         store.get(url, Some("user".to_string())).as_deref(),
    //         Some(&credentials),
    //         "Credentials should be retrieved"
    //     );
    //     assert_eq!(
    //         store.get(url, Some("other_user".to_string())).as_deref(),
    //         None,
    //         "Another username should not match"
    //     );
    //     assert_eq!(
    //         store.get(url, None).as_deref(),
    //         Some(&credentials),
    //         "When no username is provided, we should return the first match"
    //     );
    // }

    // #[test]
    // fn set_get_password() {
    //     let store = AuthenticationStore::new();
    //     let url = &Url::parse("https://example.com/simple/first/").unwrap();
    //     let credentials = Credentials::new(None, Some("p".to_string()));
    //     store.set(url, credentials.clone());
    //     assert_eq!(store.get(url, None).as_deref(), Some(&credentials));
    // }

    // #[test]
    // fn set_get_username_and_password() {
    //     let store = AuthenticationStore::new();
    //     let url = &Url::parse("https://example.com/simple/first/").unwrap();
    //     let credentials = Credentials::new(None, Some("p".to_string()));
    //     store.set(url, credentials.clone());
    //     assert_eq!(store.get(url, None).as_deref(), Some(&credentials));
    // }
}
