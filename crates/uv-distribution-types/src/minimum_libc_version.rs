use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use uv_distribution_filename::WheelFilename;
use uv_platform_tags::{LibcVersion, PlatformTag};

/// The oldest supported release of each Linux libc, or an explicit exclusion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MinimumLibcVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    glibc: Option<LibcConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    musl: Option<LibcConstraint>,
}

impl Display for MinimumLibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let mut separator = "";
        for (name, constraint) in [("glibc", self.glibc), ("musl", self.musl)] {
            if let Some(constraint) = constraint {
                formatter.write_str(separator)?;
                match constraint {
                    LibcConstraint::Version(version) => write!(formatter, "{name} {version}")?,
                    LibcConstraint::Excluded => write!(formatter, "{name} excluded")?,
                }
                separator = ", ";
            }
        }
        Ok(())
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

    /// Reject explicitly excluded libc implementations and releases newer than their baseline.
    ///
    /// Native Linux tags declare no libc version, so accepting them does not establish a libc
    /// compatibility guarantee. Non-Linux tags are unconstrained.
    pub fn allows_platform(self, platform: &PlatformTag) -> bool {
        let (constraint, minimum) = match platform {
            PlatformTag::Manylinux { major, minor, .. } => {
                (self.glibc, LibcVersion::new(*major, *minor))
            }
            PlatformTag::Manylinux1 { .. } => (self.glibc, LibcVersion::new(2, 5)),
            PlatformTag::Manylinux2010 { .. } => (self.glibc, LibcVersion::new(2, 12)),
            PlatformTag::Manylinux2014 { .. } => (self.glibc, LibcVersion::new(2, 17)),
            PlatformTag::Musllinux { major, minor, .. } => {
                (self.musl, LibcVersion::new(*major, *minor))
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
        match constraint {
            Some(LibcConstraint::Version(version)) => minimum <= version,
            Some(LibcConstraint::Excluded) => false,
            None => true,
        }
    }

    /// Restrict coverage to each configured baseline independently. Retained wheels for another
    /// libc cannot satisfy it; when both baselines are set, both must have coverage.
    pub(crate) fn coverage(self) -> [Self; 2] {
        let mut glibc = self;
        if let Some(LibcConstraint::Version(_)) = self.glibc {
            glibc.musl = Some(LibcConstraint::Excluded);
        }
        let mut musl = self;
        if let Some(LibcConstraint::Version(_)) = self.musl {
            musl.glibc = Some(LibcConstraint::Excluded);
        }
        [glibc, musl]
    }
}

/// A libc baseline, or `false` to exclude wheels for that implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibcConstraint {
    Version(LibcVersion),
    Excluded,
}

impl Serialize for LibcConstraint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Version(version) => version.serialize(serializer),
            Self::Excluded => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for LibcConstraint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = LibcConstraint;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a libc version string or `false`")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value
                    .parse()
                    .map(LibcConstraint::Version)
                    .map_err(E::custom)
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                if value {
                    Err(E::invalid_value(serde::de::Unexpected::Bool(value), &self))
                } else {
                    Ok(LibcConstraint::Excluded)
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for LibcConstraint {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "LibcConstraint".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                { "type": "string" },
                { "const": false },
            ],
        })
    }
}
