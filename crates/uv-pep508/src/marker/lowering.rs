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
}

impl Display for LoweredMarkerValueVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
        }
    }
}

impl From<LoweredMarkerValueVersion> for MarkerValueVersion {
    fn from(value: LoweredMarkerValueVersion) -> Self {
        match value {
            LoweredMarkerValueVersion::ImplementationVersion => Self::ImplementationVersion,
            LoweredMarkerValueVersion::PythonFullVersion => Self::PythonFullVersion,
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

impl From<MarkerValueString> for LoweredMarkerValueString {
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

impl From<LoweredMarkerValueString> for MarkerValueString {
    fn from(value: LoweredMarkerValueString) -> Self {
        match value {
            LoweredMarkerValueString::ImplementationName => Self::ImplementationName,
            LoweredMarkerValueString::OsName => Self::OsName,
            LoweredMarkerValueString::PlatformMachine => Self::PlatformMachine,
            LoweredMarkerValueString::PlatformPythonImplementation => {
                Self::PlatformPythonImplementation
            }
            LoweredMarkerValueString::PlatformRelease => Self::PlatformRelease,
            LoweredMarkerValueString::PlatformSystem => Self::PlatformSystem,
            LoweredMarkerValueString::PlatformVersion => Self::PlatformVersion,
            LoweredMarkerValueString::SysPlatform => Self::SysPlatform,
        }
    }
}

impl Display for LoweredMarkerValueString {
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

/// The [`MarkerValue`] value used in `platform` markers. A combination of `sys_platform` and
/// `platform_system`.
///
/// The core idea is that we normalize `platform_system == 'Linux'` to `sys_platform == 'linux'`,
/// `platform_system == 'Windows'` to `sys_platform == 'win32'`, and `platform_system == 'Darwin'`
/// to `sys_platform == 'darwin'`. Apart from that, values are passed through as-is.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum Platform {
    Darwin,
    Linux,
    Windows,
    SysPlatform(String),
    PlatformSystem(String),
}

impl Platform {
    /// Construct a [`Platform`] from the `sys_platform` value.
    pub fn from_sys_platform(value: String) -> Self {
        match value.as_str() {
            "linux" => Self::Linux,
            "win32" => Self::Windows,
            "darwin" => Self::Darwin,
            _ => Self::SysPlatform(value),
        }
    }

    /// Construct a [`Platform`] from the `platform_system` value.
    pub fn from_platform_system(value: String) -> Self {
        match value.as_str() {
            "Linux" => Self::Linux,
            "Windows" => Self::Windows,
            "Darwin" => Self::Darwin,
            _ => Self::PlatformSystem(value),
        }
    }
}

impl Display for Platform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Linux => f.write_str("sys_platform == 'linux'"),
            Platform::Windows => f.write_str("sys_platform == 'win32'"),
            Platform::Darwin => f.write_str("sys_platform == 'darwin'"),
            Platform::SysPlatform(platform) => write!(f, "sys_platform == '{platform}'"),
            Platform::PlatformSystem(system) => write!(f, "platform_system == '{system}'"),
        }
    }
}

impl From<Platform> for (MarkerValueString, String) {
    fn from(value: Platform) -> Self {
        match value {
            Platform::Linux => (MarkerValueString::SysPlatform, "linux".to_string()),
            Platform::Windows => (MarkerValueString::SysPlatform, "win32".to_string()),
            Platform::Darwin => (MarkerValueString::SysPlatform, "darwin".to_string()),
            Platform::SysPlatform(value) => (MarkerValueString::SysPlatform, value),
            Platform::PlatformSystem(value) => (MarkerValueString::PlatformSystem, value),
        }
    }
}
