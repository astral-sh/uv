use lazy_static::lazy_static;
use std::{collections::HashMap, sync::Mutex};

use netrc::Authenticator;
use tracing::warn;
use url::Url;

use crate::NetLoc;

lazy_static! {
    // Store credentials for NetLoc
    static ref PASSWORDS: Mutex<HashMap<NetLoc, Option<Credential>>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Debug, PartialEq)]
pub enum Credential {
    Basic(BasicAuthData),
    UrlEncoded(UrlAuthData),
}

impl Credential {
    pub fn username(&self) -> String {
        match self {
            Credential::Basic(auth) => auth.username.clone(),
            Credential::UrlEncoded(auth) => auth.username.clone(),
        }
    }
    pub fn password(&self) -> Option<String> {
        match self {
            Credential::Basic(auth) => auth.password.clone(),
            Credential::UrlEncoded(auth) => auth.password.clone(),
        }
    }
}

impl From<&Authenticator> for Credential {
    fn from(auth: &Authenticator) -> Self {
        Credential::Basic(BasicAuthData {
            username: auth.login.clone(),
            password: Some(auth.password.clone()),
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

pub struct AuthenticationStore;

impl AuthenticationStore {
    pub fn get(url: &Url) -> Option<Option<Credential>> {
        let netloc = NetLoc::from(url);
        let passwords = PASSWORDS.lock().unwrap();
        passwords.get(&netloc).cloned()
    }

    pub fn set(url: &Url, auth: Option<Credential>) {
        let netloc = NetLoc::from(url);
        let mut passwords = PASSWORDS.lock().unwrap();
        passwords.insert(netloc, auth);
    }

    /// Copy authentication from one URL to another URL if applicable.
    pub fn with_url_encoded_auth(url: Url) -> Url {
        let netloc = NetLoc::from(&url);
        let passwords = PASSWORDS.lock().unwrap();
        if let Some(Some(Credential::UrlEncoded(url_auth))) = passwords.get(&netloc) {
            url_auth.apply_to_url(url)
        } else {
            url
        }
    }

    pub fn save_from_url(url: &Url) {
        let netloc = NetLoc::from(url);
        let mut passwords = PASSWORDS.lock().unwrap();
        if url.username().is_empty() {
            // No credentials to save
            return;
        }
        let auth = UrlAuthData {
            username: url.username().to_string(),
            password: url.password().map(str::to_string),
        };
        passwords.insert(netloc, Some(Credential::UrlEncoded(auth)));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // NOTE: Because tests run in parallel, it is imperative to use different URLs for each
    #[test]
    fn set_get_work() {
        let url = Url::parse("https://test1example1.com/simple/").unwrap();
        let not_set_res = AuthenticationStore::get(&url);
        assert!(not_set_res.is_none());

        let found_first_url = Url::parse("https://test1example2.com/simple/first/").unwrap();
        let not_found_first_url = Url::parse("https://test1example3.com/simple/first/").unwrap();

        AuthenticationStore::set(
            &found_first_url,
            Some(Credential::Basic(BasicAuthData {
                username: "u".to_string(),
                password: Some("p".to_string()),
            })),
        );
        AuthenticationStore::set(&not_found_first_url, None);

        let found_second_url = Url::parse("https://test1example2.com/simple/second/").unwrap();
        let not_found_second_url = Url::parse("https://test1example3.com/simple/second/").unwrap();

        let found_res = AuthenticationStore::get(&found_second_url);
        assert!(found_res.is_some());
        let found_res = found_res.unwrap();
        assert!(matches!(found_res, Some(Credential::Basic(_))));

        let not_found_res = AuthenticationStore::get(&not_found_second_url);
        assert!(not_found_res.is_some());
        let not_found_res = not_found_res.unwrap();
        assert!(not_found_res.is_none());
    }

    #[test]
    fn with_url_encoded_auth_works() {
        let url = Url::parse("https://test2example.com/simple/").unwrap();
        let auth = Credential::UrlEncoded(UrlAuthData {
            username: "u".to_string(),
            password: Some("p".to_string()),
        });

        AuthenticationStore::set(&url, Some(auth.clone()));

        let url = AuthenticationStore::with_url_encoded_auth(url);
        assert_eq!(url.username(), "u");
        assert_eq!(url.password(), Some("p"));
    }

    #[test]
    fn save_from_url_works() {
        let url = Url::parse("https://u:p@test3example.com/simple/").unwrap();

        AuthenticationStore::save_from_url(&url);

        let found_res = AuthenticationStore::get(&url);
        assert!(found_res.is_some());
        let found_res = found_res.unwrap();
        assert!(matches!(found_res, Some(Credential::UrlEncoded(_))));
    }
}
