use std::{io::Write, process::Stdio};
use tokio::process::Command;
use tracing::{instrument, trace, warn};
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user_once;

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
    Dummy(Vec<(String, &'static str, &'static str)>),
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
    pub async fn fetch(&self, url: &DisplaySafeUrl, username: Option<&str>) -> Option<Credentials> {
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
            !username.map(str::is_empty).unwrap_or(false),
            "Should only use keyring with a non-empty username"
        );

        // Check the full URL first
        // <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L376C1-L379C14>
        trace!("Checking keyring for URL {url}");
        let mut credentials = match self.backend {
            KeyringProviderBackend::Subprocess => {
                self.fetch_subprocess(url.as_str(), username).await
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(ref store) => {
                Self::fetch_dummy(store, url.as_str(), username)
            }
        };
        // And fallback to a check for the host
        if credentials.is_none() {
            let host = if let Some(port) = url.port() {
                format!("{}:{}", url.host_str()?, port)
            } else {
                url.host_str()?.to_string()
            };
            trace!("Checking keyring for host {host}");
            credentials = match self.backend {
                KeyringProviderBackend::Subprocess => self.fetch_subprocess(&host, username).await,
                #[cfg(test)]
                KeyringProviderBackend::Dummy(ref store) => {
                    Self::fetch_dummy(store, &host, username)
                }
            };
        }

        credentials.map(|(username, password)| Credentials::basic(Some(username), Some(password)))
    }

    #[instrument(skip(self))]
    async fn fetch_subprocess(
        &self,
        service_name: &str,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        // https://github.com/pypa/pip/blob/24.0/src/pip/_internal/network/auth.py#L136-L141
        let mut command = Command::new("keyring");
        command.arg("get").arg(service_name);

        if let Some(username) = username {
            command.arg(username);
        } else {
            command.arg("--mode").arg("creds");
        }

        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // If we're using `--mode creds`, we need to capture the output in order to avoid
            // showing users an "unrecognized arguments: --mode" error; otherwise, we stream stderr
            // so the user has visibility into keyring's behavior if it's doing something slow
            .stderr(if username.is_some() {
                Stdio::inherit()
            } else {
                Stdio::piped()
            })
            .spawn()
            .inspect_err(|err| warn!("Failure running `keyring` command: {err}"))
            .ok()?;

        let output = child
            .wait_with_output()
            .await
            .inspect_err(|err| warn!("Failed to wait for `keyring` output: {err}"))
            .ok()?;

        if output.status.success() {
            // If we captured stderr, display it in case it's helpful to the user
            // TODO(zanieb): This was done when we added `--mode creds` support for parity with the
            // existing behavior, but it might be a better UX to hide this on success? It also
            // might be problematic that we're not streaming it. We could change this given some
            // user feedback.
            std::io::stderr().write_all(&output.stderr).ok();

            // On success, parse the newline terminated credentials
            let output = String::from_utf8(output.stdout)
                .inspect_err(|err| warn!("Failed to parse response from `keyring` command: {err}"))
                .ok()?;

            let (username, password) = if let Some(username) = username {
                // We're only expecting a password
                let password = output.trim_end();
                (username, password)
            } else {
                // We're expecting a username and password
                let mut lines = output.lines();
                let username = lines.next()?;
                let Some(password) = lines.next() else {
                    warn!(
                        "Got username without password for `{service_name}` from `keyring` command"
                    );
                    return None;
                };
                (username, password)
            };

            if password.is_empty() {
                // We allow this for backwards compatibility, but it might be better to return
                // `None` instead if there's confusion from users — we haven't seen this in practice
                // yet.
                warn!("Got empty password for `{username}@{service_name}` from `keyring` command");
            }

            Some((username.to_string(), password.to_string()))
        } else {
            // On failure, no password was available
            let stderr = std::str::from_utf8(&output.stderr).ok()?;
            if stderr.contains("unrecognized arguments: --mode") {
                // N.B. We do not show the `service_name` here because we'll show the warning twice
                //      otherwise, once for the URL and once for the realm.
                warn_user_once!(
                    "Attempted to fetch credentials using the `keyring` command, but it does not support `--mode creds`; upgrade to `keyring>=v25.2.1` for support or provide a username"
                );
            } else if username.is_none() {
                // If we captured stderr, display it in case it's helpful to the user
                std::io::stderr().write_all(&output.stderr).ok();
            }
            None
        }
    }

    #[cfg(test)]
    fn fetch_dummy(
        store: &Vec<(String, &'static str, &'static str)>,
        service_name: &str,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        store.iter().find_map(|(service, user, password)| {
            if service == service_name && username.is_none_or(|username| username == *user) {
                Some(((*user).to_string(), (*password).to_string()))
            } else {
                None
            }
        })
    }

    /// Create a new provider with [`KeyringProviderBackend::Dummy`].
    #[cfg(test)]
    pub fn dummy<S: Into<String>, T: IntoIterator<Item = (S, &'static str, &'static str)>>(
        iter: T,
    ) -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(
                iter.into_iter()
                    .map(|(service, username, password)| (service.into(), username, password))
                    .collect(),
            ),
        }
    }

    /// Create a new provider with no credentials available.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use url::Url;

    #[tokio::test]
    async fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(
            keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some("user")),
        )
        .catch_unwind()
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(
            keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some(url.username())),
        )
        .catch_unwind()
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_with_empty_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::empty();
        // Panics due to debug assertion; returns `None` in production
        let result = std::panic::AssertUnwindSafe(
            keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some(url.username())),
        )
        .catch_unwind()
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_no_auth() {
        let url = Url::parse("https://example.com").unwrap();
        let url = DisplaySafeUrl::ref_cast(&url);
        let keyring = KeyringProvider::empty();
        let credentials = keyring.fetch(url, Some("user"));
        assert!(credentials.await.is_none());
    }

    #[tokio::test]
    async fn fetch_url() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        assert_eq!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("test").unwrap()),
                    Some("user")
                )
                .await,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([("other.com", "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await;
        assert_eq!(credentials, None);
    }

    #[tokio::test]
    async fn fetch_url_prefers_url_to_host() {
        let url = Url::parse("https://example.com/").unwrap();
        let keyring = KeyringProvider::dummy([
            (url.join("foo").unwrap().as_str(), "user", "password"),
            (url.host_str().unwrap(), "user", "other-password"),
        ]);
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("foo").unwrap()),
                    Some("user")
                )
                .await,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
        assert_eq!(
            keyring
                .fetch(
                    DisplaySafeUrl::ref_cast(&url.join("bar").unwrap()),
                    Some("user")
                )
                .await,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("other-password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
            .await;
        assert_eq!(
            credentials,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_no_username() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "user", "password")]);
        let credentials = keyring.fetch(DisplaySafeUrl::ref_cast(&url), None).await;
        assert_eq!(
            credentials,
            Some(Credentials::basic(
                Some("user".to_string()),
                Some("password".to_string())
            ))
        );
    }

    #[tokio::test]
    async fn fetch_url_username_no_match() {
        let url = Url::parse("https://example.com").unwrap();
        let keyring = KeyringProvider::dummy([(url.host_str().unwrap(), "foo", "password")]);
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("bar"))
            .await;
        assert_eq!(credentials, None);

        // Still fails if we have `foo` in the URL itself
        let url = Url::parse("https://foo@example.com").unwrap();
        let credentials = keyring
            .fetch(DisplaySafeUrl::ref_cast(&url), Some("bar"))
            .await;
        assert_eq!(credentials, None);
    }
}
