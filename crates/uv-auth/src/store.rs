use std::ops::Deref;
use std::path::{Path, PathBuf};

use fs_err as fs;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uv_fs::{LockedFile, LockedFileError, LockedFileMode, with_added_extension};
use uv_preview::{Preview, PreviewFeatures};
use uv_redacted::DisplaySafeUrl;

use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

use crate::credentials::{Password, Token, Username};
use crate::realm::Realm;
use crate::service::Service;
use crate::{Credentials, KeyringProvider};

/// The storage backend to use in `uv auth` commands.
#[derive(Debug)]
pub enum AuthBackend {
    // TODO(zanieb): Right now, we're using a keyring provider for the system store but that's just
    // where the native implementation is living at the moment. We should consider refactoring these
    // into a shared API in the future.
    System(KeyringProvider),
    TextStore(TextCredentialStore, LockedFile),
}

impl AuthBackend {
    pub async fn from_settings(preview: Preview) -> Result<Self, TomlCredentialError> {
        // If preview is enabled, we'll use the system-native store
        if preview.is_enabled(PreviewFeatures::NATIVE_AUTH) {
            return Ok(Self::System(KeyringProvider::native()));
        }

        // Otherwise, we'll use the plaintext credential store
        let path = TextCredentialStore::default_file()?;
        match TextCredentialStore::read(&path).await {
            Ok((store, lock)) => Ok(Self::TextStore(store, lock)),
            Err(err)
                if err
                    .as_io_error()
                    .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(Self::TextStore(
                    TextCredentialStore::default(),
                    TextCredentialStore::lock(&path).await?,
                ))
            }
            Err(err) => Err(err),
        }
    }
}

/// Authentication scheme to use.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    /// HTTP Basic Authentication
    ///
    /// Uses a username and password.
    #[default]
    Basic,
    /// Bearer token authentication.
    ///
    /// Uses a token provided as `Bearer <token>` in the `Authorization` header.
    Bearer,
}

/// Errors that can occur when working with TOML credential storage.
#[derive(Debug, Error)]
pub enum TomlCredentialError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    LockedFile(#[from] LockedFileError),
    #[error("Failed to parse TOML credential file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Failed to serialize credentials to TOML")]
    SerializeError(#[from] toml::ser::Error),
    #[error(transparent)]
    BasicAuthError(#[from] BasicAuthError),
    #[error(transparent)]
    BearerAuthError(#[from] BearerAuthError),
    #[error("Failed to determine credentials directory")]
    CredentialsDirError,
    #[error("Token is not valid unicode")]
    TokenNotUnicode(#[from] std::string::FromUtf8Error),
}

impl TomlCredentialError {
    pub fn as_io_error(&self) -> Option<&std::io::Error> {
        match self {
            Self::Io(err) => Some(err),
            Self::LockedFile(err) => err.as_io_error(),
            Self::ParseError(_)
            | Self::SerializeError(_)
            | Self::BasicAuthError(_)
            | Self::BearerAuthError(_)
            | Self::CredentialsDirError
            | Self::TokenNotUnicode(_) => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum BasicAuthError {
    #[error("`username` is required with `scheme = basic`")]
    MissingUsername,
    #[error("`token` cannot be provided with `scheme = basic`")]
    UnexpectedToken,
}

#[derive(Debug, Error)]
pub enum BearerAuthError {
    #[error("`token` is required with `scheme = bearer`")]
    MissingToken,
    #[error("`username` cannot be provided with `scheme = bearer`")]
    UnexpectedUsername,
    #[error("`password` cannot be provided with `scheme = bearer`")]
    UnexpectedPassword,
}

/// A single credential entry in a TOML credentials file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "TomlCredentialWire", into = "TomlCredentialWire")]
struct TomlCredential {
    /// The service URL for this credential.
    service: Service,
    /// The credentials for this entry.
    credentials: Credentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TomlCredentialWire {
    /// The service URL for this credential.
    service: Service,
    /// The username to use. Only allowed with [`AuthScheme::Basic`].
    username: Username,
    /// The authentication scheme.
    #[serde(default)]
    scheme: AuthScheme,
    /// The password to use. Only allowed with [`AuthScheme::Basic`].
    password: Option<Password>,
    /// The token to use. Only allowed with [`AuthScheme::Bearer`].
    token: Option<String>,
}

impl From<TomlCredential> for TomlCredentialWire {
    fn from(value: TomlCredential) -> Self {
        match value.credentials {
            Credentials::Basic { username, password } => Self {
                service: value.service,
                username,
                scheme: AuthScheme::Basic,
                password,
                token: None,
            },
            Credentials::Bearer { token } => Self {
                service: value.service,
                username: Username::new(None),
                scheme: AuthScheme::Bearer,
                password: None,
                token: Some(String::from_utf8(token.into_bytes()).expect("Token is valid UTF-8")),
            },
        }
    }
}

impl TryFrom<TomlCredentialWire> for TomlCredential {
    type Error = TomlCredentialError;

    fn try_from(value: TomlCredentialWire) -> Result<Self, Self::Error> {
        match value.scheme {
            AuthScheme::Basic => {
                if value.username.as_deref().is_none() {
                    return Err(TomlCredentialError::BasicAuthError(
                        BasicAuthError::MissingUsername,
                    ));
                }
                if value.token.is_some() {
                    return Err(TomlCredentialError::BasicAuthError(
                        BasicAuthError::UnexpectedToken,
                    ));
                }
                let credentials = Credentials::Basic {
                    username: value.username,
                    password: value.password,
                };
                Ok(Self {
                    service: value.service,
                    credentials,
                })
            }
            AuthScheme::Bearer => {
                if value.username.is_some() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::UnexpectedUsername,
                    ));
                }
                if value.password.is_some() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::UnexpectedPassword,
                    ));
                }
                if value.token.is_none() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::MissingToken,
                    ));
                }
                let credentials = Credentials::Bearer {
                    token: Token::new(value.token.unwrap().into_bytes()),
                };
                Ok(Self {
                    service: value.service,
                    credentials,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TomlCredentials {
    /// Array of credential entries.
    #[serde(rename = "credential")]
    credentials: Vec<TomlCredential>,
}

/// A credential store with a plain text storage backend.
#[derive(Debug, Default)]
pub struct TextCredentialStore {
    credentials: FxHashMap<(Service, Username), Credentials>,
}

impl TextCredentialStore {
    /// Return the directory for storing credentials.
    pub fn directory_path() -> Result<PathBuf, TomlCredentialError> {
        if let Some(dir) = std::env::var_os(EnvVars::UV_CREDENTIALS_DIR)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
        {
            return Ok(dir);
        }

        Ok(StateStore::from_settings(None)?.bucket(StateBucket::Credentials))
    }

    /// Return the standard file path for storing credentials.
    pub fn default_file() -> Result<PathBuf, TomlCredentialError> {
        let dir = Self::directory_path()?;
        Ok(dir.join("credentials.toml"))
    }

    /// Acquire a lock on the credentials file at the given path.
    pub async fn lock(path: &Path) -> Result<LockedFile, TomlCredentialError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock = with_added_extension(path, ".lock");
        Ok(LockedFile::acquire(lock, LockedFileMode::Exclusive, "credentials store").await?)
    }

    /// Read credentials from a file.
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, TomlCredentialError> {
        let content = fs::read_to_string(path)?;
        let credentials: TomlCredentials = toml::from_str(&content)?;

        let credentials: FxHashMap<(Service, Username), Credentials> = credentials
            .credentials
            .into_iter()
            .map(|credential| {
                let username = match &credential.credentials {
                    Credentials::Basic { username, .. } => username.clone(),
                    Credentials::Bearer { .. } => Username::none(),
                };
                (
                    (credential.service.clone(), username),
                    credential.credentials,
                )
            })
            .collect();

        Ok(Self { credentials })
    }

    /// Read credentials from a file.
    ///
    /// Returns [`TextCredentialStore`] and a [`LockedFile`] to hold if mutating the store.
    ///
    /// If the store will not be written to following the read, the lock can be dropped.
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<(Self, LockedFile), TomlCredentialError> {
        let lock = Self::lock(path.as_ref()).await?;
        let store = Self::from_file(path)?;
        Ok((store, lock))
    }

    /// Persist credentials to a file.
    ///
    /// Requires a [`LockedFile`] from [`TextCredentialStore::lock`] or
    /// [`TextCredentialStore::read`] to ensure exclusive access.
    pub fn write<P: AsRef<Path>>(
        self,
        path: P,
        _lock: LockedFile,
    ) -> Result<(), TomlCredentialError> {
        let credentials = self
            .credentials
            .into_iter()
            .map(|((service, _username), credentials)| TomlCredential {
                service,
                credentials,
            })
            .collect::<Vec<_>>();

        let toml_creds = TomlCredentials { credentials };
        let content = toml::to_string_pretty(&toml_creds)?;
        fs::create_dir_all(
            path.as_ref()
                .parent()
                .ok_or(TomlCredentialError::CredentialsDirError)?,
        )?;

        // TODO(zanieb): We should use an atomic write here
        fs::write(path, content)?;
        Ok(())
    }

    /// Get credentials for a given URL and username.
    ///
    /// The most specific URL prefix match in the same [`Realm`] is returned, if any.
    pub fn get_credentials(
        &self,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Option<&Credentials> {
        let request_realm = Realm::from(url);

        // Perform an exact lookup first
        // TODO(zanieb): Consider adding `DisplaySafeUrlRef` so we can avoid this clone
        // TODO(zanieb): We could also return early here if we can't normalize to a `Service`
        if let Ok(url_service) = Service::try_from(url.clone()) {
            if let Some(credential) = self.credentials.get(&(
                url_service.clone(),
                Username::from(username.map(str::to_string)),
            )) {
                return Some(credential);
            }
        }

        // If that fails, iterate through to find a prefix match
        let mut best: Option<(usize, &Service, &Credentials)> = None;

        for ((service, stored_username), credential) in &self.credentials {
            let service_realm = Realm::from(service.url().deref());

            // Only consider services in the same realm
            if service_realm != request_realm {
                continue;
            }

            // Service path must be a prefix of request path
            if !url.path().starts_with(service.url().path()) {
                continue;
            }

            // If a username is provided, it must match
            if let Some(request_username) = username {
                if Some(request_username) != stored_username.as_deref() {
                    continue;
                }
            }

            // Update our best matching credential based on prefix length
            let specificity = service.url().path().len();
            if best.is_none_or(|(best_specificity, _, _)| specificity > best_specificity) {
                best = Some((specificity, service, credential));
            }
        }

        // Return the most specific match
        if let Some((_, _, credential)) = best {
            return Some(credential);
        }

        None
    }

    /// Store credentials for a given service.
    pub fn insert(&mut self, service: Service, credentials: Credentials) -> Option<Credentials> {
        let username = match &credentials {
            Credentials::Basic { username, .. } => username.clone(),
            Credentials::Bearer { .. } => Username::none(),
        };
        self.credentials.insert((service, username), credentials)
    }

    /// Remove credentials for a given service.
    pub fn remove(&mut self, service: &Service, username: Username) -> Option<Credentials> {
        // Remove the specific credential for this service and username
        self.credentials.remove(&(service.clone(), username))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::str::FromStr;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_toml_serialization() {
        let credentials = TomlCredentials {
            credentials: vec![
                TomlCredential {
                    service: Service::from_str("https://example.com").unwrap(),
                    credentials: Credentials::Basic {
                        username: Username::new(Some("user1".to_string())),
                        password: Some(Password::new("pass1".to_string())),
                    },
                },
                TomlCredential {
                    service: Service::from_str("https://test.org").unwrap(),
                    credentials: Credentials::Basic {
                        username: Username::new(Some("user2".to_string())),
                        password: Some(Password::new("pass2".to_string())),
                    },
                },
            ],
        };

        let toml_str = toml::to_string_pretty(&credentials).unwrap();
        let parsed: TomlCredentials = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.credentials.len(), 2);
        assert_eq!(
            parsed.credentials[0].service.to_string(),
            "https://example.com/"
        );
        assert_eq!(
            parsed.credentials[1].service.to_string(),
            "https://test.org/"
        );
    }

    #[test]
    fn test_credential_store_operations() {
        let mut store = TextCredentialStore::default();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        let service = Service::from_str("https://example.com").unwrap();
        store.insert(service.clone(), credentials.clone());
        let url = DisplaySafeUrl::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url, None).is_some());

        let url = DisplaySafeUrl::parse("https://example.com/path").unwrap();
        let retrieved = store.get_credentials(&url, None).unwrap();
        assert_eq!(retrieved.username(), Some("user"));
        assert_eq!(retrieved.password(), Some("pass"));

        assert!(
            store
                .remove(&service, Username::from(Some("user".to_string())))
                .is_some()
        );
        let url = DisplaySafeUrl::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url, None).is_none());
    }

    #[tokio::test]
    async fn test_file_operations() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"
[[credential]]
service = "https://example.com"
username = "testuser"
scheme = "basic"
password = "testpass"

[[credential]]
service = "https://test.org"
username = "user2"
password = "pass2"
"#
        )
        .unwrap();

        let store = TextCredentialStore::from_file(temp_file.path()).unwrap();

        let url = DisplaySafeUrl::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url, None).is_some());
        let url = DisplaySafeUrl::parse("https://test.org/").unwrap();
        assert!(store.get_credentials(&url, None).is_some());

        let url = DisplaySafeUrl::parse("https://example.com").unwrap();
        let cred = store.get_credentials(&url, None).unwrap();
        assert_eq!(cred.username(), Some("testuser"));
        assert_eq!(cred.password(), Some("testpass"));

        // Test saving
        let temp_output = NamedTempFile::new().unwrap();
        store
            .write(
                temp_output.path(),
                TextCredentialStore::lock(temp_file.path()).await.unwrap(),
            )
            .unwrap();

        let content = fs::read_to_string(temp_output.path()).unwrap();
        assert!(content.contains("example.com"));
        assert!(content.contains("testuser"));
    }

    #[test]
    fn test_prefix_matching() {
        let mut store = TextCredentialStore::default();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        // Store credentials for a specific path prefix
        let service = Service::from_str("https://example.com/api").unwrap();
        store.insert(service.clone(), credentials.clone());

        // Should match URLs that are prefixes of the stored service
        let matching_urls = [
            "https://example.com/api",
            "https://example.com/api/v1",
            "https://example.com/api/v1/users",
        ];

        for url_str in matching_urls {
            let url = DisplaySafeUrl::parse(url_str).unwrap();
            let cred = store.get_credentials(&url, None);
            assert!(cred.is_some(), "Failed to match URL with prefix: {url_str}");
        }

        // Should NOT match URLs that are not prefixes
        let non_matching_urls = [
            "https://example.com/different",
            "https://example.com/ap", // Not a complete path segment match
            "https://example.com",    // Shorter than the stored prefix
        ];

        for url_str in non_matching_urls {
            let url = DisplaySafeUrl::parse(url_str).unwrap();
            let cred = store.get_credentials(&url, None);
            assert!(cred.is_none(), "Should not match non-prefix URL: {url_str}");
        }
    }

    #[test]
    fn test_realm_based_matching() {
        let mut store = TextCredentialStore::default();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        // Store by full URL (realm)
        let service = Service::from_str("https://example.com").unwrap();
        store.insert(service.clone(), credentials.clone());

        // Should match URLs in the same realm
        let matching_urls = [
            "https://example.com",
            "https://example.com/path",
            "https://example.com/different/path",
            "https://example.com:443/path", // Default HTTPS port
        ];

        for url_str in matching_urls {
            let url = DisplaySafeUrl::parse(url_str).unwrap();
            let cred = store.get_credentials(&url, None);
            assert!(
                cred.is_some(),
                "Failed to match URL in same realm: {url_str}"
            );
        }

        // Should NOT match URLs in different realms
        let non_matching_urls = [
            "http://example.com",       // Different scheme
            "https://different.com",    // Different host
            "https://example.com:8080", // Different port
        ];

        for url_str in non_matching_urls {
            let url = DisplaySafeUrl::parse(url_str).unwrap();
            let cred = store.get_credentials(&url, None);
            assert!(
                cred.is_none(),
                "Should not match URL in different realm: {url_str}"
            );
        }
    }

    #[test]
    fn test_most_specific_prefix_matching() {
        let mut store = TextCredentialStore::default();
        let general_cred =
            Credentials::basic(Some("general".to_string()), Some("pass1".to_string()));
        let specific_cred =
            Credentials::basic(Some("specific".to_string()), Some("pass2".to_string()));

        // Store credentials with different prefix lengths
        let general_service = Service::from_str("https://example.com/api").unwrap();
        let specific_service = Service::from_str("https://example.com/api/v1").unwrap();
        store.insert(general_service.clone(), general_cred);
        store.insert(specific_service.clone(), specific_cred);

        // Should match the most specific prefix
        let url = DisplaySafeUrl::parse("https://example.com/api/v1/users").unwrap();
        let cred = store.get_credentials(&url, None).unwrap();
        assert_eq!(cred.username(), Some("specific"));

        // Should match the general prefix for non-specific paths
        let url = DisplaySafeUrl::parse("https://example.com/api/v2").unwrap();
        let cred = store.get_credentials(&url, None).unwrap();
        assert_eq!(cred.username(), Some("general"));
    }

    #[test]
    fn test_username_exact_url_match() {
        let mut store = TextCredentialStore::default();
        let url = DisplaySafeUrl::parse("https://example.com").unwrap();
        let service = Service::from_str("https://example.com").unwrap();
        let user1_creds = Credentials::basic(Some("user1".to_string()), Some("pass1".to_string()));
        store.insert(service.clone(), user1_creds.clone());

        // Should return credentials when username matches
        let result = store.get_credentials(&url, Some("user1"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), Some("user1"));
        assert_eq!(result.unwrap().password(), Some("pass1"));

        // Should not return credentials when username doesn't match
        let result = store.get_credentials(&url, Some("user2"));
        assert!(result.is_none());

        // Should return credentials when no username is specified
        let result = store.get_credentials(&url, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), Some("user1"));
    }

    #[test]
    fn test_username_prefix_url_match() {
        let mut store = TextCredentialStore::default();

        // Add credentials with different usernames for overlapping URL prefixes
        let general_service = Service::from_str("https://example.com/api").unwrap();
        let specific_service = Service::from_str("https://example.com/api/v1").unwrap();

        let general_creds = Credentials::basic(
            Some("general_user".to_string()),
            Some("general_pass".to_string()),
        );
        let specific_creds = Credentials::basic(
            Some("specific_user".to_string()),
            Some("specific_pass".to_string()),
        );

        store.insert(general_service, general_creds);
        store.insert(specific_service, specific_creds);

        let url = DisplaySafeUrl::parse("https://example.com/api/v1/users").unwrap();

        // Should match specific credentials when username matches
        let result = store.get_credentials(&url, Some("specific_user"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), Some("specific_user"));

        // Should match the general credentials when requesting general_user (falls back to less specific prefix)
        let result = store.get_credentials(&url, Some("general_user"));
        assert!(
            result.is_some(),
            "Should match general_user from less specific prefix"
        );
        assert_eq!(result.unwrap().username(), Some("general_user"));

        // Should match most specific when no username specified
        let result = store.get_credentials(&url, None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username(), Some("specific_user"));
    }
}
