use std::{fmt::Display, str::FromStr};
use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};

#[derive(Debug, thiserror::Error)]
pub enum VariantsJsonError {
    #[error("Invalid variants.json filename")]
    InvalidFilename,
    #[error("Invalid variants.json package name: {0}")]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid variants.json version: {0}")]
    InvalidVersion(#[from] VersionParseError),
}

/// A `<name>-<version>-variants.json` file.
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
pub struct VariantsJson {
    pub name: PackageName,
    pub version: Version,
}

impl VariantsJson {
    /// Returns a consistent cache key with a maximum length of 64 characters.
    pub fn cache_key(&self) -> String {
        const CACHE_KEY_MAX_LEN: usize = 64;

        let mut cache_key = self.version.to_string();

        if cache_key.len() <= CACHE_KEY_MAX_LEN {
            return cache_key;
        }

        // PANIC SAFETY: version strings can only contain ASCII characters.
        cache_key.truncate(CACHE_KEY_MAX_LEN);
        let cache_key = cache_key.trim_end_matches(['.', '+']);

        cache_key.to_string()
    }
}

impl FromStr for VariantsJson {
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

impl Display for VariantsJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-variants.json", self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_json_parsing() {
        let variant = VariantsJson::from_str("numpy-1.21.0-variants.json").unwrap();
        assert_eq!(variant.name.as_str(), "numpy");
        assert_eq!(variant.version.to_string(), "1.21.0");
    }
}
