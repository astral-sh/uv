use std::fmt::Display;
use std::str::FromStr;

use uv_cache_key::cache_digest;
use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};

#[derive(Debug, thiserror::Error)]
pub enum VariantsJsonError {
    #[error("Invalid `variants.json` filename")]
    InvalidFilename,
    #[error("Invalid `variants.json` package name: {0}")]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid `variants.json` version: {0}")]
    InvalidVersion(#[from] VersionParseError),
}

/// A `<name>-<version>-variants.json` filename.
#[derive(
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct VariantsJsonFilename {
    pub name: PackageName,
    pub version: Version,
}

impl VariantsJsonFilename {
    /// Returns a consistent cache key with a maximum length of 64 characters.
    pub fn cache_key(&self) -> String {
        const CACHE_KEY_MAX_LEN: usize = 64;

        let mut cache_key = self.version.to_string();

        if cache_key.len() <= CACHE_KEY_MAX_LEN {
            return cache_key;
        }

        let digest = cache_digest(&cache_key);
        // PANIC SAFETY: version strings can only contain ASCII characters.
        cache_key.truncate(CACHE_KEY_MAX_LEN - 1 - digest.len());
        let cache_key = cache_key.trim_end_matches(['.', '+']);

        format!("{cache_key}-{digest}")
    }
}

impl FromStr for VariantsJsonFilename {
    type Err = VariantsJsonError;

    /// Parse a `<name>-<version>-variants.json` filename.
    ///
    /// name and version must be normalized, i.e., they don't contain dashes.
    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let stem = filename
            .strip_suffix("-variants.json")
            .ok_or(VariantsJsonError::InvalidFilename)?;

        let (name, version) = stem
            .split_once('-')
            .ok_or(VariantsJsonError::InvalidFilename)?;
        let name = PackageName::from_str(name)?;
        let version = Version::from_str(version)?;

        Ok(Self { name, version })
    }
}

impl Display for VariantsJsonFilename {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-variants.json",
            self.name.as_dist_info_name(),
            self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_json_parsing() -> Result<(), VariantsJsonError> {
        let variant = VariantsJsonFilename::from_str("some_package-1.21.0-variants.json")?;
        assert_eq!(variant.name.as_str(), "some-package");
        assert_eq!(variant.version.to_string(), "1.21.0");
        assert_eq!(variant.to_string(), "some_package-1.21.0-variants.json");
        Ok(())
    }

    #[test]
    fn long_versions_have_distinct_cache_keys() -> Result<(), VariantsJsonError> {
        let prefix = "1.".repeat(35);
        let first = VariantsJsonFilename::from_str(&format!("example-{prefix}1-variants.json"))?;
        let second = VariantsJsonFilename::from_str(&format!("example-{prefix}2-variants.json"))?;

        assert!(first.cache_key().len() <= 64);
        assert!(second.cache_key().len() <= 64);
        assert_ne!(first.cache_key(), second.cache_key());
        Ok(())
    }
}
