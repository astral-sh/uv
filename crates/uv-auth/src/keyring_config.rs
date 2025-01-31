use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use thiserror::Error;
use tracing::debug;
use url::Url;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization/Deserialization error: {0}")]
    SerdeError(#[from] toml::de::Error),

    #[error("Invalid configuration path")]
    InvalidPath,

    #[error("Serialization error while storing config: {0}")]
    TomlSerializationError(#[from] toml::ser::Error),
}

#[cfg(test)]
static CONFIG_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub trait ConfigFile {
    fn path() -> Result<PathBuf, ConfigError>;

    fn load() -> Result<Self, ConfigError>
    where
        Self: Sized;

    fn store(&self) -> Result<(), ConfigError>;
}

impl ConfigFile for AuthConfig {
    fn path() -> Result<PathBuf, ConfigError> {
        #[cfg(test)]
        {
            // Lock the mutex safely and access the path
            let path_guard = CONFIG_PATH.lock().unwrap();
            if let Some(ref path) = *path_guard {
                return Ok(path.clone());
            }
        }

        let cache_dir = uv_dirs::user_state_dir().ok_or(ConfigError::InvalidPath)?;
        Ok(cache_dir.join("auth.toml"))
    }

    fn load() -> Result<Self, ConfigError> {
        let path = AuthConfig::path()?;
        AuthConfig::load_from_path(&path)
    }

    fn store(&self) -> Result<(), ConfigError> {
        let path = AuthConfig::path()?;
        self.store_to_path(&path)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug)]
pub struct AuthConfig {
    pub indexes: HashMap<String, Index>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Index {
    pub username: String,
}

impl AuthConfig {
    pub fn add_entry(&mut self, index_url: &Url, username: String) {
        let host = self.url_to_host(index_url);
        self.indexes.entry(host).or_insert(Index { username });
    }

    pub fn find_entry(&self, index_url: &Url) -> Option<&Index> {
        let host = self.url_to_host(index_url);
        self.indexes.get(&host)
    }

    pub fn delete_entry(&mut self, index_url: &Url) {
        let host = self.url_to_host(index_url);
        self.indexes.remove(&host);
    }

    pub fn load_from_path(path: &PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(AuthConfig {
                indexes: HashMap::new(),
            });
        }

        let contents = fs::read_to_string(path)?;
        let config: AuthConfig = toml::de::from_str(&contents)?;
        Ok(config)
    }

    pub fn store_to_path(&self, path: &PathBuf) -> Result<(), ConfigError> {
        let contents = toml::to_string_pretty(self)?;
        let dir = path
            .parent()
            .expect("Path to auth config should have a parent directory!");

        if !dir.exists() {
            debug!("Creating directory {dir:?}");
            fs::create_dir_all(dir).unwrap();
        }

        let mut file = fs::File::create(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }

    fn url_to_host(&self, url: &Url) -> String {
        let host = if let Some(port) = url.port() {
            format!(
                "{}:{}",
                url.host_str().expect("Url should have a host"),
                port
            )
        } else {
            url.host_str().expect("Url should have a host").to_string()
        };
        return host;
    }
}

#[cfg(test)]
pub(crate) fn set_test_config_path(path: PathBuf) {
    let mut path_guard = CONFIG_PATH.lock().unwrap();
    *path_guard = Some(path);
}

#[cfg(test)]
pub(crate) fn reset_config_path() {
    let mut path_guard = CONFIG_PATH.lock().unwrap();
    *path_guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Helper function to clean up a temporary file
    fn remove_temp_file(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_load_no_config_file() {
        // Step 1: Create a temporary file using tempfile
        // let temp_file = NamedTempFile::new().expect("Failed to create a temp file");

        let path = Path::new("test_auth.toml");
        remove_temp_file(path).ok();

        let config = AuthConfig::load_from_path(&path.to_path_buf());
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.indexes.len(), 0);
    }

    #[test]
    fn test_store_no_config_file() {
        // Prepare a fake file path for the test
        let url = Url::parse("https://example.com/secure/pypi").unwrap();
        let path = Path::new("test_auth.toml");
        remove_temp_file(path).ok();

        let config = AuthConfig::load_from_path(&path.to_path_buf());
        assert!(config.is_ok());
        let mut config = config.unwrap();

        config.add_entry(&url, "user1".to_string());

        let result = config.store_to_path(&path.to_path_buf());
        assert!(result.is_ok());

        // Check if the file exists and contains the correct content
        assert!(path.exists());

        let contents = fs::read_to_string(path).expect("Failed to read config file");
        assert!(contents.contains("example.com"));
        assert!(contents.contains("user1"));

        // Clean up
        remove_temp_file(path).ok();
    }

    #[test]
    fn test_find_entry() {
        let url = Url::parse("https://example.com/secure/pypi").unwrap();
        let url_not_existing = Url::parse("https://other-domain.com/secure/pypi").unwrap();

        let mut config = AuthConfig {
            indexes: HashMap::new(),
        };
        config.add_entry(&url, "user1".to_string());

        // Test finding an existing entry
        let entry = config.find_entry(&url);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().username, "user1");

        // Test finding a non-existing entry
        let entry = config.find_entry(&url_not_existing);
        assert!(entry.is_none());
    }
}
