use serde::{Deserialize, Serialize};

use uv_platform_tags::{LibcVersion, PlatformTag};

/// The oldest required release of each Linux libc.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MinimumLibcVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    glibc: Option<LibcVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    musl: Option<LibcVersion>,
}

impl MinimumLibcVersion {
    /// Whether a platform tag covers the glibc and musl baselines, respectively.
    ///
    /// Unconfigured baselines accept any platform. Generic Linux tags do not constrain libc,
    /// matching installation behavior. Non-Linux tags are unaffected.
    pub fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        LinuxLibc::from_platform_tag(platform).map_or([true; 2], |floor| self.libc_coverage(floor))
    }

    /// Whether a wheel's libc requirement satisfies the two configured baselines.
    pub fn libc_coverage(self, floor: LinuxLibc) -> [bool; 2] {
        match floor {
            LinuxLibc::Glibc(required) => [
                self.glibc.is_none_or(|version| required <= version),
                self.musl.is_none(),
            ],
            LinuxLibc::Musl(required) => [
                self.glibc.is_none(),
                self.musl.is_none_or(|version| required <= version),
            ],
        }
    }
}

/// The libc family and minimum version required by a Linux wheel tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinuxLibc {
    Glibc(LibcVersion),
    Musl(LibcVersion),
}

impl LinuxLibc {
    /// The libc requirement of a manylinux or musllinux platform tag.
    pub fn from_platform_tag(tag: &PlatformTag) -> Option<Self> {
        match tag {
            PlatformTag::Manylinux { major, minor, .. } => {
                Some(Self::Glibc(LibcVersion::new(*major, *minor)))
            }
            PlatformTag::Manylinux1 { .. } => Some(Self::Glibc(LibcVersion::new(2, 5))),
            PlatformTag::Manylinux2010 { .. } => Some(Self::Glibc(LibcVersion::new(2, 12))),
            PlatformTag::Manylinux2014 { .. } => Some(Self::Glibc(LibcVersion::new(2, 17))),
            PlatformTag::Musllinux { major, minor, .. } => {
                Some(Self::Musl(LibcVersion::new(*major, *minor)))
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
            | PlatformTag::Ios { .. } => None,
        }
    }

    /// Whether this libc can run a wheel with the given requirement.
    pub fn supports(self, required: Self) -> bool {
        match (self, required) {
            (Self::Glibc(installed), Self::Glibc(minimum))
            | (Self::Musl(installed), Self::Musl(minimum)) => installed.supports_wheel(minimum),
            (Self::Glibc(_), Self::Musl(_)) | (Self::Musl(_), Self::Glibc(_)) => false,
        }
    }
}
