use std::{collections::HashSet, process::Command, sync::Mutex};

use tracing::{debug, instrument, warn};
use url::Url;

use crate::credentials::Credentials;

/// A backend for retrieving credentials from a keyring.
///
/// See pip's implementation for reference
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
#[derive(Debug)]
pub struct KeyringProvider {
    /// Tracks host and username pairs with no credentials to avoid repeated queries.
    cache: Mutex<HashSet<(String, String)>>,
    backend: KeyringProviderBackend,
}

#[derive(Debug)]
pub enum KeyringProviderBackend {
    /// Use the `keyring` command to fetch credentials.
    Subprocess,
    #[cfg(test)]
    Dummy(std::collections::HashMap<(String, &'static str), &'static str>),
}

impl KeyringProvider {
    /// Create a new [`KeyringProvider::Subprocess`].
    pub fn subprocess() -> Self {
        Self {
            cache: Mutex::new(HashSet::new()),
            backend: KeyringProviderBackend::Subprocess,
        }
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

        let host = url.host_str()?;

        // Avoid expensive lookups by tracking previous attempts with no credentials.
        // N.B. We cache missing credentials per host so no credentials are found for
        //      a host but would return credentials for some other URL in the same realm
        //      we may not find the credentials depending on which URL we see first.
        //      This behavior avoids adding ~80ms to every request when the subprocess keyring
        //      provider is being used, but makes assumptions about the typical keyring
        //      use-cases.
        let mut cache = self.cache.lock().unwrap();

        let key = (host.to_string(), username.to_string());
        if cache.contains(&key) {
            debug!(
                "Skipping keyring lookup for {username} at {host}, already attempted and found no credentials."
            );
            return None;
        }

        // Check the full URL first
        // <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L376C1-L379C14>
        let mut password = match self.backend {
            KeyringProviderBackend::Subprocess => self.fetch_subprocess(url.as_str(), username),
            #[cfg(test)]
            KeyringProviderBackend::Dummy(ref store) => {
                self.fetch_dummy(store, url.as_str(), username)
            }
        };
        // And fallback to a check for the host
        if password.is_none() {
            password = match self.backend {
                KeyringProviderBackend::Subprocess => self.fetch_subprocess(host, username),
                #[cfg(test)]
                KeyringProviderBackend::Dummy(ref store) => self.fetch_dummy(store, host, username),
            };
        }

        if password.is_none() {
            cache.insert(key);
        }

        password.map(|password| Credentials::new(Some(username.to_string()), Some(password)))
    }

    #[instrument]
    fn fetch_subprocess(&self, service_name: &str, username: &str) -> Option<String> {
        let output = Command::new("keyring")
            .arg("get")
            .arg(service_name)
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
        store: &std::collections::HashMap<(String, &'static str), &'static str>,
        service_name: &str,
        username: &str,
    ) -> Option<String> {
        store
            .get(&(service_name.to_string(), username))
            .map(|password| password.to_string())
    }

    /// Create a new provider with [`KeyringProviderBackend::Dummy`].
    #[cfg(test)]
    pub fn dummy<S: Into<String>, T: IntoIterator<Item = ((S, &'static str), &'static str)>>(
        iter: T,
    ) -> Self {
        use std::collections::HashMap;

        Self {
            cache: Mutex::new(HashSet::new()),
            backend: KeyringProviderBackend::Dummy(HashMap::from_iter(
                iter.into_iter()
                    .map(|((service, username), password)| ((service.into(), username), password)),
            )),
        }
    }

    /// Create a new provider with no credentials available.
    #[cfg(test)]
    pub fn empty() -> Self {
        use std::collections::HashMap;

        Self {
            cache: Mutex::new(HashSet::new()),
            backend: KeyringProviderBackend::Dummy(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, "user"));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, url.username()));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_with_no_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::catch_unwind(|| keyring.fetch(&url, url.username()));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_url_no_auth() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        let credentials = keyring.fetch(&url, "user");
        assert!(credentials.is_none());
    }

    #[test]
    fn fetch_url() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
        assert_eq!(
            keyring.fetch(&url, "user"),
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url.join("test").unwrap(), "user"),
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[test]
    fn fetch_url_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(("other.com", "user"), "password")]);
        let credentials = keyring.fetch(&url, "user");
        assert_eq!(credentials, None);
    }

    #[test]
    fn fetch_url_prefers_url_to_host() {
        let url = Url::parse("https://example.com/").unwrap();
        let keyring = KeyringProvider::dummy([
            ((url.join("foo").unwrap().as_str(), "user"), "password"),
            ((url.host_str().unwrap(), "user"), "other-password"),
        ]);
        assert_eq!(
            keyring.fetch(&url.join("foo").unwrap(), "user"),
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url, "user"),
            Some(Credentials::new(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url.join("bar").unwrap(), "user"),
            Some(Credentials::new(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
    }

    /// Demonstrates "incorrect" behavior in our cache which avoids an expensive fetch of
    /// credentials for _every_ request URL at the cost of inconsistent behavior when
    /// credentials are not scoped to a realm.
    #[test]
    fn fetch_url_caches_based_on_host() {
        let url = Url::parse("https://example.com/").unwrap();
        let keyring =
            KeyringProvider::dummy([((url.join("foo").unwrap().as_str(), "user"), "password")]);

        // If we attempt an unmatching URL first...
        assert_eq!(keyring.fetch(&url.join("bar").unwrap(), "user"), None);

        // ... we will cache the missing credentials on subsequent attempts
        assert_eq!(keyring.fetch(&url.join("foo").unwrap(), "user"), None);
    }

    #[test]
    fn fetch_url_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
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
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "foo"), "password")]);
        let credentials = keyring.fetch(&url, "bar");
        assert_eq!(credentials, None);

        // Still fails if we have `foo` in the URL itself
        let url = Url::parse("https://foo@example.com").unwrap();
        let credentials = keyring.fetch(&url, "bar");
        assert_eq!(credentials, None);
    }
}
