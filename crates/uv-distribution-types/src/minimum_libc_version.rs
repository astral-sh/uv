use serde::{Deserialize, Serialize};

use uv_platform_tags::{Arch, LibcVersion, PlatformTag};

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
    /// Whether a platform tag covers the glibc and musl baselines, respectively.
    ///
    /// Unconfigured baselines accept any platform. Generic Linux tags do not constrain libc,
    /// matching installation behavior. Non-Linux tags are unaffected.
    pub fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        match LibcTag::from_platform_tag(platform) {
            Some(LibcTag::Glibc(_, minimum)) => [
                self.glibc.is_none_or(|baseline| minimum <= baseline),
                self.musl.is_none(),
            ],
            Some(LibcTag::Musl(_, minimum)) => [
                self.glibc.is_none(),
                self.musl.is_none_or(|baseline| minimum <= baseline),
            ],
            None => [true; 2],
        }
    }
}

/// The architecture and minimum libc version advertised by a Linux wheel tag.
#[derive(Debug, Clone, Copy)]
pub enum LibcTag {
    Glibc(Arch, LibcVersion),
    Musl(Arch, LibcVersion),
}

impl LibcTag {
    /// Extract the libc requirement of a manylinux or musllinux tag.
    pub fn from_platform_tag(platform: &PlatformTag) -> Option<Self> {
        match *platform {
            PlatformTag::Manylinux { major, minor, arch } => {
                Some(Self::Glibc(arch, LibcVersion::new(major, minor)))
            }
            PlatformTag::Manylinux1 { arch } => Some(Self::Glibc(arch, LibcVersion::new(2, 5))),
            PlatformTag::Manylinux2010 { arch } => Some(Self::Glibc(arch, LibcVersion::new(2, 12))),
            PlatformTag::Manylinux2014 { arch } => Some(Self::Glibc(arch, LibcVersion::new(2, 17))),
            PlatformTag::Musllinux { major, minor, arch } => {
                Some(Self::Musl(arch, LibcVersion::new(major, minor)))
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
}
