use std::fmt::{Display, Formatter};
use std::str::FromStr;

use thiserror::Error;
use url::Url;

use pep440_rs::Version;
use platform_tags::Tags;
use puffin_normalize::{InvalidNameError, PackageName};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WheelFilename {
    pub distribution: PackageName,
    pub version: Version,
    pub python_tag: Vec<String>,
    pub abi_tag: Vec<String>,
    pub platform_tag: Vec<String>,
}

impl FromStr for WheelFilename {
    type Err = WheelFilenameError;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let basename = filename.strip_suffix(".whl").ok_or_else(|| {
            WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must end with .whl".to_string(),
            )
        })?;

        // The wheel filename should contain either five or six entries. If six, then the third
        // entry is the build tag. If five, then the third entry is the Python tag.
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        //
        // 2023-11-08(burntsushi): It looks like the code below actually drops
        // the build tag if one is found. According to PEP 0427, the build tag
        // is used to break ties. This might mean that we generate identical
        // `WheelName` values for multiple distinct wheels, but it's not clear
        // if this is a problem in practice.
        let mut parts = basename.split('-');

        let Some(distribution) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a distribution name".to_string(),
            ));
        };

        let Some(version) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a version".to_string(),
            ));
        };

        let Some(build_tag_or_python_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a Python tag".to_string(),
            ));
        };

        let Some(python_tag_or_abi_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have an ABI tag".to_string(),
            ));
        };

        let Some(abi_tag_or_platform_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a platform tag".to_string(),
            ));
        };

        let (distribution, version, python_tag, abi_tag, platform_tag) =
            if let Some(platform_tag) = parts.next() {
                (
                    distribution,
                    version,
                    python_tag_or_abi_tag,
                    abi_tag_or_platform_tag,
                    platform_tag,
                )
            } else {
                (
                    distribution,
                    version,
                    build_tag_or_python_tag,
                    python_tag_or_abi_tag,
                    abi_tag_or_platform_tag,
                )
            };

        let distribution = PackageName::from_str(distribution)
            .map_err(|err| WheelFilenameError::InvalidPackageName(filename.to_string(), err))?;
        let version = Version::from_str(version)
            .map_err(|err| WheelFilenameError::InvalidVersion(filename.to_string(), err))?;
        Ok(WheelFilename {
            distribution,
            version,
            python_tag: python_tag.split('.').map(String::from).collect(),
            abi_tag: abi_tag.split('.').map(String::from).collect(),
            platform_tag: platform_tag.split('.').map(String::from).collect(),
        })
    }
}

impl Display for WheelFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}.whl",
            self.distribution,
            self.version,
            self.get_tag()
        )
    }
}

impl WheelFilename {
    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, compatible_tags: &Tags) -> bool {
        for tag in compatible_tags.iter() {
            if self.python_tag.contains(&tag.0)
                && self.abi_tag.contains(&tag.1)
                && self.platform_tag.contains(&tag.2)
            {
                return true;
            }
        }
        false
    }

    /// Get the tag for this wheel.
    pub fn get_tag(&self) -> String {
        format!(
            "{}-{}-{}",
            self.python_tag.join("."),
            self.abi_tag.join("."),
            self.platform_tag.join(".")
        )
    }
}

impl TryFrom<&Url> for WheelFilename {
    type Error = WheelFilenameError;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let filename = url
            .path_segments()
            .ok_or_else(|| {
                WheelFilenameError::InvalidWheelFileName(
                    url.to_string(),
                    "URL must have a path".to_string(),
                )
            })?
            .last()
            .ok_or_else(|| {
                WheelFilenameError::InvalidWheelFileName(
                    url.to_string(),
                    "URL must contain a filename".to_string(),
                )
            })?;
        Self::from_str(filename)
    }
}

#[derive(Error, Debug)]
pub enum WheelFilenameError {
    #[error("The wheel filename \"{0}\" is invalid: {1}")]
    InvalidWheelFileName(String, String),
    #[error("The wheel filename \"{0}\" has an invalid version part: {1}")]
    InvalidVersion(String, String),
    #[error("The wheel filename \"{0}\" has an invalid package name")]
    InvalidPackageName(String, InvalidNameError),
}
