use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::{LibcVersion, PlatformTag};

/// The selected Linux libc implementation and its oldest supported release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MinimumLibcVersion {
    Glibc(#[cfg_attr(feature = "schemars", schemars(with = "String"))] LibcVersion),
    Musl(#[cfg_attr(feature = "schemars", schemars(with = "String"))] LibcVersion),
}

impl Display for MinimumLibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Glibc(version) => write!(formatter, "glibc {version}"),
            Self::Musl(version) => write!(formatter, "musl {version}"),
        }
    }
}

impl MinimumLibcVersion {
    /// Return whether any platform tag is permitted by the libc cutoff.
    pub fn allows_wheel(self, filename: &WheelFilename) -> bool {
        filename
            .platform_tags()
            .iter()
            .any(|tag| self.allows_platform(tag))
    }

    /// Reject the other libc implementation and releases newer than the selected baseline.
    ///
    /// Native Linux tags declare no libc version, so accepting them does not establish a libc
    /// compatibility guarantee. Non-Linux tags are unconstrained.
    pub fn allows_platform(self, platform: &PlatformTag) -> bool {
        let minimum = match platform {
            PlatformTag::Manylinux { major, minor, .. } => {
                Self::Glibc(LibcVersion::new(*major, *minor))
            }
            PlatformTag::Manylinux1 { .. } => Self::Glibc(LibcVersion::new(2, 5)),
            PlatformTag::Manylinux2010 { .. } => Self::Glibc(LibcVersion::new(2, 12)),
            PlatformTag::Manylinux2014 { .. } => Self::Glibc(LibcVersion::new(2, 17)),
            PlatformTag::Musllinux { major, minor, .. } => {
                Self::Musl(LibcVersion::new(*major, *minor))
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
            | PlatformTag::Ios { .. } => return true,
        };
        match (self, minimum) {
            (Self::Glibc(version), Self::Glibc(minimum))
            | (Self::Musl(version), Self::Musl(minimum)) => minimum <= version,
            (Self::Glibc(_), Self::Musl(_)) | (Self::Musl(_), Self::Glibc(_)) => false,
        }
    }
}
