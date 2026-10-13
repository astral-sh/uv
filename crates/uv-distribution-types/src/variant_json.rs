use std::fmt::Display;
use std::str::FromStr;

use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};

#[derive(Debug, thiserror::Error)]
pub enum VariantsJsonFilenameError {
    #[error("Invalid `variants.json` filename")]
    InvalidFilename,
    #[error("Invalid `variants.json` package name: {0}")]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid `variants.json` version: {0}")]
    InvalidVersion(#[from] VersionParseError),
}

/// A `<name>-<version>-variants.json` filename.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantsJsonFilename {
    pub(crate) name: PackageName,
    pub(crate) version: Version,
}

impl FromStr for VariantsJsonFilename {
    type Err = VariantsJsonFilenameError;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let stem = filename
            .strip_suffix("-variants.json")
            .ok_or(VariantsJsonFilenameError::InvalidFilename)?;
        let (name, version) = stem
            .split_once('-')
            .ok_or(VariantsJsonFilenameError::InvalidFilename)?;
        Ok(Self {
            name: PackageName::from_str(name)?,
            version: Version::from_str(version)?,
        })
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
    use std::str::FromStr;

    use super::VariantsJsonFilename;

    #[test]
    fn variants_json_filename() -> Result<(), Box<dyn std::error::Error>> {
        let filename = VariantsJsonFilename::from_str("some_package-1.21.0-variants.json")?;
        assert_eq!(filename.name.as_str(), "some-package");
        assert_eq!(filename.version.to_string(), "1.21.0");
        assert_eq!(filename.to_string(), "some_package-1.21.0-variants.json");
        assert!(VariantsJsonFilename::from_str("some_package-invalid-variants.json").is_err());
        Ok(())
    }
}
