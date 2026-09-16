use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::{LibcVersion, PlatformTag};

/// The selected Linux libc families and their oldest supported releases.
///
/// At least one family must be specified. An omitted family is excluded, not unconstrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(extend("minProperties" = 1)))]
pub struct MinimumLibcVersion {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    glibc: Option<LibcVersion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    musl: Option<LibcVersion>,
}

impl<'de> Deserialize<'de> for MinimumLibcVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            glibc: Option<LibcVersion>,
            musl: Option<LibcVersion>,
        }

        let Wire { glibc, musl } = Wire::deserialize(deserializer)?;
        if glibc.is_none() && musl.is_none() {
            return Err(serde::de::Error::custom(
                "at least one of `glibc` or `musl` must be specified",
            ));
        }
        Ok(Self { glibc, musl })
    }
}

impl Display for MinimumLibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match (self.glibc, self.musl) {
            (Some(glibc), Some(musl)) => write!(formatter, "glibc {glibc} or musl {musl}"),
            (Some(glibc), None) => write!(formatter, "glibc {glibc}"),
            (None, Some(musl)) => write!(formatter, "musl {musl}"),
            (None, None) => formatter.write_str("the selected libc versions"),
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

    /// Reject omitted libc families and releases newer than the selected family baseline.
    ///
    /// Native Linux tags declare no libc version, so accepting them does not establish a libc
    /// compatibility guarantee. Non-Linux tags are unconstrained.
    pub fn allows_platform(self, platform: &PlatformTag) -> bool {
        match platform {
            PlatformTag::Manylinux { major, minor, .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(*major, *minor) <= version),
            PlatformTag::Manylinux1 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 5) <= version),
            PlatformTag::Manylinux2010 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 12) <= version),
            PlatformTag::Manylinux2014 { .. } => self
                .glibc
                .is_some_and(|version| LibcVersion::new(2, 17) <= version),
            PlatformTag::Musllinux { major, minor, .. } => self
                .musl
                .is_some_and(|version| LibcVersion::new(*major, *minor) <= version),
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
            | PlatformTag::Ios { .. } => true,
        }
    }
}
