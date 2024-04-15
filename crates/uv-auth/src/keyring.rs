use std::{collections::HashSet, process::Command, sync::Mutex};

use tracing::{debug, instrument, warn};
use url::Url;

use crate::credentials::Credentials;

/// A backend for retrieving credentials from a keyring.
///
/// See pip's implementation for reference
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
#[derive(Debug)]
pub enum KeyringProvider {
    /// Use the `keyring` command to fetch credentials.
    ///
    /// Tracks attempted URL and Strin
    Subprocess(Mutex<HashSet<(Url, String)>>),
    #[cfg(test)]
    Dummy(std::collections::HashMap<(Url, &'static str), &'static str>),
}

impl KeyringProvider {
    /// Create a new [`KeyringProvider::Subprocess`].
    pub fn subprocess() -> Self {
        Self::Subprocess(Mutex::new(HashSet::new()))
    }

    /// Fetch credentials for the given [`Url`] from the keyring.
    ///
    /// Returns [`None`] if no password was found for the username or if any errors
    /// are encountered in the keyring backend.
    pub(crate) fn fetch(&self, url: &Url, username: &str) -> Option<Credentials> {
        // Validate the request
        debug_assert!(
            url.host_str().is_some(),
            "Should only use keyring for urls with host"
        );
        debug_assert!(
            url.password().is_none(),
            "Should only use keyring for urls without a password"
        );
        debug_assert!(
            !username.is_empty(),
            "Should only use keyring with a username"
        );

        let password = match self {
            Self::Subprocess(attempts) => self.fetch_subprocess(attempts, url, username),
            #[cfg(test)]
            Self::Dummy(store) => self.fetch_dummy(store, url, username),
        };

        password.map(|password| Credentials::new(Some(username.to_string()), Some(password)))
    }

    #[instrument]
    fn fetch_subprocess(
        &self,
        attempts: &Mutex<HashSet<(Url, String)>>,
        url: &Url,
        username: &str,
    ) -> Option<String> {
        // Avoid expensive subprocess calls by tracking previous attempts
        let mut attempts = attempts.lock().unwrap();
        if !attempts.insert((url.to_owned(), username.to_string())) {
            debug!("Skipping subprocess lookup for {username} at {url}, already attempted.");
            return None;
        }

        let output = Command::new("keyring")
            .arg("get")
            .arg(url.to_string())
            .arg(username)
            .output()
            .inspect_err(|err| warn!("Failure running `keyring` command: {err}"))
            .ok()?;

        if output.status.success() {
            // On success, parse the newline terminated password
            String::from_utf8(output.stdout)
                .inspect_err(|err| warn!("Failed to parse response from `keyring` command: {err}"))
                .ok()
                .map(|password| password.trim_end().to_string())
        } else {
            // On failure, no password was available
            None
        }
    }

    #[cfg(test)]
    fn fetch_dummy(
        &self,
        store: &std::collections::HashMap<(Url, &'static str), &'static str>,
        url: &Url,
        username: &str,
    ) -> Option<String> {
        store
            .get(&(url.clone(), username))
            .map(|password| password.to_string())
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::Dummy(HashMap::default());
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, "user"));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::Dummy(HashMap::default());
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, url.username()));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_with_no_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::Dummy(HashMap::default());
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, url.username()));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_no_auth() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::Dummy(HashMap::default());
        let credentials = keyring.fetch(&url, "user");
        assert!(credentials.is_none());
    }

    #[test]
    fn fetch_url() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring =
            KeyringProvider::Dummy(HashMap::from_iter([((url.clone(), "user"), "password")]));
        let credentials = keyring.fetch(&url, "user");
        assert_eq!(
            credentials,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[test]
    fn fetch_url_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::Dummy(HashMap::from_iter([(
            (Url::parse("https://other.com").unwrap(), "user"),
            "password",
        )]));
        let credentials = keyring.fetch(&url, "user");
        assert_eq!(credentials, None);
    }

    #[test]
    fn fetch_url_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring =
            KeyringProvider::Dummy(HashMap::from_iter([((url.clone(), "user"), "password")]));
        let credentials = keyring.fetch(&url, "user");
        assert_eq!(
            credentials,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[test]
    fn fetch_url_username_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring =
            KeyringProvider::Dummy(HashMap::from_iter([((url.clone(), "foo"), "password")]));
        let credentials = keyring.fetch(&url, "bar");
        assert_eq!(credentials, None);

        // Still fails if we have `foo` in the URL itself
        let url = Url::parse("https://foo@example.com").unwrap();
        let credentials = keyring.fetch(&url, "bar");
        assert_eq!(credentials, None);
    }
}
