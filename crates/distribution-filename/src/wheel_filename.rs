use std::fmt::{Display, Formatter};
use std::str::FromStr;

use pep440_rs::Version;
use thiserror::Error;

use platform_tags::Tags;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WheelFilename {
    pub distribution: String,
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
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        match basename.split('-').collect::<Vec<_>>().as_slice() {
            // TODO(charlie): Build tag precedence
            &[distribution, version, _, python_tag, abi_tag, platform_tag]
            | &[distribution, version, python_tag, abi_tag, platform_tag] => {
                let version = Version::from_str(version)
                    .map_err(|err| WheelFilenameError::InvalidVersion(filename.to_string(), err))?;
                Ok(WheelFilename {
                    distribution: distribution.to_string(),
                    version,
                    python_tag: python_tag.split('.').map(String::from).collect(),
                    abi_tag: abi_tag.split('.').map(String::from).collect(),
                    platform_tag: platform_tag.split('.').map(String::from).collect(),
                })
            }
            _ => Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Expected four \"-\" in the filename".to_string(),
            )),
        }
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

#[derive(Error, Debug)]
pub enum WheelFilenameError {
    #[error("The wheel filename \"{0}\" is invalid: {1}")]
    InvalidWheelFileName(String, String),
    #[error("The wheel filename \"{0}\" has an invalid version part: {1}")]
    InvalidVersion(String, String),
}
