use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use toml;
use thiserror::Error;

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


pub trait ConfigFile {
    fn path() -> Result<PathBuf, ConfigError>;

    fn load() -> Result<Self, ConfigError>
    where
        Self: Sized;

    fn store(&self) -> Result<(), ConfigError>;
}

impl ConfigFile for AuthConfig {
    fn path() -> Result<PathBuf, ConfigError> {
        let cache_dir = uv_dirs::user_cache_dir().ok_or(ConfigError::InvalidPath)?;
        Ok(cache_dir.join("auth.toml"))
    }

    fn load() -> Result<Self, ConfigError> {
        let path = AuthConfig::path()?;
        return AuthConfig::load_from_path(&path);
    }

    fn store(&self) -> Result<(), ConfigError> {
        let path = AuthConfig::path()?;
        return self.store_to_path(&path);
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
    pub fn add_entry(&mut self, index_name: String, username: String) {
        self.indexes.entry(index_name).or_insert(Index { username });
    }

    pub fn find_entry(&self, index_name: &str) -> Option<&Index> {
        self.indexes.get(index_name)
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
        let mut file = File::create(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::fs;

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
        let path = Path::new("test_auth.toml");
        remove_temp_file(path).ok();

        let config = AuthConfig::load_from_path(&path.to_path_buf());
        assert!(config.is_ok());
        let mut config = config.unwrap();

        config.add_entry("index1".to_string(), "user1".to_string());

        let result = config.store_to_path(&path.to_path_buf());
        assert!(result.is_ok());

        // Check if the file exists and contains the correct content
        assert!(path.exists());

        let contents = fs::read_to_string(path).expect("Failed to read config file");
        assert!(contents.contains("index1"));
        assert!(contents.contains("user1"));

        // Clean up
        remove_temp_file(path).ok();
    }

    #[test]
    fn test_find_entry() {
        let mut config = AuthConfig {
            indexes: HashMap::new(),
        };
        config.add_entry("index1".to_string(), "user1".to_string());

        // Test finding an existing entry
        let entry = config.find_entry("index1");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().username, "user1");

        // Test finding a non-existing entry
        let entry = config.find_entry("nonexistent");
        assert!(entry.is_none());
    }
}
