use std::fmt::{Display, Formatter};

use uv_normalize::ExtraName;

use crate::{MarkerValueExtra, MarkerValueString, MarkerValueVersion};

/// Those environment markers with a PEP 440 version as value such as `python_version`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum LoweredMarkerValueVersion {
    /// `implementation_version`
    ImplementationVersion,
    /// `python_full_version`
    PythonFullVersion,
    /// `python_version`
    PythonVersion,
}

impl Display for LoweredMarkerValueVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
            Self::PythonVersion => f.write_str("python_version"),
        }
    }
}

impl From<MarkerValueVersion> for LoweredMarkerValueVersion {
    fn from(value: MarkerValueVersion) -> Self {
        match value {
            MarkerValueVersion::ImplementationVersion => Self::ImplementationVersion,
            MarkerValueVersion::PythonFullVersion => Self::PythonFullVersion,
            MarkerValueVersion::PythonVersion => Self::PythonVersion,
        }
    }
}

impl From<LoweredMarkerValueVersion> for MarkerValueVersion {
    fn from(value: LoweredMarkerValueVersion) -> Self {
        match value {
            LoweredMarkerValueVersion::ImplementationVersion => Self::ImplementationVersion,
            LoweredMarkerValueVersion::PythonFullVersion => Self::PythonFullVersion,
            LoweredMarkerValueVersion::PythonVersion => Self::PythonVersion,
        }
    }
}

/// Those environment markers with an arbitrary string as value such as `sys_platform`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum LoweredMarkerValueString {
    /// `implementation_name`
    ImplementationName,
    /// `os_name`
    OsName,
    /// Deprecated `os.name` from <https://peps.python.org/pep-0345/#environment-markers>
    OsNameDeprecated,
    /// `platform_machine`
    PlatformMachine,
    /// Deprecated `platform.machine` from <https://peps.python.org/pep-0345/#environment-markers>
    PlatformMachineDeprecated,
    /// `platform_python_implementation`
    PlatformPythonImplementation,
    /// Deprecated `platform.python_implementation` from <https://peps.python.org/pep-0345/#environment-markers>
    PlatformPythonImplementationDeprecated,
    /// Deprecated `python_implementation` from <https://github.com/pypa/packaging/issues/72>
    PythonImplementationDeprecated,
    /// `platform_release`
    PlatformRelease,
    /// `platform_system`
    PlatformSystem,
    /// `platform_version`
    PlatformVersion,
    /// Deprecated `platform.version` from <https://peps.python.org/pep-0345/#environment-markers>
    PlatformVersionDeprecated,
    /// `sys_platform`
    SysPlatform,
    /// Deprecated `sys.platform` from <https://peps.python.org/pep-0345/#environment-markers>
    SysPlatformDeprecated,
}

impl From<MarkerValueString> for LoweredMarkerValueString {
    fn from(value: MarkerValueString) -> Self {
        match value {
            MarkerValueString::ImplementationName => Self::ImplementationName,
            MarkerValueString::OsName => Self::OsName,
            MarkerValueString::OsNameDeprecated => Self::OsNameDeprecated,
            MarkerValueString::PlatformMachine => Self::PlatformMachine,
            MarkerValueString::PlatformMachineDeprecated => Self::PlatformMachineDeprecated,
            MarkerValueString::PlatformPythonImplementation => Self::PlatformPythonImplementation,
            MarkerValueString::PlatformPythonImplementationDeprecated => {
                Self::PlatformPythonImplementationDeprecated
            }
            MarkerValueString::PythonImplementationDeprecated => {
                Self::PythonImplementationDeprecated
            }
            MarkerValueString::PlatformRelease => Self::PlatformRelease,
            MarkerValueString::PlatformSystem => Self::PlatformSystem,
            MarkerValueString::PlatformVersion => Self::PlatformVersion,
            MarkerValueString::PlatformVersionDeprecated => Self::PlatformVersionDeprecated,
            MarkerValueString::SysPlatform => Self::SysPlatform,
            MarkerValueString::SysPlatformDeprecated => Self::SysPlatformDeprecated,
        }
    }
}

impl From<LoweredMarkerValueString> for MarkerValueString {
    fn from(value: LoweredMarkerValueString) -> Self {
        match value {
            LoweredMarkerValueString::ImplementationName => Self::ImplementationName,
            LoweredMarkerValueString::OsName => Self::OsName,
            LoweredMarkerValueString::OsNameDeprecated => Self::OsNameDeprecated,
            LoweredMarkerValueString::PlatformMachine => Self::PlatformMachine,
            LoweredMarkerValueString::PlatformMachineDeprecated => Self::PlatformMachineDeprecated,
            LoweredMarkerValueString::PlatformPythonImplementation => {
                Self::PlatformPythonImplementation
            }
            LoweredMarkerValueString::PlatformPythonImplementationDeprecated => {
                Self::PlatformPythonImplementationDeprecated
            }
            LoweredMarkerValueString::PythonImplementationDeprecated => {
                Self::PythonImplementationDeprecated
            }
            LoweredMarkerValueString::PlatformRelease => Self::PlatformRelease,
            LoweredMarkerValueString::PlatformSystem => Self::PlatformSystem,
            LoweredMarkerValueString::PlatformVersion => Self::PlatformVersion,
            LoweredMarkerValueString::PlatformVersionDeprecated => Self::PlatformVersionDeprecated,
            LoweredMarkerValueString::SysPlatform => Self::SysPlatform,
            LoweredMarkerValueString::SysPlatformDeprecated => Self::SysPlatformDeprecated,
        }
    }
}

impl Display for LoweredMarkerValueString {
    /// Normalizes deprecated names to the proper ones
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationName => f.write_str("implementation_name"),
            Self::OsName | Self::OsNameDeprecated => f.write_str("os_name"),
            Self::PlatformMachine | Self::PlatformMachineDeprecated => {
                f.write_str("platform_machine")
            }
            Self::PlatformPythonImplementation
            | Self::PlatformPythonImplementationDeprecated
            | Self::PythonImplementationDeprecated => f.write_str("platform_python_implementation"),
            Self::PlatformRelease => f.write_str("platform_release"),
            Self::PlatformSystem => f.write_str("platform_system"),
            Self::PlatformVersion | Self::PlatformVersionDeprecated => {
                f.write_str("platform_version")
            }
            Self::SysPlatform | Self::SysPlatformDeprecated => f.write_str("sys_platform"),
        }
    }
}

/// The [`ExtraName`] value used in `extra` markers.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum LoweredMarkerValueExtra {
    /// A valid [`ExtraName`].
    Extra(ExtraName),
}

impl LoweredMarkerValueExtra {
    /// Returns the [`ExtraName`] value.
    pub fn extra(&self) -> &ExtraName {
        match self {
            Self::Extra(extra) => extra,
        }
    }
}

impl From<LoweredMarkerValueExtra> for MarkerValueExtra {
    fn from(value: LoweredMarkerValueExtra) -> Self {
        match value {
            LoweredMarkerValueExtra::Extra(extra) => Self::Extra(extra),
        }
    }
}

impl Display for LoweredMarkerValueExtra {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extra(extra) => extra.fmt(f),
        }
    }
}
