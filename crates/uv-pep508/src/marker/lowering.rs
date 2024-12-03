use std::fmt::{Display, Formatter};

use uv_normalize::ExtraName;

use crate::{MarkerValueExtra, MarkerValueString, MarkerValueVersion};

/// Those environment markers with a PEP 440 version as value such as `python_version`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum CanonicalMarkerValueVersion {
    /// `implementation_version`
    ImplementationVersion,
    /// `python_full_version`
    PythonFullVersion,
}

impl Display for CanonicalMarkerValueVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
        }
    }
}

impl From<CanonicalMarkerValueVersion> for MarkerValueVersion {
    fn from(value: CanonicalMarkerValueVersion) -> Self {
        match value {
            CanonicalMarkerValueVersion::ImplementationVersion => Self::ImplementationVersion,
            CanonicalMarkerValueVersion::PythonFullVersion => Self::PythonFullVersion,
        }
    }
}

/// Those environment markers with an arbitrary string as value such as `sys_platform`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CanonicalMarkerValueString {
    /// `implementation_name`
    ImplementationName,
    /// `os_name`
    OsName,
    /// `platform_machine`
    PlatformMachine,
    /// Deprecated `platform.machine` from <https://peps.python.org/pep-0345/#environment-markers>
    /// `platform_python_implementation`
    PlatformPythonImplementation,
    /// `platform_release`
    PlatformRelease,
    /// `platform_system`
    PlatformSystem,
    /// `platform_version`
    PlatformVersion,
    /// `sys_platform`
    SysPlatform,
}

impl From<MarkerValueString> for CanonicalMarkerValueString {
    fn from(value: MarkerValueString) -> Self {
        match value {
            MarkerValueString::ImplementationName => Self::ImplementationName,
            MarkerValueString::OsName => Self::OsName,
            MarkerValueString::OsNameDeprecated => Self::OsName,
            MarkerValueString::PlatformMachine => Self::PlatformMachine,
            MarkerValueString::PlatformMachineDeprecated => Self::PlatformMachine,
            MarkerValueString::PlatformPythonImplementation => Self::PlatformPythonImplementation,
            MarkerValueString::PlatformPythonImplementationDeprecated => {
                Self::PlatformPythonImplementation
            }
            MarkerValueString::PythonImplementationDeprecated => Self::PlatformPythonImplementation,
            MarkerValueString::PlatformRelease => Self::PlatformRelease,
            MarkerValueString::PlatformSystem => Self::PlatformSystem,
            MarkerValueString::PlatformVersion => Self::PlatformVersion,
            MarkerValueString::PlatformVersionDeprecated => Self::PlatformVersion,
            MarkerValueString::SysPlatform => Self::SysPlatform,
            MarkerValueString::SysPlatformDeprecated => Self::SysPlatform,
        }
    }
}

impl From<CanonicalMarkerValueString> for MarkerValueString {
    fn from(value: CanonicalMarkerValueString) -> Self {
        match value {
            CanonicalMarkerValueString::ImplementationName => Self::ImplementationName,
            CanonicalMarkerValueString::OsName => Self::OsName,
            CanonicalMarkerValueString::PlatformMachine => Self::PlatformMachine,
            CanonicalMarkerValueString::PlatformPythonImplementation => {
                Self::PlatformPythonImplementation
            }
            CanonicalMarkerValueString::PlatformRelease => Self::PlatformRelease,
            CanonicalMarkerValueString::PlatformSystem => Self::PlatformSystem,
            CanonicalMarkerValueString::PlatformVersion => Self::PlatformVersion,
            CanonicalMarkerValueString::SysPlatform => Self::SysPlatform,
        }
    }
}

impl Display for CanonicalMarkerValueString {
    /// Normalizes deprecated names to the proper ones
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationName => f.write_str("implementation_name"),
            Self::OsName => f.write_str("os_name"),
            Self::PlatformMachine => f.write_str("platform_machine"),
            Self::PlatformPythonImplementation => f.write_str("platform_python_implementation"),
            Self::PlatformRelease => f.write_str("platform_release"),
            Self::PlatformSystem => f.write_str("platform_system"),
            Self::PlatformVersion => f.write_str("platform_version"),
            Self::SysPlatform => f.write_str("sys_platform"),
        }
    }
}

/// The [`ExtraName`] value used in `extra` markers.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum CanonicalMarkerValueExtra {
    /// A valid [`ExtraName`].
    Extra(ExtraName),
}

impl CanonicalMarkerValueExtra {
    /// Returns the [`ExtraName`] value.
    pub fn extra(&self) -> &ExtraName {
        match self {
            Self::Extra(extra) => extra,
        }
    }
}

impl From<CanonicalMarkerValueExtra> for MarkerValueExtra {
    fn from(value: CanonicalMarkerValueExtra) -> Self {
        match value {
            CanonicalMarkerValueExtra::Extra(extra) => Self::Extra(extra),
        }
    }
}

impl Display for CanonicalMarkerValueExtra {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extra(extra) => extra.fmt(f),
        }
    }
}
