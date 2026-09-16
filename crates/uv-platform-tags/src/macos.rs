use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

use rustc_hash::FxHashSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::tags::compatible_tags;
use crate::{Arch, BinaryFormat, Os, Platform, PlatformError, PlatformTag};

/// The minimum supported macOS version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacosDeploymentTarget {
    pub major: u16,
    pub minor: u16,
}

impl FromStr for MacosDeploymentTarget {
    type Err = ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split('.');
        Ok(Self {
            major: parts.next().unwrap_or_default().parse()?,
            minor: parts.next().unwrap_or("0").parse()?,
        })
    }
}

impl fmt::Display for MacosDeploymentTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl<'de> Deserialize<'de> for MacosDeploymentTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl Serialize for MacosDeploymentTarget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Platform tags compatible with a macOS deployment target, for both supported architectures.
#[derive(Debug, PartialEq, Eq)]
pub struct MacosPlatformTags {
    target: MacosDeploymentTarget,
    x86_64: FxHashSet<PlatformTag>,
    arm64: FxHashSet<PlatformTag>,
}

impl MacosPlatformTags {
    /// Generate the same platform tags used for concrete macOS resolutions.
    pub fn new(target: MacosDeploymentTarget) -> Result<Self, PlatformError> {
        let os = Os::Macos {
            major: target.major,
            minor: target.minor,
        };
        Ok(Self {
            target,
            x86_64: compatible_tags(&Platform::new(os.clone(), Arch::X86_64))?
                .into_iter()
                .collect(),
            arm64: compatible_tags(&Platform::new(os, Arch::Aarch64))?
                .into_iter()
                .collect(),
        })
    }

    /// Whether the tag can be installed on the given architecture at the deployment target.
    pub fn contains(&self, tag: &PlatformTag, arch: &BinaryFormat) -> bool {
        (*arch == BinaryFormat::X86_64 && self.x86_64.contains(tag))
            || (*arch == BinaryFormat::Arm64 && self.arm64.contains(tag))
    }

    pub fn deployment_target(&self) -> MacosDeploymentTarget {
        self.target
    }
}
