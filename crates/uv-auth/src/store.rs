use std::ops::Deref;
use std::path::{Path, PathBuf};

use fs_err as fs;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;
use url::Url;
use uv_redacted::DisplaySafeUrl;

use crate::Credentials;
use crate::credentials::{Password, Username};
use crate::realm::Realm;
use crate::service::Service;

/// Authentication scheme to use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthScheme {
    /// HTTP Basic Authentication
    ///
    /// Uses a username and password.
    Basic,
    /// Bearer token authentication.
    ///
    /// Uses a token provided as `Bearer <token>` in the `Authorization` header.
    Bearer,
}

impl Default for AuthScheme {
    fn default() -> Self {
        Self::Basic
    }
}

/// Errors that can occur when working with TOML credential storage.
#[derive(Debug, Error)]
pub enum TomlCredentialError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
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
// TODO(zanieb): It's a little clunky that we need don't nest the scheme-specific fields under a
// that scheme, but I want the username / password case to be easily accessible without
// understanding authentication schemes. We should consider a better structure here, e.g., by
// adding an internal type that we cast to after validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TomlCredential {
    /// The service URL for this credential.
    pub service: Service,
    /// The username to use. Only allowed with [`AuthScheme::Basic`].
    pub username: Username,
    /// The authentication scheme.
    #[serde(default)]
    pub scheme: AuthScheme,
    /// The password to use. Only allowed with [`AuthScheme::Basic`].
    pub password: Option<Password>,
    /// The token to use. Only allowed with [`AuthScheme::Bearer`].
    pub token: Option<String>,
}

impl TomlCredential {
    /// Validate that the credential configuration is correct for the scheme.
    fn validate(&self) -> Result<(), TomlCredentialError> {
        match self.scheme {
            AuthScheme::Basic => {
                if self.username.as_deref().is_none() {
                    return Err(TomlCredentialError::BasicAuthError(
                        BasicAuthError::MissingUsername,
                    ));
                }
                if self.token.is_some() {
                    return Err(TomlCredentialError::BasicAuthError(
                        BasicAuthError::UnexpectedToken,
                    ));
                }
            }
            AuthScheme::Bearer => {
                if self.username.is_some() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::UnexpectedUsername,
                    ));
                }
                if self.password.is_some() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::UnexpectedPassword,
                    ));
                }
                if self.token.is_none() {
                    return Err(TomlCredentialError::BearerAuthError(
                        BearerAuthError::MissingToken,
                    ));
                }
            }
        }

        Ok(())
    }

    /// Convert to [`Credentials`].
    ///
    /// This method can panic if [`TomlCredential::validate`] has not been called.
    pub fn into_credentials(self) -> Credentials {
        match self.scheme {
            AuthScheme::Basic => Credentials::Basic {
                username: self.username,
                password: self.password,
            },
            AuthScheme::Bearer => Credentials::Bearer {
                token: self.token.unwrap().into_bytes(),
            },
        }
    }

    /// Construct a [`TomlCredential`] for a service from [`Credentials`].
    pub fn from_credentials(
        service: Service,
        credentials: Credentials,
    ) -> Result<Self, TomlCredentialError> {
        match credentials {
            Credentials::Basic { username, password } => Ok(Self {
                service,
                username,
                scheme: AuthScheme::Basic,
                password,
                token: None,
            }),
            Credentials::Bearer { token } => Ok(Self {
                service,
                username: Username::new(None),
                scheme: AuthScheme::Bearer,
                password: None,
                token: Some(String::from_utf8(token)?),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TomlCredentials {
    /// Array of credential entries.
    #[serde(rename = "credential")]
    pub credentials: Vec<TomlCredential>,
}

/// A credential store with a plain text storage backend.
#[derive(Debug, Default)]
pub struct TextCredentialStore {
    credentials: FxHashMap<Service, Credentials>,
}

impl TextCredentialStore {
    /// Return the default credential file path.
    pub fn default_file() -> Result<PathBuf, TomlCredentialError> {
        let state_dir =
            uv_dirs::user_state_dir().ok_or(TomlCredentialError::CredentialsDirError)?;
        let credentials_dir = state_dir.join("credentials");
        Ok(credentials_dir.join("credentials.toml"))
    }

    /// Read credentials from a file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, TomlCredentialError> {
        let content = fs::read_to_string(path)?;
        let credentials: TomlCredentials = toml::from_str(&content)?;

        let credentials: FxHashMap<Service, Credentials> = credentials
            .credentials
            .into_iter()
            .filter_map(|credential| {
                // TODO(zanieb): Determine a better strategy for invalid credential entries
                if let Err(err) = credential.validate() {
                    debug!(
                        "Skipping invalid credential for {}: {}",
                        credential.service, err
                    );
                    return None;
                }

                Some((credential.service.clone(), credential.into_credentials()))
            })
            .collect();

        Ok(Self { credentials })
    }

    /// Persist credentials to a file.
    pub fn write<P: AsRef<Path>>(self, path: P) -> Result<(), TomlCredentialError> {
        let credentials = self
            .credentials
            .into_iter()
            .map(|(service, cred)| TomlCredential::from_credentials(service, cred))
            .collect::<Result<Vec<_>, _>>()?;

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

    /// Get credentials for a given URL.
    /// Uses realm-based prefix matching following RFC 7235 and 7230 specifications.
    /// Credentials are matched by finding the most specific prefix that matches the request URL.
    pub fn get_credentials(&self, url: &Url) -> Option<&Credentials> {
        let request_realm = Realm::from(url);

        // Perform an exact lookup first
        // TODO(zanieb): Consider adding `DisplaySafeUrlRef` so we can avoid this clone
        // TODO(zanieb): We could also return early here if we can't normalize to a `Service`
        if let Ok(url_service) = Service::try_from(DisplaySafeUrl::from(url.clone())) {
            if let Some(credential) = self.credentials.get(&url_service) {
                return Some(credential);
            }
        }

        // If that fails, iterate through to find a prefix match
        let mut best: Option<(usize, &Service, &Credentials)> = None;

        for (service, credential) in &self.credentials {
            let service_realm = Realm::from(service.url().deref());

            // Only consider services in the same realm
            if service_realm != request_realm {
                continue;
            }

            // Service path must be a prefix of request path
            if !url.path().starts_with(service.url().path()) {
                continue;
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
        self.credentials.insert(service, credentials)
    }

    /// Remove credentials for a given service.
    pub fn remove(&mut self, service: &Service) -> Option<Credentials> {
        self.credentials.remove(service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::str::FromStr;
    use tempfile::NamedTempFile;

    #[test]
    fn test_toml_credential_conversion() {
        let toml_cred = TomlCredential {
            service: Service::from_str("https://example.com").unwrap(),
            username: Username::new(Some("user".to_string())),
            scheme: AuthScheme::Basic,
            password: Some(Password::new("pass".to_string())),
            token: None,
        };

        let credentials = toml_cred.into_credentials();
        assert_eq!(credentials.username(), Some("user"));
        assert_eq!(credentials.password(), Some("pass"));

        let back_to_toml = TomlCredential::from_credentials(
            Service::from_str("https://example.com").unwrap(),
            credentials,
        )
        .unwrap();
        assert_eq!(back_to_toml.service.to_string(), "https://example.com/");
        assert_eq!(back_to_toml.username.as_deref(), Some("user"));
        assert_eq!(back_to_toml.password.as_ref().unwrap().as_str(), "pass");
        assert_eq!(back_to_toml.scheme, AuthScheme::Basic);
    }

    #[test]
    fn test_toml_serialization() {
        let credentials = TomlCredentials {
            credentials: vec![
                TomlCredential {
                    service: Service::from_str("https://example.com").unwrap(),
                    username: Username::new(Some("user1".to_string())),
                    scheme: AuthScheme::Basic,
                    password: Some(Password::new("pass1".to_string())),
                    token: None,
                },
                TomlCredential {
                    service: Service::from_str("https://test.org").unwrap(),
                    username: Username::new(Some("user2".to_string())),
                    scheme: AuthScheme::Basic,
                    password: Some(Password::new("pass2".to_string())),
                    token: None,
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
        let url = Url::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url).is_some());

        let url = Url::parse("https://example.com/path").unwrap();
        let retrieved = store.get_credentials(&url).unwrap();
        assert_eq!(retrieved.username(), Some("user"));
        assert_eq!(retrieved.password(), Some("pass"));

        assert!(store.remove(&service).is_some());
        let url = Url::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url).is_none());
    }

    #[test]
    fn test_file_operations() {
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

        let url = Url::parse("https://example.com/").unwrap();
        assert!(store.get_credentials(&url).is_some());
        let url = Url::parse("https://test.org/").unwrap();
        assert!(store.get_credentials(&url).is_some());

        let url = Url::parse("https://example.com").unwrap();
        let cred = store.get_credentials(&url).unwrap();
        assert_eq!(cred.username(), Some("testuser"));
        assert_eq!(cred.password(), Some("testpass"));

        // Test saving
        let temp_output = NamedTempFile::new().unwrap();
        store.write(temp_output.path()).unwrap();

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
            let url = Url::parse(url_str).unwrap();
            let cred = store.get_credentials(&url);
            assert!(cred.is_some(), "Failed to match URL with prefix: {url_str}");
        }

        // Should NOT match URLs that are not prefixes
        let non_matching_urls = [
            "https://example.com/different",
            "https://example.com/ap", // Not a complete path segment match
            "https://example.com",    // Shorter than the stored prefix
        ];

        for url_str in non_matching_urls {
            let url = Url::parse(url_str).unwrap();
            let cred = store.get_credentials(&url);
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
            let url = Url::parse(url_str).unwrap();
            let cred = store.get_credentials(&url);
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
            let url = Url::parse(url_str).unwrap();
            let cred = store.get_credentials(&url);
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
        let url = Url::parse("https://example.com/api/v1/users").unwrap();
        let cred = store.get_credentials(&url).unwrap();
        assert_eq!(cred.username(), Some("specific"));

        // Should match the general prefix for non-specific paths
        let url = Url::parse("https://example.com/api/v2").unwrap();
        let cred = store.get_credentials(&url).unwrap();
        assert_eq!(cred.username(), Some("general"));
    }
}
