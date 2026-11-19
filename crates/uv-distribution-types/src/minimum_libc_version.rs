use serde::{Deserialize, Serialize};

use uv_platform_tags::{LibcVersion, PlatformTag};

/// The oldest required release of each Linux libc.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MinimumLibcVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub glibc: Option<LibcVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub musl: Option<LibcVersion>,
}

impl MinimumLibcVersion {
    /// Combine the libc versions advertised by two sets of wheels, taking the oldest
    /// release of each family for which either set has a wheel.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            glibc: self.glibc.into_iter().chain(other.glibc).min(),
            musl: self.musl.into_iter().chain(other.musl).min(),
        }
    }

    /// Whether a platform tag covers the glibc and musl baselines, respectively.
    ///
    /// Unconfigured baselines accept any platform. Generic Linux tags do not constrain libc,
    /// matching installation behavior. Non-Linux tags are unaffected.
    pub fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        let Some(required) = Self::from_platform_tag(platform) else {
            return [true; 2];
        };
        [
            self.glibc
                .is_none_or(|baseline| required.glibc.is_some_and(|version| version <= baseline)),
            self.musl
                .is_none_or(|baseline| required.musl.is_some_and(|version| version <= baseline)),
        ]
    }

    /// The oldest libc release supported by a manylinux or musllinux tag.
    pub fn from_platform_tag(platform: &PlatformTag) -> Option<Self> {
        let glibc = match platform {
            PlatformTag::Manylinux { major, minor, .. } => LibcVersion::new(*major, *minor),
            PlatformTag::Manylinux1 { .. } => LibcVersion::new(2, 5),
            PlatformTag::Manylinux2010 { .. } => LibcVersion::new(2, 12),
            PlatformTag::Manylinux2014 { .. } => LibcVersion::new(2, 17),
            PlatformTag::Musllinux { major, minor, .. } => {
                return Some(Self {
                    glibc: None,
                    musl: Some(LibcVersion::new(*major, *minor)),
                });
            }
            PlatformTag::Linux { .. }
            | PlatformTag::Any
            | PlatformTag::Macos { .. }
            | PlatformTag::Win32
            | PlatformTag::WinAmd64
            | PlatformTag::WinArm64
            | PlatformTag::WinIa64
            | PlatformTag::Android { .. }
            | PlatformTag::FreeBsd { .. }
            | PlatformTag::NetBsd { .. }
            | PlatformTag::OpenBsd { .. }
            | PlatformTag::Dragonfly { .. }
            | PlatformTag::Haiku { .. }
            | PlatformTag::Illumos { .. }
            | PlatformTag::Solaris { .. }
            | PlatformTag::Pyodide { .. }
            | PlatformTag::PyEmscripten { .. }
            | PlatformTag::Ios { .. } => return None,
        };
        Some(Self {
            glibc: Some(glibc),
            musl: None,
        })
    }
}
