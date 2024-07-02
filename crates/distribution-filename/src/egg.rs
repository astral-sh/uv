use std::str::FromStr;

use thiserror::Error;

use pep440_rs::{Version, VersionParseError};
use uv_normalize::{InvalidNameError, PackageName};

#[derive(Error, Debug)]
pub enum EggInfoFilenameError {
    #[error("The filename \"{0}\" does not end in `.egg-info`")]
    InvalidExtension(String),
    #[error("The `.egg-info` filename \"{0}\" is missing a package name")]
    MissingPackageName(String),
    #[error("The `.egg-info` filename \"{0}\" is missing a version")]
    MissingVersion(String),
    #[error("The `.egg-info` filename \"{0}\" has an invalid package name")]
    InvalidPackageName(String, InvalidNameError),
    #[error("The `.egg-info` filename \"{0}\" has an invalid version: {1}")]
    InvalidVersion(String, VersionParseError),
}

/// A filename parsed from an `.egg-info` file or directory (e.g., `zstandard-0.22.0-py3.12.egg-info`).
///
/// An `.egg-info` filename can contain up to four components, as in:
///
/// ```text
/// name ["-" version ["-py" pyver ["-" required_platform]]] "." ext
/// ```
///
/// See: <https://setuptools.pypa.io/en/latest/deprecated/python_eggs.html#filename-embedded-metadata>
#[derive(Debug, Clone)]
pub struct EggInfoFilename {
    pub name: PackageName,
    pub version: Version,
}

impl EggInfoFilename {
    /// Parse an `.egg-info` filename, requiring at least a name and version.
    pub fn parse(stem: &str) -> Result<Self, EggInfoFilenameError> {
        // pip uses the following regex:
        // ```python
        // EGG_NAME = re.compile(
        //     r"""
        //     (?P<name>[^-]+) (
        //         -(?P<ver>[^-]+) (
        //             -py(?P<pyver>[^-]+) (
        //                 -(?P<plat>.+)
        //             )?
        //         )?
        //     )?
        //     """,
        //     re.VERBOSE | re.IGNORECASE,
        // ).match
        // ```
        let mut parts = stem.split('-');
        let name = parts
            .next()
            .ok_or_else(|| EggInfoFilenameError::MissingPackageName(format!("{stem}.egg-info")))?;
        let version = parts
            .next()
            .ok_or_else(|| EggInfoFilenameError::MissingVersion(format!("{stem}.egg-info")))?;
        let name = PackageName::from_str(name)
            .map_err(|e| EggInfoFilenameError::InvalidPackageName(format!("{stem}.egg-info"), e))?;
        let version = Version::from_str(version)
            .map_err(|e| EggInfoFilenameError::InvalidVersion(format!("{stem}.egg-info"), e))?;
        Ok(Self { name, version })
    }
}

impl FromStr for EggInfoFilename {
    type Err = EggInfoFilenameError;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let stem = filename
            .strip_suffix(".egg-info")
            .ok_or_else(|| EggInfoFilenameError::InvalidExtension(filename.to_string()))?;
        Self::parse(stem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egg_info_filename() {
        let filename = "zstandard-0.22.0-py3.12-darwin.egg-info";
        let parsed = EggInfoFilename::from_str(filename).unwrap();
        assert_eq!(parsed.name.as_ref(), "zstandard");
        assert_eq!(parsed.version.to_string(), "0.22.0");

        let filename = "zstandard-0.22.0-py3.12.egg-info";
        let parsed = EggInfoFilename::from_str(filename).unwrap();
        assert_eq!(parsed.name.as_ref(), "zstandard");
        assert_eq!(parsed.version.to_string(), "0.22.0");

        let filename = "zstandard-0.22.0.egg-info";
        let parsed = EggInfoFilename::from_str(filename).unwrap();
        assert_eq!(parsed.name.as_ref(), "zstandard");
        assert_eq!(parsed.version.to_string(), "0.22.0");
    }

    #[test]
    fn egg_info_filename_missing_version() {
        let filename = "zstandard.egg-info";
        let err = EggInfoFilename::from_str(filename).unwrap_err();
        assert_eq!(
            err.to_string(),
            "The `.egg-info` filename \"zstandard.egg-info\" is missing a version"
        );
    }
}
