use std::process::Stdio;
use tokio::process::Command;
use tracing::{instrument, trace, warn};
use url::Url;

use crate::credentials::Credentials;

/// A backend for retrieving credentials from a keyring.
///
/// See pip's implementation for reference
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
#[derive(Debug)]
pub struct KeyringProvider {
    backend: KeyringProviderBackend,
}

#[derive(Debug)]
pub(crate) enum KeyringProviderBackend {
    /// Use the `keyring` command to fetch credentials.
    Subprocess,
    #[cfg(test)]
    Dummy(std::collections::HashMap<(String, &'static str), &'static str>),
}

impl KeyringProvider {
    /// Create a new [`KeyringProvider::Subprocess`].
    pub fn subprocess() -> Self {
        Self {
            backend: KeyringProviderBackend::Subprocess,
        }
    }

    /// Fetch credentials for the given [`Url`] from the keyring.
    ///
    /// Returns [`None`] if no password was found for the username or if any errors
    /// are encountered in the keyring backend.
    #[instrument(skip_all, fields(url = % url.to_string(), username))]
    pub async fn fetch(&self, url: &Url, username: &str) -> Option<Credentials> {
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

        // Check the full URL first
        // <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L376C1-L379C14>
        trace!("Checking keyring for URL {url}");
        let mut password = match self.backend {
            KeyringProviderBackend::Subprocess => {
                self.fetch_subprocess(url.as_str(), username).await
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(ref store) => {
                Self::fetch_dummy(store, url.as_str(), username)
            }
        };
        // And fallback to a check for the host
        if password.is_none() {
            let host = if let Some(port) = url.port() {
                format!("{}:{}", url.host_str()?, port)
            } else {
                url.host_str()?.to_string()
            };
            trace!("Checking keyring for host {host}");
            password = match self.backend {
                KeyringProviderBackend::Subprocess => self.fetch_subprocess(&host, username).await,
                #[cfg(test)]
                KeyringProviderBackend::Dummy(ref store) => {
                    Self::fetch_dummy(store, &host, username)
                }
            };
        }

        password.map(|password| Credentials::new(Some(username.to_string()), Some(password)))
    }

    #[instrument(skip(self))]
    async fn fetch_subprocess(&self, service_name: &str, username: &str) -> Option<String> {
        // https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/auth.py#L136-L141
        let child = Command::new("keyring")
            .arg("get")
            .arg(service_name)
            .arg(username)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .inspect_err(|err| warn!("Failure running `keyring` command: {err}"))
            .ok()?;

        let output = child
            .wait_with_output()
            .await
            .inspect_err(|err| warn!("Failed to wait for `keyring` output: {err}"))
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
        store: &std::collections::HashMap<(String, &'static str), &'static str>,
        service_name: &str,
        username: &str,
    ) -> Option<String> {
        store
            .get(&(service_name.to_string(), username))
            .map(|password| (*password).to_string())
    }

    /// Create a new provider with [`KeyringProviderBackend::Dummy`].
    #[cfg(test)]
    pub fn dummy<S: Into<String>, T: IntoIterator<Item = ((S, &'static str), &'static str)>>(
        iter: T,
    ) -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(
                iter.into_iter()
                    .map(|((service, username), password)| ((service.into(), username), password))
                    .collect(),
            ),
        }
    }

    /// Create a new provider with no credentials available.
    #[cfg(test)]
    pub fn empty() -> Self {
        use std::collections::HashMap;

        Self {
            backend: KeyringProviderBackend::Dummy(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;

    #[tokio::test]
    async fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, "user"))
            .catch_unwind()
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, url.username()))
            .catch_unwind()
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_with_no_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, url.username()))
            .catch_unwind()
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_no_auth() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        let credentials = keyring.fetch(&url, "user");
        assert!(credentials.await.is_none());
    }

    #[tokio::test]
    async fn fetch_url() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
        assert_eq!(
            keyring.fetch(&url, "user").await,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url.join("test").unwrap(), "user").await,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(("other.com", "user"), "password")]);
        let credentials = keyring.fetch(&url, "user").await;
        assert_eq!(credentials, None);
    }

    #[tokio::test]
    async fn fetch_url_prefers_url_to_host() {
        let url = Url::parse("https://example.com/").unwrap();
        let keyring = KeyringProvider::dummy([
            ((url.join("foo").unwrap().as_str(), "user"), "password"),
            ((url.host_str().unwrap(), "user"), "other-password"),
        ]);
        assert_eq!(
            keyring.fetch(&url.join("foo").unwrap(), "user").await,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url, "user").await,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
        assert_eq!(
            keyring.fetch(&url.join("bar").unwrap(), "user").await,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
        let credentials = keyring.fetch(&url, "user").await;
        assert_eq!(
            credentials,
            Some(Credentials::new(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "foo"), "password")]);
        let credentials = keyring.fetch(&url, "bar").await;
        assert_eq!(credentials, None);

        // Still fails if we have `foo` in the URL itself
        let url = Url::parse("https://foo@example.com").unwrap();
        let credentials = keyring.fetch(&url, "bar").await;
        assert_eq!(credentials, None);
    }
}
