use std::fmt;
use std::sync::LazyLock;

use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use uv_distribution_filename::WheelFilename;
use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString, marker};
use uv_platform_tags::{Os, PlatformTag};
use uv_pypi_types::{SupportedEnvironments, deserialize_marker_entries};

use crate::{implied_platform_markers_with_aliases, implied_python_markers};

static LINUX: LazyLock<MarkerTree> = LazyLock::new(|| {
    MarkerTree::expression(MarkerExpression::String {
        key: MarkerValueString::SysPlatform,
        operator: MarkerOperator::Equal,
        value: "linux".into(),
    })
});

/// The Linux libc family selected by a supported environment.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum LinuxLibc {
    Glibc,
    Musl,
}

impl LinuxLibc {
    /// Return the family required by a platform tag. Generic Linux wheels can run on either.
    pub fn from_tag(tag: &PlatformTag) -> Option<Self> {
        match tag {
            PlatformTag::Manylinux { .. }
            | PlatformTag::Manylinux1 { .. }
            | PlatformTag::Manylinux2010 { .. }
            | PlatformTag::Manylinux2014 { .. } => Some(Self::Glibc),
            PlatformTag::Musllinux { .. } => Some(Self::Musl),
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

    /// Return the libc family of a Linux installation target.
    pub fn from_os(os: &Os) -> Option<Self> {
        match os {
            Os::Manylinux { .. } => Some(Self::Glibc),
            Os::Musllinux { .. } => Some(Self::Musl),
            Os::Windows
            | Os::Macos { .. }
            | Os::FreeBsd { .. }
            | Os::NetBsd { .. }
            | Os::OpenBsd { .. }
            | Os::Dragonfly { .. }
            | Os::Haiku { .. }
            | Os::Illumos { .. }
            | Os::Pyodide { .. }
            | Os::PyEmscripten { .. }
            | Os::Android { .. }
            | Os::Ios { .. } => None,
        }
    }
}

impl fmt::Display for LinuxLibc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Glibc => "glibc",
            Self::Musl => "musl",
        })
    }
}

/// One entry in `tool.uv.environments`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
enum ProjectEnvironment {
    Marker(#[serde(serialize_with = "marker::ser::serialize_explicit")] MarkerTree),
    Linux(LinuxEnvironment),
}

/// A marker whose Linux targets are restricted to one libc family.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct LinuxEnvironment {
    #[serde(serialize_with = "marker::ser::serialize_explicit")]
    marker: MarkerTree,
    libc: LinuxLibc,
}

impl From<MarkerTree> for ProjectEnvironment {
    fn from(marker: MarkerTree) -> Self {
        Self::Marker(marker)
    }
}

impl<'de> Deserialize<'de> for ProjectEnvironment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvironmentVisitor;
        impl<'de> Visitor<'de> for EnvironmentVisitor {
            type Value = ProjectEnvironment;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a marker string or a table with `marker` and `libc`")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value
                    .parse()
                    .map(ProjectEnvironment::Marker)
                    .map_err(E::custom)
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                LinuxEnvironment::deserialize(MapAccessDeserializer::new(map))
                    .map(ProjectEnvironment::Linux)
            }
        }
        deserializer.deserialize_any(EnvironmentVisitor)
    }
}

impl ProjectEnvironment {
    fn marker(self) -> MarkerTree {
        match self {
            Self::Marker(marker) => marker,
            Self::Linux(entry) => entry.marker.and(*LINUX),
        }
    }
}

/// The supported environments declared by a project.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectEnvironments(
    #[serde(deserialize_with = "deserialize_marker_entries")] Vec<ProjectEnvironment>,
);

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ProjectEnvironments {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ProjectEnvironments".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                generator.subschema_for::<String>(),
                generator.subschema_for::<Vec<ProjectEnvironment>>(),
            ],
        })
    }
}

impl ProjectEnvironments {
    /// Return the PEP 508 domains used for resolution forks.
    pub fn markers(&self) -> SupportedEnvironments {
        SupportedEnvironments::from_markers(self.0.iter().map(|entry| entry.marker()).collect())
    }

    /// Collect the Linux environments in which each libc family is supported.
    pub fn libc_environments(&self) -> Option<LibcEnvironments> {
        if self
            .0
            .iter()
            .all(|entry| matches!(entry, ProjectEnvironment::Marker(_)))
        {
            return None;
        }
        let mut glibc = MarkerTree::FALSE;
        let mut musl = MarkerTree::FALSE;
        for entry in &self.0 {
            let linux = entry.marker().and(*LINUX);
            match entry {
                ProjectEnvironment::Marker(_) => {
                    glibc = glibc.or(linux);
                    musl = musl.or(linux);
                }
                ProjectEnvironment::Linux(LinuxEnvironment {
                    libc: LinuxLibc::Glibc,
                    ..
                }) => glibc = glibc.or(linux),
                ProjectEnvironment::Linux(LinuxEnvironment {
                    libc: LinuxLibc::Musl,
                    ..
                }) => musl = musl.or(linux),
            }
        }
        Some(LibcEnvironments { glibc, musl })
    }
}

/// The supported Linux marker domains, persisted in the lock for frozen installations.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct LibcEnvironments {
    #[serde(
        default = "empty_marker",
        skip_serializing_if = "marker::ser::is_false",
        serialize_with = "marker::ser::serialize_explicit"
    )]
    glibc: MarkerTree,
    #[serde(
        default = "empty_marker",
        skip_serializing_if = "marker::ser::is_false",
        serialize_with = "marker::ser::serialize_explicit"
    )]
    musl: MarkerTree,
}

fn empty_marker() -> MarkerTree {
    MarkerTree::FALSE
}

impl LibcEnvironments {
    /// A wheel is reachable if any of its platform tags is usable in the package's marker domain.
    pub fn allows_wheel(self, filename: &WheelFilename, marker: MarkerTree) -> bool {
        let tags = filename.platform_tags();
        if !tags.iter().any(|tag| LinuxLibc::from_tag(tag).is_some()) {
            return true;
        }
        let python = implied_python_markers(filename).and(marker);
        tags.iter().any(|tag| {
            let platform = implied_platform_markers_with_aliases(tag);
            // Unknown platform tags cannot prove a wheel is unreachable.
            if platform.is_false() {
                return true;
            }
            let allowed =
                LinuxLibc::from_tag(tag).map_or(MarkerTree::TRUE, |libc| self.marker(libc));
            !platform.and(python).is_disjoint(allowed)
        })
    }

    /// Return the supported marker domain for a libc family.
    pub fn marker(self, libc: LinuxLibc) -> MarkerTree {
        match libc {
            LinuxLibc::Glibc => self.glibc,
            LinuxLibc::Musl => self.musl,
        }
    }
}
