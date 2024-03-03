use std::sync::RwLock;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;

/// A global instance of `GlobalConfig` protected by a read-write lock for thread-safe access.
/// Particularly faster reads since `RwLock` allows multiple readers and a single writer at a time.
static SETTINGS: Lazy<RwLock<GlobalConfig>> = Lazy::new(|| RwLock::new(GlobalConfig::default()));

/// Represents application global config.
///
/// This struct holds global configuration from uv, such as its version.
/// The settings are intended to be globally accessible and modifiable in a thread-safe manner.
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub version: String,
}

impl Default for GlobalConfig {
    /// Returns a default instance of `GlobalConfig`.
    /// `version` is by default initialized the package version as a fallback.
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl GlobalConfig {
    /// Retrieves a clone of the current `GlobalConfig` in a thread-safe manner via a read lock.
    pub fn settings() -> Result<GlobalConfig> {
        SETTINGS
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock for settings: {}", e))
            .map(|settings| settings.clone())
    }

    /// Updates the `version` in the `GlobalConfig` in a thread-safe manner via a write lock.
    pub fn update_version(version: String) -> Result<()> {
        SETTINGS
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock for settings: {}", e))
            .map(|mut settings| {
                settings.version = version;
            })
    }
}
