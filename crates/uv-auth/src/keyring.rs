use std::{io::Write, process::Stdio};

use tokio::process::Command;
use tracing::{instrument, trace, warn};
use uv_fs::LockedFileError;
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user_once;

use crate::credentials::Credentials;
use crate::service::{Service, ServiceParseError};

mod native;

/// A backend for retrieving credentials from a keyring.
///
/// See pip's implementation for reference
/// <https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/network/auth.py#L102>
#[derive(Debug)]
pub struct KeyringProvider {
    backend: KeyringProviderBackend,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Keyring(#[from] uv_keyring::Error),

    #[error("Stored credentials in the system keyring are corrupt")]
    CorruptStoredCredentials(#[source] serde_json::Error),

    #[error("Failed to serialize credentials for the system keyring")]
    SerializeStoredCredentials(#[source] serde_json::Error),

    #[error("Failed to prepare lock directory for native credential store")]
    NativeLockDirectory(#[source] std::io::Error),

    #[error("Failed to acquire lock for native credential store")]
    NativeLock(#[source] LockedFileError),

    #[error("Invalid service URL for native credential storage")]
    InvalidService(#[source] ServiceParseError),

    #[error("Credential service does not match the locked realm")]
    MismatchedRealm,

    #[error("Multiple credentials found for URL '{0}', specify which username to use")]
    AmbiguousUsername(DisplaySafeUrl),

    #[error("Native credential storage requires a username")]
    MissingUsername,

    #[error("Native credential storage requires a password")]
    MissingPassword,

    #[error("The '{0}' keyring provider does not support storing credentials")]
    StoreUnsupported(&'static str),

    #[error("The '{0}' keyring provider does not support removing credentials")]
    RemoveUnsupported(&'static str),
}

#[derive(Debug)]
enum KeyringProviderBackend {
    /// Use a native system keyring integration for credentials.
    Native,
    /// Use the external `keyring` command for credentials.
    Subprocess,
    #[cfg(test)]
    Dummy(Vec<(String, &'static str, &'static str)>),
}

impl KeyringProviderBackend {
    fn name(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Subprocess => "subprocess",
            #[cfg(test)]
            Self::Dummy(_) => "dummy",
        }
    }
}

impl std::fmt::Display for KeyringProviderBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

impl KeyringProvider {
    /// Create a native system keyring provider.
    pub(crate) fn native() -> Self {
        Self {
            backend: KeyringProviderBackend::Native,
        }
    }

    /// Create a subprocess keyring provider.
    pub fn subprocess() -> Self {
        Self {
            backend: KeyringProviderBackend::Subprocess,
        }
    }

    /// Store credentials for the given [`DisplaySafeUrl`] in the keyring.
    ///
    /// Only the native keyring provider is supported at this time.
    #[instrument(skip_all, fields(url = %url, username))]
    pub async fn store(
        &self,
        url: &DisplaySafeUrl,
        credentials: &Credentials,
    ) -> Result<(), Error> {
        if credentials.username().is_none() {
            return Err(Error::MissingUsername);
        }
        if credentials.password().is_none() {
            return Err(Error::MissingPassword);
        }

        let url = url.without_credentials().into_owned();
        let service = Service::try_from(url).map_err(Error::InvalidService)?;

        match &self.backend {
            KeyringProviderBackend::Native => native::store(&service, credentials).await,
            KeyringProviderBackend::Subprocess => Err(Error::StoreUnsupported(self.backend.name())),
            #[cfg(test)]
            KeyringProviderBackend::Dummy(_) => Err(Error::StoreUnsupported(self.backend.name())),
        }
    }

    /// Remove credentials for the given [`DisplaySafeUrl`] from the keyring.
    ///
    /// Only the native keyring provider is supported at this time.
    #[instrument(skip_all, fields(url = %url, username))]
    pub async fn remove(&self, url: &DisplaySafeUrl, username: &str) -> Result<(), Error> {
        let url = url.without_credentials().into_owned();
        let service = Service::try_from(url).map_err(Error::InvalidService)?;

        match &self.backend {
            KeyringProviderBackend::Native => native::remove(&service, username).await,
            KeyringProviderBackend::Subprocess => {
                Err(Error::RemoveUnsupported(self.backend.name()))
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(_) => Err(Error::RemoveUnsupported(self.backend.name())),
        }
    }

    /// Fetch credentials for the given URL from the keyring.
    ///
    /// Returns [`Ok(None)`] if no password was found for the username.
    #[instrument(skip_all, fields(url = %url, username))]
    pub async fn fetch(
        &self,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Result<Option<Credentials>, Error> {
        debug_assert!(
            url.host_str().is_some(),
            "Should only use keyring for URLs with host"
        );
        debug_assert!(
            url.password().is_none(),
            "Should only use keyring for URLs without a password"
        );
        debug_assert!(
            !username.map(str::is_empty).unwrap_or(false),
            "Should only use keyring with a non-empty username"
        );

        match &self.backend {
            KeyringProviderBackend::Native => native::fetch(url, username).await,
            KeyringProviderBackend::Subprocess => {
                let credentials = self.fetch_subprocess_with_fallback(url, username).await;
                Ok(credentials
                    .map(|(username, password)| Credentials::basic(Some(username), Some(password))))
            }
            #[cfg(test)]
            KeyringProviderBackend::Dummy(store) => {
                let credentials = Self::fetch_dummy_with_fallback(store, url, username);
                Ok(credentials
                    .map(|(username, password)| Credentials::basic(Some(username), Some(password))))
            }
        }
    }

    /// Fetch subprocess credentials using the legacy URL, host, and scheme-host lookup order.
    async fn fetch_subprocess_with_fallback(
        &self,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        trace!("Checking keyring for URL {url}");
        let mut credentials = self.fetch_subprocess(url.as_str(), username).await;
        if credentials.is_some() {
            return credentials;
        }

        let host = legacy_host(url)?;
        trace!("Checking keyring for host {host}");
        credentials = self.fetch_subprocess(&host, username).await;
        if credentials.is_none() && url.scheme() != "https" {
            let scheme_host = format!("{}://{host}", url.scheme());
            trace!("Checking keyring for scheme+host {scheme_host}");
            credentials = self.fetch_subprocess(&scheme_host, username).await;
        }
        credentials
    }

    #[instrument(skip(self))]
    async fn fetch_subprocess(
        &self,
        service_name: &str,
        username: Option<&str>,
    ) -> Option<(String, String)> {
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
            std::io::stderr().write_all(&output.stderr).ok();
            let output = String::from_utf8(output.stdout)
                .inspect_err(|err| warn!("Failed to parse response from `keyring` command: {err}"))
                .ok()?;

            let (username, password) = if let Some(username) = username {
                (username, output.trim_end())
            } else {
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
                warn!("Got empty password for `{username}@{service_name}` from `keyring` command");
            }
            Some((username.to_string(), password.to_string()))
        } else {
            let stderr = std::str::from_utf8(&output.stderr).ok()?;
            if stderr.contains("unrecognized arguments: --mode") {
                warn_user_once!(
                    "Attempted to fetch credentials using the `keyring` command, but it does not support `--mode creds`; upgrade to `keyring>=v25.2.1` or provide a username"
                );
            } else if username.is_none() {
                std::io::stderr().write_all(&output.stderr).ok();
            }
            None
        }
    }

    #[cfg(test)]
    fn fetch_dummy_with_fallback(
        store: &[(String, &'static str, &'static str)],
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Option<(String, String)> {
        let mut credentials = Self::fetch_dummy(store, url.as_str(), username);
        if credentials.is_some() {
            return credentials;
        }

        let host = legacy_host(url)?;
        credentials = Self::fetch_dummy(store, &host, username);
        if credentials.is_none() && url.scheme() != "https" {
            credentials = Self::fetch_dummy(store, &format!("{}://{host}", url.scheme()), username);
        }
        credentials
    }

    #[cfg(test)]
    fn fetch_dummy(
        store: &[(String, &'static str, &'static str)],
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

    /// Create a provider backed by static test credentials.
    #[cfg(test)]
    pub(crate) fn dummy<
        S: Into<String>,
        T: IntoIterator<Item = (S, &'static str, &'static str)>,
    >(
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

    /// Create a test provider with no credentials.
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            backend: KeyringProviderBackend::Dummy(Vec::new()),
        }
    }
}

/// Return the host and optional explicit port used by legacy keyring lookups.
fn legacy_host(url: &DisplaySafeUrl) -> Option<String> {
    let host = url.host_str()?;
    Some(if let Some(port) = url.port() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    })
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;
    use url::Url;

    use super::*;

    #[tokio::test]
    async fn fetch_url_no_host() {
        let url = Url::parse("file:/etc/bin/").unwrap();
        let keyring = KeyringProvider::empty();
        let fetch = keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some("user"));
        if cfg!(debug_assertions) {
            assert!(
                std::panic::AssertUnwindSafe(fetch)
                    .catch_unwind()
                    .await
                    .is_err()
            );
        } else {
            assert_eq!(fetch.await.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn fetch_url_with_password() {
        let url = Url::parse("https://user:password@example.com").unwrap();
        let keyring = KeyringProvider::empty();
        let fetch = keyring.fetch(DisplaySafeUrl::ref_cast(&url), Some(url.username()));
        if cfg!(debug_assertions) {
            assert!(
                std::panic::AssertUnwindSafe(fetch)
                    .catch_unwind()
                    .await
                    .is_err()
            );
        } else {
            assert_eq!(fetch.await.unwrap(), None);
        }
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
                .await
                .unwrap()
                .and_then(|credentials| credentials.password().map(str::to_string)),
            Some("password".to_string())
        );
    }

    #[tokio::test]
    async fn fetch_http_scheme_host_fallback() {
        let url = Url::parse("http://127.0.0.1:8080/basic-auth/simple/anyio/").unwrap();
        let keyring = KeyringProvider::dummy([("http://127.0.0.1:8080", "user", "password")]);
        assert!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn fetch_http_scheme_host_does_not_cross_schemes() {
        let url = Url::parse("https://127.0.0.1:8080/basic-auth/simple/anyio/").unwrap();
        let keyring = KeyringProvider::dummy([("http://127.0.0.1:8080", "user", "password")]);
        assert_eq!(
            keyring
                .fetch(DisplaySafeUrl::ref_cast(&url), Some("user"))
                .await
                .unwrap(),
            None
        );
    }
}
