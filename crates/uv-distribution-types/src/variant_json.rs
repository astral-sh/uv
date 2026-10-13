use std::fmt::Display;
use std::str::FromStr;

use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};

#[derive(Debug, thiserror::Error)]
pub enum VariantsJsonFilenameError {
    #[error("Invalid `variants.json` filename: `{0}`")]
    InvalidFilename(String),
    #[error("Invalid `variants.json` package name: {0}")]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid `variants.json` version `{0}`: {1}")]
    InvalidVersion(String, #[source] VersionParseError),
}

/// A `<name>-<version>-variants.json` filename.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantsJsonFilename {
    pub(crate) name: PackageName,
    pub version: Version,
}

impl FromStr for VariantsJsonFilename {
    type Err = VariantsJsonFilenameError;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let (name, version) = filename
            .strip_suffix("-variants.json")
            .and_then(|stem| stem.split_once('-'))
            .ok_or_else(|| VariantsJsonFilenameError::InvalidFilename(filename.to_string()))?;
        Ok(Self {
            name: PackageName::from_str(name)?,
            version: Version::from_str(version).map_err(|err| {
                VariantsJsonFilenameError::InvalidVersion(version.to_string(), err)
            })?,
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
