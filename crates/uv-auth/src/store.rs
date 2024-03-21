use std::{collections::HashMap, sync::Mutex};

use netrc::Authenticator;
use tracing::warn;
use url::Url;

use crate::NetLoc;

#[derive(Clone, Debug, PartialEq)]
pub enum Credential {
    Basic(BasicAuthData),
    UrlEncoded(UrlAuthData),
}

impl Credential {
    pub fn username(&self) -> &str {
        match self {
            Credential::Basic(auth) => &auth.username,
            Credential::UrlEncoded(auth) => &auth.username,
        }
    }
    pub fn password(&self) -> Option<&str> {
        match self {
            Credential::Basic(auth) => auth.password.as_deref(),
            Credential::UrlEncoded(auth) => auth.password.as_deref(),
        }
    }
}

impl From<Authenticator> for Credential {
    fn from(auth: Authenticator) -> Self {
        Credential::Basic(BasicAuthData {
            username: auth.login,
            password: Some(auth.password),
        })
    }
}

// Used for URL encoded auth in User info
// <https://datatracker.ietf.org/doc/html/rfc3986#section-3.2.1>
#[derive(Clone, Debug, PartialEq)]
pub struct UrlAuthData {
    pub username: String,
    pub password: Option<String>,
}

impl UrlAuthData {
    pub fn apply_to_url(&self, mut url: Url) -> Url {
        url.set_username(&self.username)
            .unwrap_or_else(|()| warn!("Failed to set username"));
        url.set_password(self.password.as_deref())
            .unwrap_or_else(|()| warn!("Failed to set password"));
        url
    }
}

// HttpBasicAuth - Used for netrc and keyring auth
// <https://datatracker.ietf.org/doc/html/rfc7617>
#[derive(Clone, Debug, PartialEq)]
pub struct BasicAuthData {
    pub username: String,
    pub password: Option<String>,
}

pub struct AuthenticationStore {
    credentials: Mutex<HashMap<NetLoc, Option<Credential>>>,
}

impl Default for AuthenticationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthenticationStore {
    pub fn new() -> Self {
        Self {
            credentials: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, url: &Url) -> Option<Option<Credential>> {
        let netloc = NetLoc::from(url);
        let credentials = self.credentials.lock().unwrap();
        credentials.get(&netloc).cloned()
    }

    pub fn set(&self, url: &Url, auth: Option<Credential>) {
        let netloc = NetLoc::from(url);
        let mut credentials = self.credentials.lock().unwrap();
        credentials.insert(netloc, auth);
    }

    /// Store in-URL credentials for future use.
    pub fn save_from_url(&self, url: &Url) {
        let netloc = NetLoc::from(url);
        let mut credentials = self.credentials.lock().unwrap();
        if url.username().is_empty() {
            // No credentials to save
            return;
        }
        let auth = UrlAuthData {
            // Using the encoded username can break authentication when `@` is converted to `%40`
            // so we decode it for storage; RFC7617 does not explicitly say that authentication should
            // not be percent-encoded, but the omission of percent-encoding from all encoding discussion
            // indicates that it probably should not be done.
            username: urlencoding::decode(url.username())
                .expect("An encoded username should always decode")
                .into_owned(),
            password: url.password().map(str::to_string),
        };
        credentials.insert(netloc, Some(Credential::UrlEncoded(auth)));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn store_set_and_get() {
        let store = AuthenticationStore::new();
        let url = Url::parse("https://example1.com/simple/").unwrap();
        let not_set_res = store.get(&url);
        assert!(not_set_res.is_none());

        let found_first_url = Url::parse("https://example2.com/simple/first/").unwrap();
        let not_found_first_url = Url::parse("https://example3.com/simple/first/").unwrap();

        store.set(
            &found_first_url,
            Some(Credential::Basic(BasicAuthData {
                username: "u".to_string(),
                password: Some("p".to_string()),
            })),
        );
        store.set(&not_found_first_url, None);

        let found_second_url = Url::parse("https://example2.com/simple/second/").unwrap();
        let not_found_second_url = Url::parse("https://example3.com/simple/second/").unwrap();

        let found_res = store.get(&found_second_url);
        assert!(found_res.is_some());
        let found_res = found_res.unwrap();
        assert!(matches!(found_res, Some(Credential::Basic(_))));

        let not_found_res = store.get(&not_found_second_url);
        assert!(not_found_res.is_some());
        let not_found_res = not_found_res.unwrap();
        assert!(not_found_res.is_none());
    }

    #[test]
    fn store_save_from_url() {
        let store = AuthenticationStore::new();
        let url = Url::parse("https://u:p@example.com/simple/").unwrap();

        store.save_from_url(&url);

        let found_res = store.get(&url);
        assert!(found_res.is_some());
        let found_res = found_res.unwrap();
        assert!(matches!(found_res, Some(Credential::UrlEncoded(_))));

        let url = Url::parse("https://example2.com/simple/").unwrap();
        store.save_from_url(&url);
        let found_res = store.get(&url);
        assert!(found_res.is_none());
    }
}
