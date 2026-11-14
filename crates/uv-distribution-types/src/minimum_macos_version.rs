use std::fmt::{Display, Formatter};
use std::str::FromStr;

use uv_platform_tags::{Arch, BinaryFormat, PlatformTag, compatible_macos_tags};

/// The oldest required macOS release, expressed as a deployment target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MinimumMacosVersion(u16, u16);

impl MinimumMacosVersion {
    /// Whether this wheel can be installed at the configured floor on `machine`.
    pub fn supports(self, tag: &PlatformTag, machine: BinaryFormat) -> bool {
        let arch = match machine {
            BinaryFormat::X86_64 => Arch::X86_64,
            // No ARM Mac runs 10.x, even if a universal2 wheel advertises a 10.x target.
            BinaryFormat::Arm64 if self.0 >= 11 => Arch::Aarch64,
            _ => return false,
        };
        compatible_macos_tags(self.0, self.1, arch).any(|candidate| &candidate == tag)
    }

    fn normalized(major: u16, minor: u16) -> Self {
        if (major, minor) == (10, 16) {
            Self(11, 0)
        } else {
            Self(major, minor)
        }
    }
}

impl FromStr for MinimumMacosVersion {
    type Err = ParseMinimumMacosVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (major, minor) = value.split_once('.').ok_or(ParseMinimumMacosVersionError)?;
        if !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseMinimumMacosVersionError);
        }
        Ok(Self::normalized(
            major.parse().map_err(|_| ParseMinimumMacosVersionError)?,
            minor.parse().map_err(|_| ParseMinimumMacosVersionError)?,
        ))
    }
}

impl Display for MinimumMacosVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.0, self.1)
    }
}

impl serde::Serialize for MinimumMacosVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for MinimumMacosVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("expected a macOS version in the form `<major>.<minor>` (e.g., `14.0`)")]
pub struct ParseMinimumMacosVersionError;
