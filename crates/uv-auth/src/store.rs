use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use fs_err as fs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;
use url::Url;

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
    #[error("Failed to serialize credentials to TOML: {0}")]
    SerializeError(#[from] toml::ser::Error),
    #[error("Invalid credential configuration: {0}")]
    InvalidCredential(String),
}

/// A single credential entry in the TOML file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TomlCredential {
    /// The service URL for this credential.
    pub service: Service,
    /// The username to use.
    pub username: Username,
    /// The authentication scheme.
    #[serde(default)]
    pub scheme: AuthScheme,
    /// The password to use.
    pub password: Option<Password>,
    /// The token to use.
    pub token: Option<String>,
}

impl TomlCredential {
    /// Validate that the credential configuration is correct for the scheme.
    pub fn validate(&self) -> Result<(), TomlCredentialError> {
        match self.scheme {
            AuthScheme::Basic => {
                if self.username.as_deref().is_none() {
                    return Err(TomlCredentialError::InvalidCredential(
                        "Basic auth credentials must have a username".to_string(),
                    ));
                }
                if self.token.is_some() {
                    return Err(TomlCredentialError::InvalidCredential(
                        "Basic auth credentials cannot have a token".to_string(),
                    ));
                }
            }
            AuthScheme::Bearer => {
                if self.username.is_some() {
                    return Err(TomlCredentialError::InvalidCredential(
                        "Bearer token credentials must have empty username".to_string(),
                    ));
                }
                if self.password.is_some() {
                    return Err(TomlCredentialError::InvalidCredential(
                        "Bearer token credentials must have empty password - use 'token' field instead"
                            .to_string(),
                    ));
                }
                if self.token.is_none() {
                    return Err(TomlCredentialError::InvalidCredential(
                        "Bearer token credentials must have a 'token' field".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

/// The root structure for TOML credential files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TomlCredentials {
    /// Array of credential entries.
    #[serde(rename = "credential")]
    pub credentials: Vec<TomlCredential>,
}

impl TomlCredential {
    /// Convert a TOML credential to the internal Credentials enum.
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
    pub fn from_credentials(service: Service, credentials: &Credentials) -> Option<Self> {
        match credentials {
            Credentials::Basic { username, password } => password.as_ref().map(|password| Self {
                service,
                username: username.clone(),
                scheme: AuthScheme::Basic,
                password: Some(password.clone()),
                token: None,
            }),
            Credentials::Bearer { token } => Some(Self {
                service,
                username: Username::new(None),
                scheme: AuthScheme::Bearer,
                password: None,
                token: Some(String::from_utf8_lossy(token).to_string()),
            }),
        }
    }
}

/// A credential store that reads from and writes to TOML files.
#[derive(Debug)]
pub struct TomlCredentialStore {
    credentials: HashMap<String, Credentials>,
}

impl TomlCredentialStore {
    /// Load credentials from a TOML file.
    /// Returns an empty store if the file doesn't exist.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, TomlCredentialError> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist, return empty store
                return Ok(Self {
                    credentials: HashMap::new(),
                });
            }
            Err(e) => return Err(TomlCredentialError::Io(e)),
        };

        let toml_creds: TomlCredentials = toml::from_str(&content).unwrap_or_default();

        let credentials: HashMap<String, Credentials> = toml_creds
            .credentials
            .into_iter()
            .filter_map(|toml_cred| {
                if let Err(e) = toml_cred.validate() {
                    debug!(
                        "Skipping invalid credential for {}: {}",
                        toml_cred.service, e
                    );
                    return None;
                }

                debug!("Loaded credential for service: {}", toml_cred.service);
                let service_str = toml_cred.service.to_string();
                let cred = toml_cred.into_credentials();
                Some((service_str, cred))
            })
            .collect();

        debug!("Loaded {} credentials from TOML file", credentials.len());

        Ok(Self { credentials })
    }

    /// Save credentials to a TOML file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), TomlCredentialError> {
        let credentials = self
            .credentials
            .iter()
            .filter_map(|(service_str, cred)| {
                Service::from_str(service_str)
                    .ok()
                    .and_then(|service| TomlCredential::from_credentials(service, cred))
            })
            .collect();

        let toml_creds = TomlCredentials { credentials };
        let content = toml::to_string_pretty(&toml_creds)?;
        fs::write(path, content)?;
        debug!(
            "Saved {} credentials to TOML file",
            toml_creds.credentials.len()
        );
        Ok(())
    }

    /// Get credentials for a given URL.
    /// Uses realm-based prefix matching following RFC 7235 and 7230 specifications.
    /// Credentials are matched by finding the most specific prefix that matches the request URL.
    pub fn get_credentials(&self, url: &Url) -> Option<Credentials> {
        let url_str = url.to_string();
        let request_realm = Realm::from(url);

        // Try exact URL match first
        if let Some(cred) = self.credentials.get(&url_str) {
            debug!("Found credentials for exact URL: {}", url_str);
            return Some(cred.clone());
        }

        // Find the most specific matching service
        let mut best_match: Option<(usize, &String, &Credentials)> = None;

        for (service, cred) in &self.credentials {
            // Try to parse the service as a URL for realm and prefix comparison
            if let Ok(service_url) = Url::parse(service) {
                let service_realm = Realm::from(&service_url);

                // Only consider services in the same realm
                if service_realm == request_realm {
                    // Check if the service URL is a prefix of the request URL
                    let service_path = service_url.path();
                    let request_path = url.path();

                    // Service path must be a prefix of request path
                    if request_path.starts_with(service_path) {
                        let specificity = service_path.len();
                        debug!("Found realm+prefix match: {} for {}", service, url_str);

                        // Keep this match if it's more specific than the current best
                        if best_match
                            .is_none_or(|(best_specificity, _, _)| specificity > best_specificity)
                        {
                            best_match = Some((specificity, service, cred));
                        }
                    }
                }
            }
        }

        // Return the most specific match
        if let Some((_, service, cred)) = best_match {
            debug!("Selected most specific match: {} for {}", service, url_str);
            return Some(cred.clone());
        }

        debug!("No credentials found for URL: {}", url_str);
        None
    }

    /// Store credentials for a given service.
    pub fn store_credentials(&mut self, service: &Service, credentials: Credentials) {
        let service_str = service.to_string();
        self.credentials.insert(service_str.clone(), credentials);
        debug!("Stored credentials for service: {}", service_str);
    }

    /// Remove credentials for a given service.
    pub fn remove_credentials(&mut self, service: &str) -> bool {
        let removed = self.credentials.remove(service).is_some();
        if removed {
            debug!("Removed credentials for service: {}", service);
        }
        removed
    }

    /// Get all stored service names.
    pub fn services(&self) -> Vec<String> {
        self.credentials.keys().cloned().collect()
    }

    /// Check if credentials exist for a service.
    pub fn has_credentials(&self, service: &str) -> bool {
        self.credentials.contains_key(service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
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
            &credentials,
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
        let mut store = TomlCredentialStore::load_from_file("nonexistent.toml").unwrap();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        let service = Service::from_str("https://example.com").unwrap();
        store.store_credentials(&service, credentials.clone());
        assert!(store.has_credentials("https://example.com/"));

        let url = Url::parse("https://example.com/path").unwrap();
        let retrieved = store.get_credentials(&url).unwrap();
        assert_eq!(retrieved.username(), Some("user"));
        assert_eq!(retrieved.password(), Some("pass"));

        assert!(store.remove_credentials("https://example.com/"));
        assert!(!store.has_credentials("https://example.com/"));
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

        let store = TomlCredentialStore::load_from_file(temp_file.path()).unwrap();

        assert!(store.has_credentials("https://example.com/"));
        assert!(store.has_credentials("https://test.org/"));

        let url = Url::parse("https://example.com").unwrap();
        let cred = store.get_credentials(&url).unwrap();
        assert_eq!(cred.username(), Some("testuser"));
        assert_eq!(cred.password(), Some("testpass"));

        // Test saving
        let temp_output = NamedTempFile::new().unwrap();
        store.save_to_file(temp_output.path()).unwrap();

        let content = fs::read_to_string(temp_output.path()).unwrap();
        assert!(content.contains("example.com"));
        assert!(content.contains("testuser"));
    }

    #[test]
    fn test_prefix_matching() {
        let mut store = TomlCredentialStore::load_from_file("nonexistent.toml").unwrap();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        // Store credentials for a specific path prefix
        let service = Service::from_str("https://example.com/api").unwrap();
        store.store_credentials(&service, credentials.clone());

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
        let mut store = TomlCredentialStore::load_from_file("nonexistent.toml").unwrap();
        let credentials = Credentials::basic(Some("user".to_string()), Some("pass".to_string()));

        // Store by full URL (realm)
        let service = Service::from_str("https://example.com").unwrap();
        store.store_credentials(&service, credentials.clone());

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
        let mut store = TomlCredentialStore::load_from_file("nonexistent.toml").unwrap();
        let general_cred =
            Credentials::basic(Some("general".to_string()), Some("pass1".to_string()));
        let specific_cred =
            Credentials::basic(Some("specific".to_string()), Some("pass2".to_string()));

        // Store credentials with different prefix lengths
        let general_service = Service::from_str("https://example.com/api").unwrap();
        let specific_service = Service::from_str("https://example.com/api/v1").unwrap();
        store.store_credentials(&general_service, general_cred);
        store.store_credentials(&specific_service, specific_cred);

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
