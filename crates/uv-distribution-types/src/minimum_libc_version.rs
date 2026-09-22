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
    pub(crate) fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        let glibc = match platform {
            PlatformTag::Manylinux { major, minor, .. } => LibcVersion::new(*major, *minor),
            PlatformTag::Manylinux1 { .. } => LibcVersion::new(2, 5),
            PlatformTag::Manylinux2010 { .. } => LibcVersion::new(2, 12),
            PlatformTag::Manylinux2014 { .. } => LibcVersion::new(2, 17),
            PlatformTag::Musllinux { major, minor, .. } => {
                return [
                    self.glibc.is_none(),
                    self.musl
                        .is_none_or(|version| LibcVersion::new(*major, *minor) <= version),
                ];
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
            | PlatformTag::Ios { .. } => return [true; 2],
        };
        [
            self.glibc.is_none_or(|version| glibc <= version),
            self.musl.is_none(),
        ]
    }
}
