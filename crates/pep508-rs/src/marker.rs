//! PEP 508 markers implementations with validation and warnings
//!
//! Markers allow you to install dependencies only in specific environments (python version,
//! operating system, architecture, etc.) or when a specific feature is activated. E.g. you can
//! say `importlib-metadata ; python_version < "3.8"` or
//! `itsdangerous (>=1.1.0) ; extra == 'security'`. Unfortunately, the marker grammar has some
//! oversights (e.g. <https://github.com/pypa/packaging.python.org/pull/1181>) and
//! the design of comparisons (PEP 440 comparisons with lexicographic fallback) leads to confusing
//! outcomes. This implementation tries to carefully validate everything and emit warnings whenever
//! bogus comparisons with unintended semantics are made.

use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

#[cfg(feature = "pyo3")]
use pyo3::{
    basic::CompareOp, exceptions::PyValueError, pyclass, pymethods, types::PyAnyMethods, PyResult,
    Python,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use pep440_rs::{Version, VersionParseError, VersionPattern, VersionSpecifier};
use uv_normalize::ExtraName;

use crate::cursor::Cursor;
use crate::{Pep508Error, Pep508ErrorSource, Pep508Url, Reporter, TracingReporter};

/// Ways in which marker evaluation can fail
#[derive(Debug, Eq, Hash, Ord, PartialOrd, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "pyo3", pyclass(module = "pep508"))]
pub enum MarkerWarningKind {
    /// Using an old name from PEP 345 instead of the modern equivalent
    /// <https://peps.python.org/pep-0345/#environment-markers>
    DeprecatedMarkerName,
    /// Doing an operation other than `==` and `!=` on a quoted string with `extra`, such as
    /// `extra > "perf"` or `extra == os_name`
    ExtraInvalidComparison,
    /// Comparing a string valued marker and a string lexicographically, such as `"3.9" > "3.10"`
    LexicographicComparison,
    /// Comparing two markers, such as `os_name != sys_implementation`
    MarkerMarkerComparison,
    /// Failed to parse a PEP 440 version or version specifier, e.g. `>=1<2`
    Pep440Error,
    /// Comparing two strings, such as `"3.9" > "3.10"`
    StringStringComparison,
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl MarkerWarningKind {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __hash__(&self) -> u8 {
        *self as u8
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __richcmp__(&self, other: Self, op: CompareOp) -> bool {
        op.matches(self.cmp(&other))
    }
}

/// Those environment markers with a PEP 440 version as value such as `python_version`
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum MarkerValueVersion {
    /// `implementation_version`
    ImplementationVersion,
    /// `python_full_version`
    PythonFullVersion,
    /// `python_version`
    PythonVersion,
}

impl Display for MarkerValueVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
            Self::PythonVersion => f.write_str("python_version"),
        }
    }
}

/// Those environment markers with an arbitrary string as value such as `sys_platform`
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerValueString {
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

impl Display for MarkerValueString {
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

/// One of the predefined environment values
///
/// <https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers>
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerValue {
    /// Those environment markers with a PEP 440 version as value such as `python_version`
    MarkerEnvVersion(MarkerValueVersion),
    /// Those environment markers with an arbitrary string as value such as `sys_platform`
    MarkerEnvString(MarkerValueString),
    /// `extra`. This one is special because it's a list and not env but user given
    Extra,
    /// Not a constant, but a user given quoted string with a value inside such as '3.8' or "windows"
    QuotedString(String),
}

impl MarkerValue {
    fn string_value(value: String) -> Self {
        Self::QuotedString(value)
    }
}

impl FromStr for MarkerValue {
    type Err = String;

    /// This is specifically for the reserved values
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = match s {
            "implementation_name" => Self::MarkerEnvString(MarkerValueString::ImplementationName),
            "implementation_version" => {
                Self::MarkerEnvVersion(MarkerValueVersion::ImplementationVersion)
            }
            "os_name" => Self::MarkerEnvString(MarkerValueString::OsName),
            "os.name" => Self::MarkerEnvString(MarkerValueString::OsNameDeprecated),
            "platform_machine" => Self::MarkerEnvString(MarkerValueString::PlatformMachine),
            "platform.machine" => {
                Self::MarkerEnvString(MarkerValueString::PlatformMachineDeprecated)
            }
            "platform_python_implementation" => {
                Self::MarkerEnvString(MarkerValueString::PlatformPythonImplementation)
            }
            "platform.python_implementation" => {
                Self::MarkerEnvString(MarkerValueString::PlatformPythonImplementationDeprecated)
            }
            "python_implementation" => {
                Self::MarkerEnvString(MarkerValueString::PythonImplementationDeprecated)
            }
            "platform_release" => Self::MarkerEnvString(MarkerValueString::PlatformRelease),
            "platform_system" => Self::MarkerEnvString(MarkerValueString::PlatformSystem),
            "platform_version" => Self::MarkerEnvString(MarkerValueString::PlatformVersion),
            "platform.version" => {
                Self::MarkerEnvString(MarkerValueString::PlatformVersionDeprecated)
            }
            "python_full_version" => Self::MarkerEnvVersion(MarkerValueVersion::PythonFullVersion),
            "python_version" => Self::MarkerEnvVersion(MarkerValueVersion::PythonVersion),
            "sys_platform" => Self::MarkerEnvString(MarkerValueString::SysPlatform),
            "sys.platform" => Self::MarkerEnvString(MarkerValueString::SysPlatformDeprecated),
            "extra" => Self::Extra,
            _ => return Err(format!("Invalid key: {s}")),
        };
        Ok(value)
    }
}

impl Display for MarkerValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MarkerEnvVersion(marker_value_version) => marker_value_version.fmt(f),
            Self::MarkerEnvString(marker_value_string) => marker_value_string.fmt(f),
            Self::Extra => f.write_str("extra"),
            Self::QuotedString(value) => write!(f, "'{value}'"),
        }
    }
}

/// How to compare key and value, such as by `==`, `>` or `not in`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerOperator {
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `>`
    GreaterThan,
    /// `>=`
    GreaterEqual,
    /// `<`
    LessThan,
    /// `<=`
    LessEqual,
    /// `~=`
    TildeEqual,
    /// `in`
    In,
    /// `not in`
    NotIn,
}

impl MarkerOperator {
    /// Compare two versions, returning None for `in` and `not in`
    fn to_pep440_operator(self) -> Option<pep440_rs::Operator> {
        match self {
            Self::Equal => Some(pep440_rs::Operator::Equal),
            Self::NotEqual => Some(pep440_rs::Operator::NotEqual),
            Self::GreaterThan => Some(pep440_rs::Operator::GreaterThan),
            Self::GreaterEqual => Some(pep440_rs::Operator::GreaterThanEqual),
            Self::LessThan => Some(pep440_rs::Operator::LessThan),
            Self::LessEqual => Some(pep440_rs::Operator::LessThanEqual),
            Self::TildeEqual => Some(pep440_rs::Operator::TildeEqual),
            Self::In => None,
            Self::NotIn => None,
        }
    }
}

impl FromStr for MarkerOperator {
    type Err = String;

    /// PEP 508 allows arbitrary whitespace between "not" and "in", and so do we
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = match s {
            "==" => Self::Equal,
            "!=" => Self::NotEqual,
            ">" => Self::GreaterThan,
            ">=" => Self::GreaterEqual,
            "<" => Self::LessThan,
            "<=" => Self::LessEqual,
            "~=" => Self::TildeEqual,
            "in" => Self::In,
            not_space_in
                if not_space_in
                    // start with not
                    .strip_prefix("not")
                    // ends with in
                    .and_then(|space_in| space_in.strip_suffix("in"))
                    // and has only whitespace in between
                    .is_some_and(|space| !space.is_empty() && space.trim().is_empty()) =>
            {
                Self::NotIn
            }
            other => return Err(format!("Invalid comparator: {other}")),
        };
        Ok(value)
    }
}

impl Display for MarkerOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::GreaterThan => ">",
            Self::GreaterEqual => ">=",
            Self::LessThan => "<",
            Self::LessEqual => "<=",
            Self::TildeEqual => "~=",
            Self::In => "in",
            Self::NotIn => "not in",
        })
    }
}

/// Helper type with a [Version] and its original text
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(get_all, module = "pep508"))]
pub struct StringVersion {
    /// Original unchanged string
    pub string: String,
    /// Parsed version
    pub version: Version,
}

impl FromStr for StringVersion {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            string: s.to_string(),
            version: Version::from_str(s)?,
        })
    }
}

impl Display for StringVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}

impl Serialize for StringVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.string)
    }
}

impl<'de> Deserialize<'de> for StringVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        Self::from_str(&string).map_err(de::Error::custom)
    }
}

impl Deref for StringVersion {
    type Target = Version;

    fn deref(&self) -> &Self::Target {
        &self.version
    }
}

/// The marker values for a python interpreter, normally the current one
///
/// <https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers>
///
/// Some are `(String, Version)` because we have to support version comparison
#[allow(missing_docs, clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "pyo3", pyclass(module = "pep508"))]
pub struct MarkerEnvironment {
    #[serde(flatten)]
    inner: Arc<MarkerEnvironmentInner>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
struct MarkerEnvironmentInner {
    implementation_name: String,
    implementation_version: StringVersion,
    os_name: String,
    platform_machine: String,
    platform_python_implementation: String,
    platform_release: String,
    platform_system: String,
    platform_version: String,
    python_full_version: StringVersion,
    python_version: StringVersion,
    sys_platform: String,
}

impl MarkerEnvironment {
    /// Returns of the PEP 440 version typed value of the key in the current environment
    pub fn get_version(&self, key: &MarkerValueVersion) -> &Version {
        match key {
            MarkerValueVersion::ImplementationVersion => &self.implementation_version().version,
            MarkerValueVersion::PythonFullVersion => &self.python_full_version().version,
            MarkerValueVersion::PythonVersion => &self.python_version().version,
        }
    }

    /// Returns of the stringly typed value of the key in the current environment
    pub fn get_string(&self, key: &MarkerValueString) -> &str {
        match key {
            MarkerValueString::ImplementationName => self.implementation_name(),
            MarkerValueString::OsName | MarkerValueString::OsNameDeprecated => self.os_name(),
            MarkerValueString::PlatformMachine | MarkerValueString::PlatformMachineDeprecated => {
                self.platform_machine()
            }
            MarkerValueString::PlatformPythonImplementation
            | MarkerValueString::PlatformPythonImplementationDeprecated
            | MarkerValueString::PythonImplementationDeprecated => {
                self.platform_python_implementation()
            }
            MarkerValueString::PlatformRelease => self.platform_release(),
            MarkerValueString::PlatformSystem => self.platform_system(),
            MarkerValueString::PlatformVersion | MarkerValueString::PlatformVersionDeprecated => {
                self.platform_version()
            }
            MarkerValueString::SysPlatform | MarkerValueString::SysPlatformDeprecated => {
                self.sys_platform()
            }
        }
    }
}

/// APIs for retrieving specific parts of a marker environment.
impl MarkerEnvironment {
    /// Returns the name of the Python implementation for this environment.
    ///
    /// This is equivalent to `sys.implementation.name`.
    ///
    /// Some example values are: `cpython`.
    #[inline]
    pub fn implementation_name(&self) -> &str {
        &self.inner.implementation_name
    }

    /// Returns the Python implementation version for this environment.
    ///
    /// This value is derived from `sys.implementation.version`. See [PEP 508
    /// environment markers] for full details.
    ///
    /// This is equivalent to `sys.implementation.name`.
    ///
    /// Some example values are: `3.4.0`, `3.5.0b1`.
    ///
    /// [PEP 508 environment markers]: https://peps.python.org/pep-0508/#environment-markers
    #[inline]
    pub fn implementation_version(&self) -> &StringVersion {
        &self.inner.implementation_version
    }

    /// Returns the name of the operating system for this environment.
    ///
    /// This is equivalent to `os.name`.
    ///
    /// Some example values are: `posix`, `java`.
    #[inline]
    pub fn os_name(&self) -> &str {
        &self.inner.os_name
    }

    /// Returns the name of the machine for this environment's platform.
    ///
    /// This is equivalent to `platform.machine()`.
    ///
    /// Some example values are: `x86_64`.
    #[inline]
    pub fn platform_machine(&self) -> &str {
        &self.inner.platform_machine
    }

    /// Returns the name of the Python implementation for this environment's
    /// platform.
    ///
    /// This is equivalent to `platform.python_implementation()`.
    ///
    /// Some example values are: `CPython`, `Jython`.
    #[inline]
    pub fn platform_python_implementation(&self) -> &str {
        &self.inner.platform_python_implementation
    }

    /// Returns the release for this environment's platform.
    ///
    /// This is equivalent to `platform.release()`.
    ///
    /// Some example values are: `3.14.1-x86_64-linode39`, `14.5.0`, `1.8.0_51`.
    #[inline]
    pub fn platform_release(&self) -> &str {
        &self.inner.platform_release
    }

    /// Returns the system for this environment's platform.
    ///
    /// This is equivalent to `platform.system()`.
    ///
    /// Some example values are: `Linux`, `Windows`, `Java`.
    #[inline]
    pub fn platform_system(&self) -> &str {
        &self.inner.platform_system
    }

    /// Returns the version for this environment's platform.
    ///
    /// This is equivalent to `platform.version()`.
    ///
    /// Some example values are: `#1 SMP Fri Apr 25 13:07:35 EDT 2014`,
    /// `Java HotSpot(TM) 64-Bit Server VM, 25.51-b03, Oracle Corporation`,
    /// `Darwin Kernel Version 14.5.0: Wed Jul 29 02:18:53 PDT 2015;
    /// root:xnu-2782.40.9~2/RELEASE_X86_64`.
    #[inline]
    pub fn platform_version(&self) -> &str {
        &self.inner.platform_version
    }

    /// Returns the full version of Python for this environment.
    ///
    /// This is equivalent to `platform.python_version()`.
    ///
    /// Some example values are: `3.4.0`, `3.5.0b1`.
    #[inline]
    pub fn python_full_version(&self) -> &StringVersion {
        &self.inner.python_full_version
    }

    /// Returns the version of Python for this environment.
    ///
    /// This is equivalent to `'.'.join(platform.python_version_tuple()[:2])`.
    ///
    /// Some example values are: `3.4`, `2.7`.
    #[inline]
    pub fn python_version(&self) -> &StringVersion {
        &self.inner.python_version
    }

    /// Returns the name of the system platform for this environment.
    ///
    /// This is equivalent to `sys.platform`.
    ///
    /// Some example values are: `linux`, `linux2`, `darwin`, `java1.8.0_51`
    /// (note that `linux` is from Python3 and `linux2` from Python2).
    #[inline]
    pub fn sys_platform(&self) -> &str {
        &self.inner.sys_platform
    }
}

/// APIs for setting specific parts of a marker environment.
impl MarkerEnvironment {
    /// Set the name of the Python implementation for this environment.
    ///
    /// See also [`MarkerEnvironment::implementation_name`].
    #[inline]
    #[must_use]
    pub fn with_implementation_name(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).implementation_name = value.into();
        self
    }

    /// Set the Python implementation version for this environment.
    ///
    /// See also [`MarkerEnvironment::implementation_version`].
    #[inline]
    #[must_use]
    pub fn with_implementation_version(
        mut self,
        value: impl Into<StringVersion>,
    ) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).implementation_version = value.into();
        self
    }

    /// Set the name of the operating system for this environment.
    ///
    /// See also [`MarkerEnvironment::os_name`].
    #[inline]
    #[must_use]
    pub fn with_os_name(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).os_name = value.into();
        self
    }

    /// Set the name of the machine for this environment's platform.
    ///
    /// See also [`MarkerEnvironment::platform_machine`].
    #[inline]
    #[must_use]
    pub fn with_platform_machine(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).platform_machine = value.into();
        self
    }

    /// Set the name of the Python implementation for this environment's
    /// platform.
    ///
    /// See also [`MarkerEnvironment::platform_python_implementation`].
    #[inline]
    #[must_use]
    pub fn with_platform_python_implementation(
        mut self,
        value: impl Into<String>,
    ) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).platform_python_implementation = value.into();
        self
    }

    /// Set the release for this environment's platform.
    ///
    /// See also [`MarkerEnvironment::platform_release`].
    #[inline]
    #[must_use]
    pub fn with_platform_release(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).platform_release = value.into();
        self
    }

    /// Set the system for this environment's platform.
    ///
    /// See also [`MarkerEnvironment::platform_system`].
    #[inline]
    #[must_use]
    pub fn with_platform_system(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).platform_system = value.into();
        self
    }

    /// Set the version for this environment's platform.
    ///
    /// See also [`MarkerEnvironment::platform_version`].
    #[inline]
    #[must_use]
    pub fn with_platform_version(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).platform_version = value.into();
        self
    }

    /// Set the full version of Python for this environment.
    ///
    /// See also [`MarkerEnvironment::python_full_version`].
    #[inline]
    #[must_use]
    pub fn with_python_full_version(
        mut self,
        value: impl Into<StringVersion>,
    ) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).python_full_version = value.into();
        self
    }

    /// Set the version of Python for this environment.
    ///
    /// See also [`MarkerEnvironment::python_full_version`].
    #[inline]
    #[must_use]
    pub fn with_python_version(mut self, value: impl Into<StringVersion>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).python_version = value.into();
        self
    }

    /// Set the name of the system platform for this environment.
    ///
    /// See also [`MarkerEnvironment::sys_platform`].
    #[inline]
    #[must_use]
    pub fn with_sys_platform(mut self, value: impl Into<String>) -> MarkerEnvironment {
        Arc::make_mut(&mut self.inner).sys_platform = value.into();
        self
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl MarkerEnvironment {
    /// Construct your own marker environment
    #[new]
    #[pyo3(signature = (*,
        implementation_name,
        implementation_version,
        os_name,
        platform_machine,
        platform_python_implementation,
        platform_release,
        platform_system,
        platform_version,
        python_full_version,
        python_version,
        sys_platform
    ))]
    #[allow(clippy::too_many_arguments)]
    fn py_new(
        implementation_name: &str,
        implementation_version: &str,
        os_name: &str,
        platform_machine: &str,
        platform_python_implementation: &str,
        platform_release: &str,
        platform_system: &str,
        platform_version: &str,
        python_full_version: &str,
        python_version: &str,
        sys_platform: &str,
    ) -> PyResult<Self> {
        let implementation_version =
            StringVersion::from_str(implementation_version).map_err(|err| {
                PyValueError::new_err(format!(
                    "implementation_version is not a valid PEP440 version: {err}"
                ))
            })?;
        let python_full_version = StringVersion::from_str(python_full_version).map_err(|err| {
            PyValueError::new_err(format!(
                "python_full_version is not a valid PEP440 version: {err}"
            ))
        })?;
        let python_version = StringVersion::from_str(python_version).map_err(|err| {
            PyValueError::new_err(format!(
                "python_version is not a valid PEP440 version: {err}"
            ))
        })?;
        Ok(Self {
            inner: Arc::new(MarkerEnvironmentInner {
                implementation_name: implementation_name.to_string(),
                implementation_version,
                os_name: os_name.to_string(),
                platform_machine: platform_machine.to_string(),
                platform_python_implementation: platform_python_implementation.to_string(),
                platform_release: platform_release.to_string(),
                platform_system: platform_system.to_string(),
                platform_version: platform_version.to_string(),
                python_full_version,
                python_version,
                sys_platform: sys_platform.to_string(),
            }),
        })
    }

    /// Query the current python interpreter to get the correct marker value
    #[staticmethod]
    fn current(py: Python<'_>) -> PyResult<Self> {
        let os = py.import_bound("os")?;
        let platform = py.import_bound("platform")?;
        let sys = py.import_bound("sys")?;
        let python_version_tuple: (String, String, String) = platform
            .getattr("python_version_tuple")?
            .call0()?
            .extract()?;

        // See pseudocode at
        // https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers
        let name = sys.getattr("implementation")?.getattr("name")?.extract()?;
        let info = sys.getattr("implementation")?.getattr("version")?;
        let kind = info.getattr("releaselevel")?.extract::<String>()?;
        let implementation_version: String = format!(
            "{}.{}.{}{}",
            info.getattr("major")?.extract::<usize>()?,
            info.getattr("minor")?.extract::<usize>()?,
            info.getattr("micro")?.extract::<usize>()?,
            if kind == "final" {
                String::new()
            } else {
                format!("{}{}", kind, info.getattr("serial")?.extract::<usize>()?)
            }
        );
        let python_full_version: String = platform.getattr("python_version")?.call0()?.extract()?;
        let python_version = format!("{}.{}", python_version_tuple.0, python_version_tuple.1);

        // This is not written down in PEP 508, but it's the only reasonable assumption to make
        let implementation_version =
            StringVersion::from_str(&implementation_version).map_err(|err| {
                PyValueError::new_err(format!(
                    "Broken python implementation, implementation_version is not a valid PEP440 version: {err}"
                ))
            })?;
        let python_full_version = StringVersion::from_str(&python_full_version).map_err(|err| {
            PyValueError::new_err(format!(
                "Broken python implementation, python_full_version is not a valid PEP440 version: {err}"
            ))
        })?;
        let python_version = StringVersion::from_str(&python_version).map_err(|err| {
            PyValueError::new_err(format!(
                "Broken python implementation, python_version is not a valid PEP440 version: {err}"
            ))
        })?;
        Ok(Self {
            inner: Arc::new(MarkerEnvironmentInner {
                implementation_name: name,
                implementation_version,
                os_name: os.getattr("name")?.extract()?,
                platform_machine: platform.getattr("machine")?.call0()?.extract()?,
                platform_python_implementation: platform
                    .getattr("python_implementation")?
                    .call0()?
                    .extract()?,
                platform_release: platform.getattr("release")?.call0()?.extract()?,
                platform_system: platform.getattr("system")?.call0()?.extract()?,
                platform_version: platform.getattr("version")?.call0()?.extract()?,
                python_full_version,
                python_version,
                sys_platform: sys.getattr("platform")?.extract()?,
            }),
        })
    }

    /// Returns the name of the Python implementation for this environment.
    #[getter]
    pub fn py_implementation_name(&self) -> String {
        self.implementation_name().to_string()
    }

    /// Returns the Python implementation version for this environment.
    #[getter]
    pub fn py_implementation_version(&self) -> StringVersion {
        self.implementation_version().clone()
    }

    /// Returns the name of the operating system for this environment.
    #[getter]
    pub fn py_os_name(&self) -> String {
        self.os_name().to_string()
    }

    /// Returns the name of the machine for this environment's platform.
    #[getter]
    pub fn py_platform_machine(&self) -> String {
        self.platform_machine().to_string()
    }

    /// Returns the name of the Python implementation for this environment's
    /// platform.
    #[getter]
    pub fn py_platform_python_implementation(&self) -> String {
        self.platform_python_implementation().to_string()
    }

    /// Returns the release for this environment's platform.
    #[getter]
    pub fn py_platform_release(&self) -> String {
        self.platform_release().to_string()
    }

    /// Returns the system for this environment's platform.
    #[getter]
    pub fn py_platform_system(&self) -> String {
        self.platform_system().to_string()
    }

    /// Returns the version for this environment's platform.
    #[getter]
    pub fn py_platform_version(&self) -> String {
        self.platform_version().to_string()
    }

    /// Returns the full version of Python for this environment.
    #[getter]
    pub fn py_python_full_version(&self) -> StringVersion {
        self.python_full_version().clone()
    }

    /// Returns the version of Python for this environment.
    #[getter]
    pub fn py_python_version(&self) -> StringVersion {
        self.python_version().clone()
    }

    /// Returns the name of the system platform for this environment.
    #[getter]
    pub fn py_sys_platform(&self) -> String {
        self.sys_platform().to_string()
    }
}

/// A builder for constructing a marker environment.
///
/// A value of this type can be fallibly converted to a full
/// [`MarkerEnvironment`] via [`MarkerEnvironment::try_from`]. This can fail when
/// the version strings given aren't valid.
///
/// The main utility of this type is for constructing dummy or test environment
/// values.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MarkerEnvironmentBuilder<'a> {
    pub implementation_name: &'a str,
    pub implementation_version: &'a str,
    pub os_name: &'a str,
    pub platform_machine: &'a str,
    pub platform_python_implementation: &'a str,
    pub platform_release: &'a str,
    pub platform_system: &'a str,
    pub platform_version: &'a str,
    pub python_full_version: &'a str,
    pub python_version: &'a str,
    pub sys_platform: &'a str,
}

impl<'a> TryFrom<MarkerEnvironmentBuilder<'a>> for MarkerEnvironment {
    type Error = VersionParseError;

    fn try_from(builder: MarkerEnvironmentBuilder<'a>) -> Result<Self, Self::Error> {
        Ok(MarkerEnvironment {
            inner: Arc::new(MarkerEnvironmentInner {
                implementation_name: builder.implementation_name.to_string(),
                implementation_version: builder.implementation_version.parse()?,
                os_name: builder.os_name.to_string(),
                platform_machine: builder.platform_machine.to_string(),
                platform_python_implementation: builder.platform_python_implementation.to_string(),
                platform_release: builder.platform_release.to_string(),
                platform_system: builder.platform_system.to_string(),
                platform_version: builder.platform_version.to_string(),
                python_full_version: builder.python_full_version.parse()?,
                python_version: builder.python_version.parse()?,
                sys_platform: builder.sys_platform.to_string(),
            }),
        })
    }
}

/// Represents one clause such as `python_version > "3.8"`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum MarkerExpression {
    /// A version expression, e.g. `<version key> <version op> <quoted PEP 440 version>`.
    Version {
        key: MarkerValueVersion,
        specifier: VersionSpecifier,
    },
    /// An inverted version expression, e.g `<quoted PEP 440 version> <version op> <version key>`.
    VersionInverted {
        /// No star allowed here, `'3.*' == python_version` is not a valid PEP 440 comparison.
        version: Version,
        operator: pep440_rs::Operator,
        key: MarkerValueVersion,
    },
    /// An string marker comparison, e.g. `sys_platform == '...'`.
    String {
        key: MarkerValueString,
        operator: MarkerOperator,
        value: String,
    },
    /// An inverted string marker comparison, e.g. `'...' == sys_platform`.
    StringInverted {
        value: String,
        operator: MarkerOperator,
        key: MarkerValueString,
    },
    /// `extra <extra op> '...'` or `'...' <extra op> extra`
    Extra {
        operator: ExtraOperator,
        name: ExtraName,
    },
    /// An invalid or meaningless expression, such as '...' == '...'.
    ///
    /// Invalid expressions always evaluate to `false`, and are warned for during parsing.
    Arbitrary {
        l_value: MarkerValue,
        operator: MarkerOperator,
        r_value: MarkerValue,
    },
}

/// The operator for an extra expression, either '==' or '!='.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ExtraOperator {
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
}

impl ExtraOperator {
    fn from_marker_operator(operator: MarkerOperator) -> Option<ExtraOperator> {
        match operator {
            MarkerOperator::Equal => Some(ExtraOperator::Equal),
            MarkerOperator::NotEqual => Some(ExtraOperator::NotEqual),
            _ => None,
        }
    }
}

impl Display for ExtraOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
        })
    }
}

impl MarkerExpression {
    /// Parse a [`MarkerExpression`] from a string with the given reporter.
    pub fn parse_reporter(s: &str, reporter: &mut impl Reporter) -> Result<Self, Pep508Error> {
        let mut chars = Cursor::new(s);
        let expression = parse_marker_key_op_value(&mut chars, reporter)?;
        chars.eat_whitespace();
        if let Some((pos, unexpected)) = chars.next() {
            return Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Unexpected character '{unexpected}', expected end of input"
                )),
                start: pos,
                len: chars.remaining(),
                input: chars.to_string(),
            });
        }
        Ok(expression)
    }

    /// Convert a <`marker_value`> <`marker_op`> <`marker_value`> expression into it's
    /// typed equivalent.
    fn new(
        l_value: MarkerValue,
        operator: MarkerOperator,
        r_value: MarkerValue,
        reporter: &mut impl Reporter,
    ) -> MarkerExpression {
        match l_value {
            // The only sound choice for this is `<version key> <version op> <quoted PEP 440 version>`
            MarkerValue::MarkerEnvVersion(key) => {
                let MarkerValue::QuotedString(value) = r_value else {
                    reporter.report(
                        MarkerWarningKind::Pep440Error,
                        format!(
                            "Expected double quoted PEP 440 version to compare with {key}, found {r_value},
                            will evaluate to false"
                        ),
                    );

                    return MarkerExpression::arbitrary(
                        MarkerValue::MarkerEnvVersion(key),
                        operator,
                        r_value,
                    );
                };

                match MarkerExpression::version(key.clone(), operator, &value, reporter) {
                    Some(expr) => expr,
                    None => MarkerExpression::arbitrary(
                        MarkerValue::MarkerEnvVersion(key),
                        operator,
                        MarkerValue::QuotedString(value),
                    ),
                }
            }
            // The only sound choice for this is `<env key> <op> <string>`
            MarkerValue::MarkerEnvString(key) => {
                let value = match r_value {
                    MarkerValue::Extra
                    | MarkerValue::MarkerEnvVersion(_)
                    | MarkerValue::MarkerEnvString(_) => {
                        reporter.report(
                            MarkerWarningKind::MarkerMarkerComparison,
                            "Comparing two markers with each other doesn't make any sense,
                            will evaluate to false"
                                .to_string(),
                        );

                        return MarkerExpression::arbitrary(
                            MarkerValue::MarkerEnvString(key),
                            operator,
                            r_value,
                        );
                    }
                    MarkerValue::QuotedString(r_string) => r_string,
                };

                MarkerExpression::String {
                    key,
                    operator,
                    value,
                }
            }
            // `extra == '...'`
            MarkerValue::Extra => {
                let value = match r_value {
                    MarkerValue::MarkerEnvVersion(_)
                    | MarkerValue::MarkerEnvString(_)
                    | MarkerValue::Extra => {
                        reporter.report(
                            MarkerWarningKind::ExtraInvalidComparison,
                            "Comparing extra with something other than a quoted string is wrong,
                            will evaluate to false"
                                .to_string(),
                        );
                        return MarkerExpression::arbitrary(l_value, operator, r_value);
                    }
                    MarkerValue::QuotedString(value) => value,
                };

                match MarkerExpression::extra(operator, &value, reporter) {
                    Some(expr) => expr,
                    None => MarkerExpression::arbitrary(
                        MarkerValue::Extra,
                        operator,
                        MarkerValue::QuotedString(value),
                    ),
                }
            }
            // This is either MarkerEnvVersion, MarkerEnvString or Extra inverted
            MarkerValue::QuotedString(l_string) => {
                match r_value {
                    // The only sound choice for this is `<quoted PEP 440 version> <version op>` <version key>
                    MarkerValue::MarkerEnvVersion(key) => {
                        match MarkerExpression::version_inverted(
                            &l_string,
                            operator,
                            key.clone(),
                            reporter,
                        ) {
                            Some(expr) => expr,
                            None => MarkerExpression::arbitrary(
                                MarkerValue::QuotedString(l_string),
                                operator,
                                MarkerValue::MarkerEnvVersion(key),
                            ),
                        }
                    }
                    // '...' == <env key>
                    MarkerValue::MarkerEnvString(key) => MarkerExpression::StringInverted {
                        key,
                        operator,
                        value: l_string,
                    },
                    // `'...' == extra`
                    MarkerValue::Extra => {
                        match MarkerExpression::extra(operator, &l_string, reporter) {
                            Some(expr) => expr,
                            None => MarkerExpression::arbitrary(
                                MarkerValue::QuotedString(l_string),
                                operator,
                                MarkerValue::Extra,
                            ),
                        }
                    }
                    // `'...' == '...'`, doesn't make much sense
                    MarkerValue::QuotedString(_) => {
                        // Not even pypa/packaging 22.0 supports this
                        // https://github.com/pypa/packaging/issues/632
                        let expr = MarkerExpression::arbitrary(
                            MarkerValue::QuotedString(l_string),
                            operator,
                            r_value,
                        );

                        reporter.report(MarkerWarningKind::StringStringComparison, format!(
                            "Comparing two quoted strings with each other doesn't make sense: {expr},
                            will evaluate to false"
                        ));

                        expr
                    }
                }
            }
        }
    }

    /// Creates an instance of [`MarkerExpression::Arbitrary`] with the given values.
    fn arbitrary(
        l_value: MarkerValue,
        operator: MarkerOperator,
        r_value: MarkerValue,
    ) -> MarkerExpression {
        MarkerExpression::Arbitrary {
            l_value,
            operator,
            r_value,
        }
    }

    /// Creates an instance of [`MarkerExpression::Version`] with the given values.
    ///
    /// Reports a warning on failure, and returns `None`.
    pub fn version(
        key: MarkerValueVersion,
        marker_operator: MarkerOperator,
        value: &str,
        reporter: &mut impl Reporter,
    ) -> Option<MarkerExpression> {
        let pattern = match value.parse::<VersionPattern>() {
            Ok(pattern) => pattern,
            Err(err) => {
                reporter.report(
                    MarkerWarningKind::Pep440Error,
                    format!(
                        "Expected PEP 440 version to compare with {key}, found {value}, will evaluate to false: {err}"
                    ),
                );

                return None;
            }
        };

        let Some(operator) = marker_operator.to_pep440_operator() else {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!(
                    "Expected PEP 440 version operator to compare {} with '{}',
                    found '{}', will evaluate to false",
                    key,
                    pattern.version(),
                    marker_operator
                ),
            );

            return None;
        };

        let specifier = match VersionSpecifier::from_pattern(operator, pattern) {
            Ok(specifier) => specifier,
            Err(err) => {
                reporter.report(
                    MarkerWarningKind::Pep440Error,
                    format!("Invalid operator/version combination: {err}"),
                );
                return None;
            }
        };

        Some(MarkerExpression::Version { key, specifier })
    }

    /// Creates an instance of [`MarkerExpression::VersionInverted`] with the given values.
    ///
    /// Reports a warning on failure, and returns `None`.
    fn version_inverted(
        value: &str,
        marker_operator: MarkerOperator,
        key: MarkerValueVersion,
        reporter: &mut impl Reporter,
    ) -> Option<MarkerExpression> {
        let version = match value.parse::<Version>() {
            Ok(version) => version,
            Err(err) => {
                reporter.report(
                    MarkerWarningKind::Pep440Error,
                    format!(
                        "Expected PEP 440 version to compare with {key}, found {value}, will evaluate to false: {err}"
                    ),
                );

                return None;
            }
        };

        let Some(operator) = marker_operator.to_pep440_operator() else {
            reporter.report(
                MarkerWarningKind::Pep440Error,
                format!(
                    "Expected PEP 440 version operator to compare {key} with '{version}',
                    found '{marker_operator}', will evaluate to false"
                ),
            );

            return None;
        };

        Some(MarkerExpression::VersionInverted {
            version,
            operator,
            key,
        })
    }

    /// Creates an instance of [`MarkerExpression::Extra`] with the given values, falling back to
    /// [`MarkerExpression::Arbitrary`] on failure.
    fn extra(
        operator: MarkerOperator,
        value: &str,
        reporter: &mut impl Reporter,
    ) -> Option<MarkerExpression> {
        let name = match ExtraName::from_str(value) {
            Ok(name) => name,
            Err(err) => {
                reporter.report(
                    MarkerWarningKind::ExtraInvalidComparison,
                    format!("Expected extra name, found '{value}', will evaluate to false: {err}"),
                );

                return None;
            }
        };

        if let Some(operator) = ExtraOperator::from_marker_operator(operator) {
            Some(MarkerExpression::Extra { operator, name })
        } else {
            reporter.report(
                MarkerWarningKind::ExtraInvalidComparison,
                "Comparing extra with something other than a quoted string is wrong,
                    will evaluate to false"
                    .to_string(),
            );
            None
        }
    }

    /// Evaluate a <`marker_value`> <`marker_op`> <`marker_value`> expression
    ///
    /// When `env` is `None`, all expressions that reference the environment
    /// will evaluate as `true`.
    fn evaluate(
        &self,
        env: Option<&MarkerEnvironment>,
        extras: &[ExtraName],
        reporter: &mut impl Reporter,
    ) -> bool {
        match self {
            MarkerExpression::Version { key, specifier } => env
                .map(|env| specifier.contains(env.get_version(key)))
                .unwrap_or(true),
            MarkerExpression::VersionInverted {
                key,
                operator,
                version,
            } => env
                .map(|env| {
                    let r_version = VersionPattern::verbatim(env.get_version(key).clone());
                    let specifier = match VersionSpecifier::from_pattern(*operator, r_version) {
                        Ok(specifier) => specifier,
                        Err(err) => {
                            reporter.report(
                                MarkerWarningKind::Pep440Error,
                                format!("Invalid operator/version combination: {err}"),
                            );

                            return false;
                        }
                    };

                    specifier.contains(version)
                })
                .unwrap_or(true),
            MarkerExpression::String {
                key,
                operator,
                value,
            } => env
                .map(|env| {
                    let l_string = env.get_string(key);
                    Self::compare_strings(l_string, *operator, value, reporter)
                })
                .unwrap_or(true),
            MarkerExpression::StringInverted {
                key,
                operator,
                value,
            } => env
                .map(|env| {
                    let r_string = env.get_string(key);
                    Self::compare_strings(value, *operator, r_string, reporter)
                })
                .unwrap_or(true),
            MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name,
            } => extras.contains(name),
            MarkerExpression::Extra {
                operator: ExtraOperator::NotEqual,
                name,
            } => !extras.contains(name),
            MarkerExpression::Arbitrary { .. } => false,
        }
    }

    /// Evaluates only the extras and python version part of the markers. We use this during
    /// dependency resolution when we want to have packages for all possible environments but
    /// already know the extras and the possible python versions (from `requires-python`)
    ///
    /// This considers only expression in the from `extra == '...'`, `'...' == extra`,
    /// `python_version <pep PEP 440 operator> '...'` and
    /// `'...' <pep PEP 440 operator>  python_version`.
    ///
    /// Note that unlike [`Self::evaluate`] this does not perform any checks for bogus expressions but
    /// will simply return true.
    ///
    /// ```rust
    /// # use std::collections::HashSet;
    /// # use std::str::FromStr;
    /// # use pep508_rs::{MarkerTree, Pep508Error};
    /// # use pep440_rs::Version;
    /// # use uv_normalize::ExtraName;
    ///
    /// # fn main() -> Result<(), Pep508Error> {
    /// let marker_tree = MarkerTree::from_str(r#"("linux" in sys_platform) and extra == 'day'"#)?;
    /// let versions: Vec<Version> = (8..12).map(|minor| Version::new([3, minor])).collect();
    /// assert!(marker_tree.evaluate_extras_and_python_version(&[ExtraName::from_str("day").unwrap()].into(), &versions));
    /// assert!(!marker_tree.evaluate_extras_and_python_version(&[ExtraName::from_str("night").unwrap()].into(), &versions));
    ///
    /// let marker_tree = MarkerTree::from_str(r#"extra == 'day' and python_version < '3.11' and '3.10' <= python_version"#)?;
    /// assert!(!marker_tree.evaluate_extras_and_python_version(&[ExtraName::from_str("day").unwrap()].into(), &vec![Version::new([3, 9])]));
    /// assert!(marker_tree.evaluate_extras_and_python_version(&[ExtraName::from_str("day").unwrap()].into(), &vec![Version::new([3, 10])]));
    /// assert!(!marker_tree.evaluate_extras_and_python_version(&[ExtraName::from_str("day").unwrap()].into(), &vec![Version::new([3, 11])]));
    /// # Ok(())
    /// # }
    /// ```
    fn evaluate_extras_and_python_version(
        &self,
        extras: &HashSet<ExtraName>,
        python_versions: &[Version],
    ) -> bool {
        match self {
            MarkerExpression::Version {
                key: MarkerValueVersion::PythonVersion,
                specifier,
            } => python_versions
                .iter()
                .any(|l_version| specifier.contains(l_version)),
            MarkerExpression::VersionInverted {
                key: MarkerValueVersion::PythonVersion,
                operator,
                version,
            } => {
                python_versions.iter().any(|r_version| {
                    // operator and right hand side make the specifier and in this case the
                    // right hand is `python_version` so changes every iteration
                    let Ok(specifier) = VersionSpecifier::from_pattern(
                        *operator,
                        VersionPattern::verbatim(r_version.clone()),
                    ) else {
                        return true;
                    };

                    specifier.contains(version)
                })
            }
            MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name,
            } => extras.contains(name),
            MarkerExpression::Extra {
                operator: ExtraOperator::NotEqual,
                name,
            } => !extras.contains(name),
            _ => true,
        }
    }

    /// Compare strings by PEP 508 logic, with warnings
    fn compare_strings(
        l_string: &str,
        operator: MarkerOperator,
        r_string: &str,
        reporter: &mut impl Reporter,
    ) -> bool {
        match operator {
            MarkerOperator::Equal => l_string == r_string,
            MarkerOperator::NotEqual => l_string != r_string,
            MarkerOperator::GreaterThan => {
                reporter.report(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                );
                l_string > r_string
            }
            MarkerOperator::GreaterEqual => {
                reporter.report(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                );
                l_string >= r_string
            }
            MarkerOperator::LessThan => {
                reporter.report(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                );
                l_string < r_string
            }
            MarkerOperator::LessEqual => {
                reporter.report(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                );
                l_string <= r_string
            }
            MarkerOperator::TildeEqual => {
                reporter.report(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Can't compare {l_string} and {r_string} with `~=`"),
                );
                false
            }
            MarkerOperator::In => r_string.contains(l_string),
            MarkerOperator::NotIn => !r_string.contains(l_string),
        }
    }
}

impl FromStr for MarkerExpression {
    type Err = Pep508Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MarkerExpression::parse_reporter(s, &mut TracingReporter)
    }
}

impl Display for MarkerExpression {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkerExpression::Version { key, specifier } => {
                write!(
                    f,
                    "{key} {} '{}'",
                    specifier.operator(),
                    specifier.version()
                )
            }
            MarkerExpression::VersionInverted {
                version,
                operator,
                key,
            } => {
                write!(f, "'{version}' {operator} {key}")
            }
            MarkerExpression::String {
                key,
                operator,
                value,
            } => {
                write!(f, "{key} {operator} '{value}'")
            }
            MarkerExpression::StringInverted {
                value,
                operator,
                key,
            } => {
                write!(f, "'{value}' {operator} {key}")
            }
            MarkerExpression::Extra { operator, name } => {
                write!(f, "extra {operator} '{name}'")
            }
            MarkerExpression::Arbitrary {
                l_value,
                operator,
                r_value,
            } => {
                write!(f, "{l_value} {operator} {r_value}")
            }
        }
    }
}

/// Represents one of the nested marker expressions with and/or/parentheses
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerTree {
    /// A simple expression such as `python_version > "3.8"`
    Expression(MarkerExpression),
    /// An and between nested expressions, such as
    /// `python_version > "3.8" and implementation_name == 'cpython'`
    And(Vec<MarkerTree>),
    /// An or between nested expressions, such as
    /// `python_version > "3.8" or implementation_name == 'cpython'`
    Or(Vec<MarkerTree>),
}

impl<'de> Deserialize<'de> for MarkerTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

impl Serialize for MarkerTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl FromStr for MarkerTree {
    type Err = Pep508Error;

    fn from_str(markers: &str) -> Result<Self, Self::Err> {
        parse_markers(markers, &mut TracingReporter)
    }
}

impl MarkerTree {
    /// Like [`FromStr::from_str`], but the caller chooses the return type generic.
    pub fn parse_str<T: Pep508Url>(markers: &str) -> Result<Self, Pep508Error<T>> {
        parse_markers(markers, &mut TracingReporter)
    }

    /// Parse a [`MarkerTree`] from a string with the given reporter.
    pub fn parse_reporter(
        markers: &str,
        reporter: &mut impl Reporter,
    ) -> Result<Self, Pep508Error> {
        parse_markers(markers, reporter)
    }

    /// Whether the marker is `MarkerTree::And(Vec::new())`.
    pub fn is_universal(&self) -> bool {
        self == &MarkerTree::And(Vec::new())
    }

    /// Does this marker apply in the given environment?
    pub fn evaluate(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        self.evaluate_optional_environment(Some(env), extras)
    }

    /// Evaluates this marker tree against an optional environment and a
    /// possibly empty sequence of extras.
    ///
    /// When an environment is not provided, all marker expressions based on
    /// the environment evaluate to `true`. That is, this provides environment
    /// independent marker evaluation. In practice, this means only the extras
    /// are evaluated when an environment is not provided.
    pub fn evaluate_optional_environment(
        &self,
        env: Option<&MarkerEnvironment>,
        extras: &[ExtraName],
    ) -> bool {
        self.report_deprecated_options(&mut TracingReporter);
        match self {
            Self::Expression(expression) => expression.evaluate(env, extras, &mut TracingReporter),
            Self::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_reporter_impl(env, extras, &mut TracingReporter)),
            Self::Or(expressions) => expressions
                .iter()
                .any(|x| x.evaluate_reporter_impl(env, extras, &mut TracingReporter)),
        }
    }

    /// Remove the extras from a marker, returning `None` if the marker tree evaluates to `true`.
    ///
    /// Any `extra` markers that are always `true` given the provided extras will be removed.
    /// Any `extra` markers that are always `false` given the provided extras will be left
    /// unchanged.
    ///
    /// For example, if `dev` is a provided extra, given `sys_platform == 'linux' and extra == 'dev'`,
    /// the marker will be simplified to `sys_platform == 'linux'`.
    pub fn simplify_extras(self, extras: &[ExtraName]) -> Option<MarkerTree> {
        self.simplify_extras_with(|name| extras.contains(name))
    }

    /// Remove the extras from a marker, returning `None` if the marker tree evaluates to `true`.
    ///
    /// Any `extra` markers that are always `true` given the provided predicate will be removed.
    /// Any `extra` markers that are always `false` given the provided predicate will be left
    /// unchanged.
    ///
    /// For example, if `is_extra('dev')` is true, given
    /// `sys_platform == 'linux' and extra == 'dev'`, the marker will be simplified to
    /// `sys_platform == 'linux'`.
    pub fn simplify_extras_with(self, is_extra: impl Fn(&ExtraName) -> bool) -> Option<MarkerTree> {
        // Because `simplify_extras_with_impl` is recursive, and we need to use
        // our predicate in recursive calls, we need the predicate itself to
        // have some indirection (or else we'd have to clone it). To avoid a
        // recursive type at codegen time, we just introduce the indirection
        // here, but keep the calling API ergonomic.
        self.simplify_extras_with_impl(&is_extra)
    }

    fn simplify_extras_with_impl(
        self,
        is_extra: &impl Fn(&ExtraName) -> bool,
    ) -> Option<MarkerTree> {
        /// Returns `true` if the given expression is always `true` given the set of extras.
        fn is_true(expression: &MarkerExpression, is_extra: impl Fn(&ExtraName) -> bool) -> bool {
            match expression {
                MarkerExpression::Extra {
                    operator: ExtraOperator::Equal,
                    name,
                } => is_extra(name),
                MarkerExpression::Extra {
                    operator: ExtraOperator::NotEqual,
                    name,
                } => !is_extra(name),
                _ => false,
            }
        }

        match self {
            Self::Expression(expression) => {
                // If the expression is true, we can remove the marker entirely.
                if is_true(&expression, is_extra) {
                    None
                } else {
                    // If not, return the original marker.
                    Some(Self::Expression(expression))
                }
            }
            Self::And(expressions) => {
                // Remove any expressions that are _true_ due to the presence of an extra.
                let simplified = expressions
                    .into_iter()
                    .filter_map(|marker| marker.simplify_extras_with_impl(is_extra))
                    .collect::<Vec<_>>();

                // If there are no expressions left, return None.
                if simplified.is_empty() {
                    None
                } else if simplified.len() == 1 {
                    // If there is only one expression left, return the remaining expression.
                    simplified.into_iter().next()
                } else {
                    // If there are still expressions left, return the simplified marker.
                    Some(Self::And(simplified))
                }
            }
            Self::Or(expressions) => {
                let num_expressions = expressions.len();

                // Remove any expressions that are _true_ due to the presence of an extra.
                let simplified = expressions
                    .into_iter()
                    .filter_map(|marker| marker.simplify_extras_with_impl(is_extra))
                    .collect::<Vec<_>>();

                // If _any_ of the expressions are true (i.e., if any of the markers were filtered
                // out in the above filter step), the entire marker is true.
                if simplified.len() < num_expressions {
                    None
                } else if simplified.len() == 1 {
                    // If there is only one expression left, return the remaining expression.
                    simplified.into_iter().next()
                } else {
                    // If there are still expressions left, return the simplified marker.
                    Some(Self::Or(simplified))
                }
            }
        }
    }

    /// Same as [`Self::evaluate`], but instead of using logging to warn, you can pass your own
    /// handler for warnings
    pub fn evaluate_reporter(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        reporter: &mut impl Reporter,
    ) -> bool {
        self.report_deprecated_options(reporter);
        self.evaluate_reporter_impl(Some(env), extras, reporter)
    }

    fn evaluate_reporter_impl(
        &self,
        env: Option<&MarkerEnvironment>,
        extras: &[ExtraName],
        reporter: &mut impl Reporter,
    ) -> bool {
        match self {
            Self::Expression(expression) => expression.evaluate(env, extras, reporter),
            Self::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_reporter_impl(env, extras, reporter)),
            Self::Or(expressions) => expressions
                .iter()
                .any(|x| x.evaluate_reporter_impl(env, extras, reporter)),
        }
    }

    /// Checks if the requirement should be activated with the given set of active extras and a set
    /// of possible python versions (from `requires-python`) without evaluating the remaining
    /// environment markers, i.e. if there is potentially an environment that could activate this
    /// requirement.
    ///
    /// Note that unlike [`Self::evaluate`] this does not perform any checks for bogus expressions but
    /// will simply return true. As caller you should separately perform a check with an environment
    /// and forward all warnings.
    pub fn evaluate_extras_and_python_version(
        &self,
        extras: &HashSet<ExtraName>,
        python_versions: &[Version],
    ) -> bool {
        match self {
            Self::Expression(expression) => {
                expression.evaluate_extras_and_python_version(extras, python_versions)
            }
            Self::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_extras_and_python_version(extras, python_versions)),
            Self::Or(expressions) => expressions
                .iter()
                .any(|x| x.evaluate_extras_and_python_version(extras, python_versions)),
        }
    }

    /// Same as [`Self::evaluate`], but instead of using logging to warn, you get a Vec with all
    /// warnings collected
    pub fn evaluate_collect_warnings(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
    ) -> (bool, Vec<(MarkerWarningKind, String)>) {
        let mut warnings = Vec::new();
        let mut reporter = |kind, warning| {
            warnings.push((kind, warning));
        };
        self.report_deprecated_options(&mut reporter);
        let result = self.evaluate_reporter_impl(Some(env), extras, &mut reporter);
        (result, warnings)
    }

    /// Report the deprecated marker from <https://peps.python.org/pep-0345/#environment-markers>
    fn report_deprecated_options(&self, reporter: &mut impl Reporter) {
        match self {
            Self::Expression(expression) => {
                let MarkerExpression::String { key, .. } = expression else {
                    return;
                };

                match key {
                    MarkerValueString::OsNameDeprecated => {
                        reporter.report(
                            MarkerWarningKind::DeprecatedMarkerName,
                            "os.name is deprecated in favor of os_name".to_string(),
                        );
                    }
                    MarkerValueString::PlatformMachineDeprecated => {
                        reporter.report(
                            MarkerWarningKind::DeprecatedMarkerName,
                            "platform.machine is deprecated in favor of platform_machine"
                                .to_string(),
                        );
                    }
                    MarkerValueString::PlatformPythonImplementationDeprecated => {
                        reporter.report(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "platform.python_implementation is deprecated in favor of platform_python_implementation".to_string(),
                            );
                    }
                    MarkerValueString::PythonImplementationDeprecated => {
                        reporter.report(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "python_implementation is deprecated in favor of platform_python_implementation".to_string(),
                            );
                    }
                    MarkerValueString::PlatformVersionDeprecated => {
                        reporter.report(
                            MarkerWarningKind::DeprecatedMarkerName,
                            "platform.version is deprecated in favor of platform_version"
                                .to_string(),
                        );
                    }
                    MarkerValueString::SysPlatformDeprecated => {
                        reporter.report(
                            MarkerWarningKind::DeprecatedMarkerName,
                            "sys.platform  is deprecated in favor of sys_platform".to_string(),
                        );
                    }
                    _ => {}
                }
            }
            Self::And(expressions) => {
                for expression in expressions {
                    expression.report_deprecated_options(reporter);
                }
            }
            Self::Or(expressions) => {
                for expression in expressions {
                    expression.report_deprecated_options(reporter);
                }
            }
        }
    }

    /// Combine this marker tree with the one given via a conjunction.
    ///
    /// This does some shallow flattening. That is, if `self` is a conjunction
    /// already, then `tree` is added to it instead of creating a new
    /// conjunction.
    pub fn and(&mut self, tree: MarkerTree) {
        match *self {
            MarkerTree::Expression(_) | MarkerTree::Or(_) => {
                let this = std::mem::replace(self, MarkerTree::And(vec![]));
                *self = MarkerTree::And(vec![this]);
            }
            MarkerTree::And(_) => {}
        }
        if let MarkerTree::And(ref mut exprs) = *self {
            if let MarkerTree::And(tree) = tree {
                exprs.extend(tree);
            } else {
                exprs.push(tree);
            }
        }
    }

    /// Combine this marker tree with the one given via a disjunction.
    ///
    /// This does some shallow flattening. That is, if `self` is a disjunction
    /// already, then `tree` is added to it instead of creating a new
    /// disjunction.
    pub fn or(&mut self, tree: MarkerTree) {
        match *self {
            MarkerTree::Expression(_) | MarkerTree::And(_) => {
                let this = std::mem::replace(self, MarkerTree::And(vec![]));
                *self = MarkerTree::Or(vec![this]);
            }
            MarkerTree::Or(_) => {}
        }
        if let MarkerTree::Or(ref mut exprs) = *self {
            if let MarkerTree::Or(tree) = tree {
                exprs.extend(tree);
            } else {
                exprs.push(tree);
            }
        }
    }
}

impl Display for MarkerTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let format_inner = |expression: &Self| {
            if matches!(expression, Self::Expression(_)) {
                format!("{expression}")
            } else {
                format!("({expression})")
            }
        };
        match self {
            Self::Expression(expression) => write!(f, "{expression}"),
            Self::And(and_list) => f.write_str(
                &and_list
                    .iter()
                    .map(format_inner)
                    .collect::<Vec<String>>()
                    .join(" and "),
            ),
            Self::Or(or_list) => f.write_str(
                &or_list
                    .iter()
                    .map(format_inner)
                    .collect::<Vec<String>>()
                    .join(" or "),
            ),
        }
    }
}

/// ```text
/// version_cmp   = wsp* <'<=' | '<' | '!=' | '==' | '>=' | '>' | '~=' | '==='>
/// marker_op     = version_cmp | (wsp* 'in') | (wsp* 'not' wsp+ 'in')
/// ```
/// The `wsp*` has already been consumed by the caller.
fn parse_marker_operator<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<MarkerOperator, Pep508Error<T>> {
    let (start, len) = if cursor.peek_char().is_some_and(char::is_alphabetic) {
        // "in" or "not"
        cursor.take_while(|char| !char.is_whitespace() && char != '\'' && char != '"')
    } else {
        // A mathematical operator
        cursor.take_while(|char| matches!(char, '<' | '=' | '>' | '~' | '!'))
    };
    let operator = cursor.slice(start, len);
    if operator == "not" {
        // 'not' wsp+ 'in'
        match cursor.next() {
            None => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(
                        "Expected whitespace after 'not', found end of input".to_string(),
                    ),
                    start: cursor.pos(),
                    len: 1,
                    input: cursor.to_string(),
                });
            }
            Some((_, whitespace)) if whitespace.is_whitespace() => {}
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected whitespace after 'not', found '{other}'"
                    )),
                    start: pos,
                    len: other.len_utf8(),
                    input: cursor.to_string(),
                });
            }
        };
        cursor.eat_whitespace();
        cursor.next_expect_char('i', cursor.pos())?;
        cursor.next_expect_char('n', cursor.pos())?;
        return Ok(MarkerOperator::NotIn);
    }
    MarkerOperator::from_str(operator).map_err(|_| Pep508Error {
        message: Pep508ErrorSource::String(format!(
            "Expected a valid marker operator (such as '>=' or 'not in'), found '{operator}'"
        )),
        start,
        len,
        input: cursor.to_string(),
    })
}

/// Either a single or double quoted string or one of '`python_version`', '`python_full_version`',
/// '`os_name`', '`sys_platform`', '`platform_release`', '`platform_system`', '`platform_version`',
/// '`platform_machine`', '`platform_python_implementation`', '`implementation_name`',
/// '`implementation_version`', 'extra'
fn parse_marker_value<T: Pep508Url>(cursor: &mut Cursor) -> Result<MarkerValue, Pep508Error<T>> {
    // > User supplied constants are always encoded as strings with either ' or " quote marks. Note
    // > that backslash escapes are not defined, but existing implementations do support them. They
    // > are not included in this specification because they add complexity and there is no observable
    // > need for them today. Similarly we do not define non-ASCII character support: all the runtime
    // > variables we are referencing are expected to be ASCII-only.
    match cursor.peek() {
        None => Err(Pep508Error {
            message: Pep508ErrorSource::String(
                "Expected marker value, found end of dependency specification".to_string(),
            ),
            start: cursor.pos(),
            len: 1,
            input: cursor.to_string(),
        }),
        // It can be a string ...
        Some((start_pos, quotation_mark @ ('"' | '\''))) => {
            cursor.next();
            let (start, len) = cursor.take_while(|c| c != quotation_mark);
            let value = cursor.slice(start, len).to_string();
            cursor.next_expect_char(quotation_mark, start_pos)?;
            Ok(MarkerValue::string_value(value))
        }
        // ... or it can be a keyword
        Some(_) => {
            let (start, len) = cursor.take_while(|char| {
                !char.is_whitespace() && !['>', '=', '<', '!', '~', ')'].contains(&char)
            });
            let key = cursor.slice(start, len);
            MarkerValue::from_str(key).map_err(|_| Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected a valid marker name, found '{key}'"
                )),
                start,
                len,
                input: cursor.to_string(),
            })
        }
    }
}

/// ```text
/// marker_var:l marker_op:o marker_var:r
/// ```
fn parse_marker_key_op_value<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerExpression, Pep508Error<T>> {
    cursor.eat_whitespace();
    let lvalue = parse_marker_value(cursor)?;
    cursor.eat_whitespace();
    // "not in" and "in" must be preceded by whitespace. We must already have matched a whitespace
    // when we're here because other `parse_marker_key` would have pulled the characters in and
    // errored
    let operator = parse_marker_operator(cursor)?;
    cursor.eat_whitespace();
    let rvalue = parse_marker_value(cursor)?;

    Ok(MarkerExpression::new(lvalue, operator, rvalue, reporter))
}

/// ```text
/// marker_expr   = marker_var:l marker_op:o marker_var:r -> (o, l, r)
///               | wsp* '(' marker:m wsp* ')' -> m
/// ```
fn parse_marker_expr<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    cursor.eat_whitespace();
    if let Some(start_pos) = cursor.eat_char('(') {
        let marker = parse_marker_or(cursor, reporter)?;
        cursor.next_expect_char(')', start_pos)?;
        Ok(marker)
    } else {
        Ok(MarkerTree::Expression(parse_marker_key_op_value(
            cursor, reporter,
        )?))
    }
}

/// ```text
/// marker_and    = marker_expr:l wsp* 'and' marker_expr:r -> ('and', l, r)
///               | marker_expr:m -> m
/// ```
fn parse_marker_and<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    parse_marker_op(cursor, "and", MarkerTree::And, parse_marker_expr, reporter)
}

/// ```text
/// marker_or     = marker_and:l wsp* 'or' marker_and:r -> ('or', l, r)
///                   | marker_and:m -> m
/// ```
fn parse_marker_or<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    parse_marker_op(cursor, "or", MarkerTree::Or, parse_marker_and, reporter)
}

/// Parses both `marker_and` and `marker_or`
fn parse_marker_op<T: Pep508Url, R: Reporter>(
    cursor: &mut Cursor,
    op: &str,
    op_constructor: fn(Vec<MarkerTree>) -> MarkerTree,
    parse_inner: fn(&mut Cursor, &mut R) -> Result<MarkerTree, Pep508Error<T>>,
    reporter: &mut R,
) -> Result<MarkerTree, Pep508Error<T>> {
    // marker_and or marker_expr
    let first_element = parse_inner(cursor, reporter)?;
    // wsp*
    cursor.eat_whitespace();
    // Check if we're done here instead of invoking the whole vec allocating loop
    if matches!(cursor.peek_char(), None | Some(')')) {
        return Ok(first_element);
    }

    let mut expressions = Vec::with_capacity(1);
    expressions.push(first_element);
    loop {
        // wsp*
        cursor.eat_whitespace();
        // ('or' marker_and) or ('and' marker_or)
        let (start, len) = cursor.peek_while(|c| !c.is_whitespace());
        match cursor.slice(start, len) {
            value if value == op => {
                cursor.take_while(|c| !c.is_whitespace());
                let expression = parse_inner(cursor, reporter)?;
                expressions.push(expression);
            }
            _ => {
                // Build minimal trees
                return if expressions.len() == 1 {
                    Ok(expressions.remove(0))
                } else {
                    Ok(op_constructor(expressions))
                };
            }
        }
    }
}

/// ```text
/// marker        = marker_or^
/// ```
pub(crate) fn parse_markers_cursor<T: Pep508Url>(
    cursor: &mut Cursor,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    let marker = parse_marker_or(cursor, reporter)?;
    cursor.eat_whitespace();
    if let Some((pos, unexpected)) = cursor.next() {
        // If we're here, both parse_marker_or and parse_marker_and returned because the next
        // character was neither "and" nor "or"
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(format!(
                "Unexpected character '{unexpected}', expected 'and', 'or' or end of input"
            )),
            start: pos,
            len: cursor.remaining(),
            input: cursor.to_string(),
        });
    };
    Ok(marker)
}

/// Parses markers such as `python_version < '3.8'` or
/// `python_version == "3.10" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))`
fn parse_markers<T: Pep508Url>(
    markers: &str,
    reporter: &mut impl Reporter,
) -> Result<MarkerTree, Pep508Error<T>> {
    let mut chars = Cursor::new(markers);
    parse_markers_cursor(&mut chars, reporter)
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use insta::assert_snapshot;

    use pep440_rs::VersionSpecifier;
    use uv_normalize::ExtraName;

    use crate::marker::{ExtraOperator, MarkerEnvironment, MarkerEnvironmentBuilder};
    use crate::{
        MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString, MarkerValueVersion,
    };

    fn parse_err(input: &str) -> String {
        MarkerTree::from_str(input).unwrap_err().to_string()
    }

    fn env37() -> MarkerEnvironment {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "",
            implementation_version: "3.7",
            os_name: "linux",
            platform_machine: "",
            platform_python_implementation: "",
            platform_release: "",
            platform_system: "",
            platform_version: "",
            python_full_version: "3.7",
            python_version: "3.7",
            sys_platform: "linux",
        })
        .unwrap()
    }

    /// Copied from <https://github.com/pypa/packaging/blob/85ff971a250dc01db188ef9775499c15553a8c95/tests/test_markers.py#L175-L221>
    #[test]
    fn test_marker_equivalence() {
        let values = [
            (r"python_version == '2.7'", r#"python_version == "2.7""#),
            (r#"python_version == "2.7""#, r#"python_version == "2.7""#),
            (
                r#"python_version == "2.7" and os_name == "posix""#,
                r#"python_version == "2.7" and os_name == "posix""#,
            ),
            (
                r#"python_version == "2.7" or os_name == "posix""#,
                r#"python_version == "2.7" or os_name == "posix""#,
            ),
            (
                r#"python_version == "2.7" and os_name == "posix" or sys_platform == "win32""#,
                r#"python_version == "2.7" and os_name == "posix" or sys_platform == "win32""#,
            ),
            (r#"(python_version == "2.7")"#, r#"python_version == "2.7""#),
            (
                r#"(python_version == "2.7" and sys_platform == "win32")"#,
                r#"python_version == "2.7" and sys_platform == "win32""#,
            ),
            (
                r#"python_version == "2.7" and (sys_platform == "win32" or sys_platform == "linux")"#,
                r#"python_version == "2.7" and (sys_platform == "win32" or sys_platform == "linux")"#,
            ),
        ];
        for (a, b) in values {
            assert_eq!(
                MarkerTree::from_str(a).unwrap(),
                MarkerTree::from_str(b).unwrap(),
                "{a} {b}"
            );
        }
    }

    #[test]
    fn test_marker_evaluation() {
        let env27 = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "",
            implementation_version: "2.7",
            os_name: "linux",
            platform_machine: "",
            platform_python_implementation: "",
            platform_release: "",
            platform_system: "",
            platform_version: "",
            python_full_version: "2.7",
            python_version: "2.7",
            sys_platform: "linux",
        })
        .unwrap();
        let env37 = env37();
        let marker1 = MarkerTree::from_str("python_version == '2.7'").unwrap();
        let marker2 = MarkerTree::from_str(
            "os_name == \"linux\" or python_version == \"3.7\" and sys_platform == \"win32\"",
        )
        .unwrap();
        let marker3 = MarkerTree::from_str(
            "python_version == \"2.7\" and (sys_platform == \"win32\" or sys_platform == \"linux\")",
        ).unwrap();
        assert!(marker1.evaluate(&env27, &[]));
        assert!(!marker1.evaluate(&env37, &[]));
        assert!(marker2.evaluate(&env27, &[]));
        assert!(marker2.evaluate(&env37, &[]));
        assert!(marker3.evaluate(&env27, &[]));
        assert!(!marker3.evaluate(&env37, &[]));
    }

    #[test]
    #[cfg(feature = "tracing")]
    fn warnings() {
        let env37 = env37();
        testing_logger::setup();
        let compare_keys = MarkerTree::from_str("platform_version == sys_platform").unwrap();
        compare_keys.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Comparing two markers with each other doesn't make any sense, will evaluate to false"
            );
            assert_eq!(captured_logs[0].level, log::Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let non_pep440 = MarkerTree::from_str("python_version >= '3.9.'").unwrap();
        non_pep440.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Expected PEP 440 version to compare with python_version, found '3.9.', \
                 will evaluate to false: after parsing '3.9', found '.', which is \
                 not part of a valid version"
            );
            assert_eq!(captured_logs[0].level, log::Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let string_string = MarkerTree::from_str("'b' >= 'a'").unwrap();
        string_string.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Comparing two quoted strings with each other doesn't make sense: 'b' >= 'a', will evaluate to false"
            );
            assert_eq!(captured_logs[0].level, log::Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let string_string = MarkerTree::from_str(r"os.name == 'posix' and platform.machine == 'x86_64' and platform.python_implementation == 'CPython' and 'Ubuntu' in platform.version and sys.platform == 'linux'").unwrap();
        string_string.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            let messages: Vec<_> = captured_logs
                .iter()
                .map(|message| {
                    assert_eq!(message.level, log::Level::Warn);
                    &message.body
                })
                .collect();
            let expected = [
                "os.name is deprecated in favor of os_name",
                "platform.machine is deprecated in favor of platform_machine",
                "platform.python_implementation is deprecated in favor of platform_python_implementation",
                "platform.version is deprecated in favor of platform_version",
                "sys.platform  is deprecated in favor of sys_platform"
            ];
            assert_eq!(messages, &expected);
        });
    }

    #[test]
    fn test_not_in() {
        MarkerTree::from_str("'posix' not in os_name").unwrap();
    }

    #[test]
    fn test_marker_version_inverted() {
        let env37 = env37();
        let (result, warnings) = MarkerTree::from_str("python_version > '3.6'")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(result);

        let (result, warnings) = MarkerTree::from_str("'3.6' > python_version")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(!result);
    }

    #[test]
    fn test_marker_string_inverted() {
        let env37 = env37();
        let (result, warnings) = MarkerTree::from_str("'nux' in sys_platform")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(result);

        let (result, warnings) = MarkerTree::from_str("sys_platform in 'nux'")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(!result);
    }

    #[test]
    fn test_marker_version_star() {
        let env37 = env37();
        let (result, warnings) = MarkerTree::from_str("python_version == '3.7.*'")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(result);
    }

    #[test]
    fn test_tilde_equal() {
        let env37 = env37();
        let (result, warnings) = MarkerTree::from_str("python_version ~= '3.7'")
            .unwrap()
            .evaluate_collect_warnings(&env37, &[]);
        assert_eq!(warnings, &[]);
        assert!(result);
    }

    #[test]
    fn test_closing_parentheses() {
        MarkerTree::from_str(r#"( "linux" in sys_platform) and extra == 'all'"#).unwrap();
    }

    #[test]
    fn wrong_quotes_dot_star() {
        assert_snapshot!(
            parse_err(r#"python_version == "3.8".* and python_version >= "3.8""#),
            @r#"
            Unexpected character '.', expected 'and', 'or' or end of input
            python_version == "3.8".* and python_version >= "3.8"
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"#
        );
        assert_snapshot!(
            parse_err(r#"python_version == "3.8".*"#),
            @r#"
            Unexpected character '.', expected 'and', 'or' or end of input
            python_version == "3.8".*
                                   ^"#
        );
    }

    #[test]
    fn test_marker_expression() {
        assert_eq!(
            MarkerExpression::from_str(r#"os_name == "nt""#).unwrap(),
            MarkerExpression::String {
                key: MarkerValueString::OsName,
                operator: MarkerOperator::Equal,
                value: "nt".to_string(),
            }
        );
    }

    #[test]
    fn test_marker_expression_inverted() {
        assert_eq!(
            MarkerTree::from_str(
                r#""nt" in os_name and '3.7' >= python_version and python_full_version >= '3.7'"#
            )
            .unwrap(),
            MarkerTree::And(vec![
                MarkerTree::Expression(MarkerExpression::StringInverted {
                    value: "nt".to_string(),
                    operator: MarkerOperator::In,
                    key: MarkerValueString::OsName,
                }),
                MarkerTree::Expression(MarkerExpression::VersionInverted {
                    key: MarkerValueVersion::PythonVersion,
                    operator: pep440_rs::Operator::GreaterThanEqual,
                    version: "3.7".parse().unwrap(),
                }),
                MarkerTree::Expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::from_pattern(
                        pep440_rs::Operator::GreaterThanEqual,
                        "3.7".parse().unwrap()
                    )
                    .unwrap()
                }),
            ])
        );
    }

    #[test]
    fn test_marker_expression_to_long() {
        let err = MarkerExpression::from_str(r#"os_name == "nt" and python_version >= "3.8""#)
            .unwrap_err()
            .to_string();
        assert_snapshot!(
            err,
            @r#"
            Unexpected character 'a', expected end of input
            os_name == "nt" and python_version >= "3.8"
                            ^^^^^^^^^^^^^^^^^^^^^^^^^^"#
        );
    }

    #[test]
    fn test_marker_environment_from_json() {
        let _env: MarkerEnvironment = serde_json::from_str(
            r##"{
                "implementation_name": "cpython",
                "implementation_version": "3.7.13",
                "os_name": "posix",
                "platform_machine": "x86_64",
                "platform_python_implementation": "CPython",
                "platform_release": "5.4.188+",
                "platform_system": "Linux",
                "platform_version": "#1 SMP Sun Apr 24 10:03:06 PDT 2022",
                "python_full_version": "3.7.13",
                "python_version": "3.7",
                "sys_platform": "linux"
            }"##,
        )
        .unwrap();
    }

    #[test]
    fn test_simplify_extras() {
        // Given `os_name == "nt" and extra == "dev"`, simplify to `os_name == "nt"`.
        let markers = MarkerTree::from_str(r#"os_name == "nt" and extra == "dev""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(
            simplified,
            Some(MarkerTree::Expression(MarkerExpression::String {
                key: MarkerValueString::OsName,
                operator: MarkerOperator::Equal,
                value: "nt".to_string(),
            }))
        );

        // Given `os_name == "nt" or extra == "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"os_name == "nt" or extra == "dev""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, None);

        // Given `extra == "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"extra == "dev""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, None);

        // Given `extra == "dev" and extra == "test"`, simplify to `extra == "test"`.
        let markers = MarkerTree::from_str(r#"extra == "dev" and extra == "test""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(
            simplified,
            Some(MarkerTree::Expression(MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name: ExtraName::from_str("test").unwrap(),
            }))
        );

        // Given `os_name == "nt" and extra == "test"`, don't simplify.
        let markers = MarkerTree::from_str(r#"os_name == "nt" and extra == "test""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(
            simplified,
            Some(MarkerTree::And(vec![
                MarkerTree::Expression(MarkerExpression::String {
                    key: MarkerValueString::OsName,
                    operator: MarkerOperator::Equal,
                    value: "nt".to_string(),
                }),
                MarkerTree::Expression(MarkerExpression::Extra {
                    operator: ExtraOperator::Equal,
                    name: ExtraName::from_str("test").unwrap(),
                }),
            ]))
        );

        // Given `os_name == "nt" and (python_version == "3.7" or extra == "dev")`, simplify to
        // `os_name == "nt".
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" and (python_version == "3.7" or extra == "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(
            simplified,
            Some(MarkerTree::Expression(MarkerExpression::String {
                key: MarkerValueString::OsName,
                operator: MarkerOperator::Equal,
                value: "nt".to_string(),
            }))
        );

        // Given `os_name == "nt" or (python_version == "3.7" and extra == "dev")`, simplify to
        // `os_name == "nt" or python_version == "3.7"`.
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" or (python_version == "3.7" and extra == "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(
            simplified,
            Some(MarkerTree::Or(vec![
                MarkerTree::Expression(MarkerExpression::String {
                    key: MarkerValueString::OsName,
                    operator: MarkerOperator::Equal,
                    value: "nt".to_string(),
                }),
                MarkerTree::Expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonVersion,
                    specifier: VersionSpecifier::from_pattern(
                        pep440_rs::Operator::Equal,
                        "3.7".parse().unwrap()
                    )
                    .unwrap(),
                }),
            ]))
        );
    }
}
