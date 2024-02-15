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

use crate::{Cursor, Pep508Error, Pep508ErrorSource};
use pep440_rs::{Version, VersionPattern, VersionSpecifier};
#[cfg(feature = "pyo3")]
use pyo3::{
    basic::CompareOp, exceptions::PyValueError, pyclass, pymethods, PyAny, PyResult, Python,
};
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use tracing::warn;
use uv_normalize::ExtraName;

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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
            Self::PlatformPythonImplementation | Self::PlatformPythonImplementationDeprecated => {
                f.write_str("platform_python_implementation")
            }
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
    fn to_pep440_operator(&self) -> Option<pep440_rs::Operator> {
        match self {
            MarkerOperator::Equal => Some(pep440_rs::Operator::Equal),
            MarkerOperator::NotEqual => Some(pep440_rs::Operator::NotEqual),
            MarkerOperator::GreaterThan => Some(pep440_rs::Operator::GreaterThan),
            MarkerOperator::GreaterEqual => Some(pep440_rs::Operator::GreaterThanEqual),
            MarkerOperator::LessThan => Some(pep440_rs::Operator::LessThan),
            MarkerOperator::LessEqual => Some(pep440_rs::Operator::LessThanEqual),
            MarkerOperator::TildeEqual => Some(pep440_rs::Operator::TildeEqual),
            MarkerOperator::In => None,
            MarkerOperator::NotIn => None,
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
                    .map(|space| !space.is_empty() && space.trim().is_empty())
                    .unwrap_or_default() =>
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
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            string: s.to_string(),
            version: Version::from_str(s).map_err(|e| e.to_string())?,
        })
    }
}

impl Display for StringVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.version.fmt(f)
    }
}

#[cfg(feature = "serde")]
impl Serialize for StringVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.string)
    }
}

#[cfg(feature = "serde")]
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
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "pyo3", pyclass(get_all, module = "pep508"))]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MarkerEnvironment {
    pub implementation_name: String,
    pub implementation_version: StringVersion,
    pub os_name: String,
    pub platform_machine: String,
    pub platform_python_implementation: String,
    pub platform_release: String,
    pub platform_system: String,
    pub platform_version: String,
    pub python_full_version: StringVersion,
    pub python_version: StringVersion,
    pub sys_platform: String,
}

impl MarkerEnvironment {
    /// Returns of the PEP 440 version typed value of the key in the current environment
    fn get_version(&self, key: &MarkerValueVersion) -> &Version {
        match key {
            MarkerValueVersion::ImplementationVersion => &self.implementation_version.version,
            MarkerValueVersion::PythonFullVersion => &self.python_full_version.version,
            MarkerValueVersion::PythonVersion => &self.python_version.version,
        }
    }

    /// Returns of the stringly typed value of the key in the current environment
    fn get_string(&self, key: &MarkerValueString) -> &str {
        match key {
            MarkerValueString::ImplementationName => &self.implementation_name,
            MarkerValueString::OsName | MarkerValueString::OsNameDeprecated => &self.os_name,
            MarkerValueString::PlatformMachine | MarkerValueString::PlatformMachineDeprecated => {
                &self.platform_machine
            }
            MarkerValueString::PlatformPythonImplementation
            | MarkerValueString::PlatformPythonImplementationDeprecated => {
                &self.platform_python_implementation
            }
            MarkerValueString::PlatformRelease => &self.platform_release,
            MarkerValueString::PlatformSystem => &self.platform_system,
            MarkerValueString::PlatformVersion | MarkerValueString::PlatformVersionDeprecated => {
                &self.platform_version
            }
            MarkerValueString::SysPlatform | MarkerValueString::SysPlatformDeprecated => {
                &self.sys_platform
            }
        }
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
        })
    }

    /// Query the current python interpreter to get the correct marker value
    #[staticmethod]
    fn current(py: Python<'_>) -> PyResult<Self> {
        let os = py.import("os")?;
        let platform = py.import("platform")?;
        let sys = py.import("sys")?;
        let python_version_tuple: (String, String, String) = platform
            .getattr("python_version_tuple")?
            .call0()?
            .extract()?;

        // See pseudocode at
        // https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers
        let name = sys.getattr("implementation")?.getattr("name")?.extract()?;
        let info: &PyAny = sys.getattr("implementation")?.getattr("version")?;
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
        })
    }
}

/// Represents one clause such as `python_version > "3.8"` in the form
/// ```text
/// <a name from the PEP508 list | a string> <an operator> <a name from the PEP508 list | a string>
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MarkerExpression {
    /// A name from the PEP508 list or a string
    pub l_value: MarkerValue,
    /// an operator, such as `>=` or `not in`
    pub operator: MarkerOperator,
    /// A name from the PEP508 list or a string
    pub r_value: MarkerValue,
}

impl MarkerExpression {
    /// Evaluate a <`marker_value`> <`marker_op`> <`marker_value`> expression
    fn evaluate(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) -> bool {
        match &self.l_value {
            // The only sound choice for this is `<version key> <version op> <quoted PEP 440 version>`
            MarkerValue::MarkerEnvVersion(l_key) => {
                let value = &self.r_value;
                let r_vpat = if let MarkerValue::QuotedString(r_string) = &value {
                    match r_string.parse::<VersionPattern>() {
                        Ok(vpat) => vpat,
                        Err(err) => {
                            reporter(MarkerWarningKind::Pep440Error, format!(
                                "Expected PEP 440 version to compare with {}, found {}, evaluating to false: {}",
                                l_key, self.r_value, err
                            ), self);
                            return false;
                        }
                    }
                } else {
                    reporter(MarkerWarningKind::Pep440Error, format!(
                        "Expected double quoted PEP 440 version to compare with {}, found {}, evaluating to false",
                        l_key, self.r_value
                    ), self);
                    return false;
                };

                let operator = match self.operator.to_pep440_operator() {
                    None => {
                        reporter(MarkerWarningKind::Pep440Error, format!(
                            "Expected PEP 440 version operator to compare {} with '{}', found '{}', evaluating to false",
                            l_key, r_vpat.version(), self.operator
                        ), self);
                        return false;
                    }
                    Some(operator) => operator,
                };

                let specifier = match VersionSpecifier::new(operator, r_vpat) {
                    Ok(specifier) => specifier,
                    Err(err) => {
                        reporter(
                            MarkerWarningKind::Pep440Error,
                            format!("Invalid operator/version combination: {err}"),
                            self,
                        );
                        return false;
                    }
                };

                let l_version = env.get_version(l_key);
                specifier.contains(l_version)
            }
            // This is half the same block as above inverted
            MarkerValue::MarkerEnvString(l_key) => {
                let r_string = match &self.r_value {
                    MarkerValue::Extra
                    | MarkerValue::MarkerEnvVersion(_)
                    | MarkerValue::MarkerEnvString(_) => {
                        reporter(MarkerWarningKind::MarkerMarkerComparison, "Comparing two markers with each other doesn't make any sense, evaluating to false".to_string(), self);
                        return false;
                    }
                    MarkerValue::QuotedString(r_string) => r_string,
                };

                let l_string = env.get_string(l_key);
                self.compare_strings(l_string, r_string, reporter)
            }
            // `extra == '...'`
            MarkerValue::Extra => {
                let r_value_string = match &self.r_value {
                    MarkerValue::MarkerEnvVersion(_)
                    | MarkerValue::MarkerEnvString(_)
                    | MarkerValue::Extra => {
                        reporter(MarkerWarningKind::ExtraInvalidComparison, "Comparing extra with something other than a quoted string is wrong, evaluating to false".to_string(), self);
                        return false;
                    }
                    MarkerValue::QuotedString(r_value_string) => r_value_string,
                };
                match ExtraName::from_str(r_value_string) {
                    Ok(r_extra) => extras.contains(&r_extra),
                    Err(err) => {
                        reporter(MarkerWarningKind::ExtraInvalidComparison, format!(
                            "Expected extra name, found '{r_value_string}', evaluating to false: {err}"
                        ), self);
                        false
                    }
                }
            }
            // This is either MarkerEnvVersion, MarkerEnvString or Extra inverted
            MarkerValue::QuotedString(l_string) => {
                match &self.r_value {
                    // The only sound choice for this is `<quoted PEP 440 version> <version op>` <version key>
                    MarkerValue::MarkerEnvVersion(r_key) => {
                        let l_version = match Version::from_str(l_string) {
                            Ok(l_version) => l_version,
                            Err(err) => {
                                reporter(MarkerWarningKind::Pep440Error, format!(
                                    "Expected double quoted PEP 440 version to compare with {}, found {}, evaluating to false: {}",
                                    l_string, self.r_value, err
                                ), self);
                                return false;
                            }
                        };
                        let r_version = env.get_version(r_key);

                        let operator = match self.operator.to_pep440_operator() {
                            None => {
                                reporter(MarkerWarningKind::Pep440Error, format!(
                                    "Expected PEP 440 version operator to compare '{}' with {}, found '{}', evaluating to false",
                                    l_string, r_key, self.operator
                                ), self);
                                return false;
                            }
                            Some(operator) => operator,
                        };

                        let specifier = match VersionSpecifier::new(
                            operator,
                            VersionPattern::verbatim(r_version.clone()),
                        ) {
                            Ok(specifier) => specifier,
                            Err(err) => {
                                reporter(
                                    MarkerWarningKind::Pep440Error,
                                    format!("Invalid operator/version combination: {err}"),
                                    self,
                                );
                                return false;
                            }
                        };

                        specifier.contains(&l_version)
                    }
                    // This is half the same block as above inverted
                    MarkerValue::MarkerEnvString(r_key) => {
                        let r_string = env.get_string(r_key);
                        self.compare_strings(l_string, r_string, reporter)
                    }
                    // `'...' == extra`
                    MarkerValue::Extra => match ExtraName::from_str(l_string) {
                        Ok(l_extra) => self.marker_compare(&l_extra, extras, reporter),
                        Err(err) => {
                            reporter(MarkerWarningKind::ExtraInvalidComparison, format!(
                                    "Expected extra name, found '{l_string}', evaluating to false: {err}"
                                ), self);
                            false
                        }
                    },
                    // `'...' == '...'`, doesn't make much sense
                    MarkerValue::QuotedString(_) => {
                        // Not even pypa/packaging 22.0 supports this
                        // https://github.com/pypa/packaging/issues/632
                        reporter(MarkerWarningKind::StringStringComparison, format!(
                            "Comparing two quoted strings with each other doesn't make sense: {self}, evaluating to false"
                        ), self);
                        false
                    }
                }
            }
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
        match (&self.l_value, &self.operator, &self.r_value) {
            // `extra == '...'`
            (MarkerValue::Extra, MarkerOperator::Equal, MarkerValue::QuotedString(r_string)) => {
                ExtraName::from_str(r_string).is_ok_and(|r_extra| extras.contains(&r_extra))
            }
            // `'...' == extra`
            (MarkerValue::QuotedString(l_string), MarkerOperator::Equal, MarkerValue::Extra) => {
                ExtraName::from_str(l_string).is_ok_and(|l_extra| extras.contains(&l_extra))
            }
            // `extra != '...'`
            (MarkerValue::Extra, MarkerOperator::NotEqual, MarkerValue::QuotedString(r_string)) => {
                ExtraName::from_str(r_string).is_ok_and(|r_extra| !extras.contains(&r_extra))
            }
            // `'...' != extra`
            (MarkerValue::QuotedString(l_string), MarkerOperator::NotEqual, MarkerValue::Extra) => {
                ExtraName::from_str(l_string).is_ok_and(|l_extra| !extras.contains(&l_extra))
            }
            (
                MarkerValue::MarkerEnvVersion(MarkerValueVersion::PythonVersion),
                operator,
                MarkerValue::QuotedString(r_string),
            ) => {
                // ignore all errors block
                (|| {
                    // The right hand side is allowed to contain a star, e.g. `python_version == '3.*'`
                    let r_vpat = r_string.parse::<VersionPattern>().ok()?;
                    let operator = operator.to_pep440_operator()?;
                    // operator and right hand side make the specifier
                    let specifier = VersionSpecifier::new(operator, r_vpat).ok()?;

                    let compatible = python_versions
                        .iter()
                        .any(|l_version| specifier.contains(l_version));
                    Some(compatible)
                })()
                .unwrap_or(true)
            }
            (
                MarkerValue::QuotedString(l_string),
                operator,
                MarkerValue::MarkerEnvVersion(MarkerValueVersion::PythonVersion),
            ) => {
                // ignore all errors block
                (|| {
                    // Not star allowed here, `'3.*' == python_version` is not a valid PEP 440
                    // comparison
                    let l_version = Version::from_str(l_string).ok()?;
                    let operator = operator.to_pep440_operator()?;

                    let compatible = python_versions.iter().any(|r_version| {
                        // operator and right hand side make the specifier and in this case the
                        // right hand is `python_version` so changes every iteration
                        match VersionSpecifier::new(
                            operator,
                            VersionPattern::verbatim(r_version.clone()),
                        ) {
                            Ok(specifier) => specifier.contains(&l_version),
                            Err(_) => true,
                        }
                    });

                    Some(compatible)
                })()
                .unwrap_or(true)
            }
            _ => true,
        }
    }

    /// Compare strings by PEP 508 logic, with warnings
    fn compare_strings(
        &self,
        l_string: &str,
        r_string: &str,
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) -> bool {
        match self.operator {
            MarkerOperator::Equal => l_string == r_string,
            MarkerOperator::NotEqual => l_string != r_string,
            MarkerOperator::GreaterThan => {
                reporter(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                    self,
                );
                l_string > r_string
            }
            MarkerOperator::GreaterEqual => {
                reporter(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                    self,
                );
                l_string >= r_string
            }
            MarkerOperator::LessThan => {
                reporter(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                    self,
                );
                l_string < r_string
            }
            MarkerOperator::LessEqual => {
                reporter(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Comparing {l_string} and {r_string} lexicographically"),
                    self,
                );
                l_string <= r_string
            }
            MarkerOperator::TildeEqual => {
                reporter(
                    MarkerWarningKind::LexicographicComparison,
                    format!("Can't compare {l_string} and {r_string} with `~=`"),
                    self,
                );
                false
            }
            MarkerOperator::In => r_string.contains(l_string),
            MarkerOperator::NotIn => !r_string.contains(l_string),
        }
    }

    // The `marker <op> '...'` comparison
    fn marker_compare(
        &self,
        value: &ExtraName,
        extras: &[ExtraName],
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) -> bool {
        match self.operator {
            MarkerOperator::Equal => extras.contains(value),
            MarkerOperator::NotEqual => !extras.contains(value),
            _ => {
                reporter(MarkerWarningKind::ExtraInvalidComparison, "Comparing extra with something other than equal (`==`) or unequal (`!=`) is wrong, evaluating to false".to_string(), self);
                false
            }
        }
    }
}

impl FromStr for MarkerExpression {
    type Err = Pep508Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = Cursor::new(s);
        let expression = parse_marker_key_op_value(&mut chars)?;
        chars.eat_whitespace();
        if let Some((pos, unexpected)) = chars.next() {
            return Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Unexpected character '{unexpected}', expected end of input"
                )),
                start: pos,
                len: chars.chars.clone().count(),
                input: chars.to_string(),
            });
        }
        Ok(expression)
    }
}

impl Display for MarkerExpression {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.l_value, self.operator, self.r_value)
    }
}

/// Represents one of the nested marker expressions with and/or/parentheses
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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

impl FromStr for MarkerTree {
    type Err = Pep508Error;

    fn from_str(markers: &str) -> Result<Self, Self::Err> {
        parse_markers(markers)
    }
}

impl MarkerTree {
    /// Does this marker apply in the given environment?
    pub fn evaluate(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        let mut reporter = |_kind, message, _marker_expression: &MarkerExpression| {
            warn!("{}", message);
        };
        self.report_deprecated_options(&mut reporter);
        match self {
            MarkerTree::Expression(expression) => expression.evaluate(env, extras, &mut reporter),
            MarkerTree::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_reporter_impl(env, extras, &mut reporter)),
            MarkerTree::Or(expressions) => expressions
                .iter()
                .any(|x| x.evaluate_reporter_impl(env, extras, &mut reporter)),
        }
    }

    /// Same as [`Self::evaluate`], but instead of using logging to warn, you can pass your own
    /// handler for warnings
    pub fn evaluate_reporter(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) -> bool {
        self.report_deprecated_options(reporter);
        self.evaluate_reporter_impl(env, extras, reporter)
    }

    fn evaluate_reporter_impl(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) -> bool {
        match self {
            MarkerTree::Expression(expression) => expression.evaluate(env, extras, reporter),
            MarkerTree::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_reporter_impl(env, extras, reporter)),
            MarkerTree::Or(expressions) => expressions
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
            MarkerTree::Expression(expression) => {
                expression.evaluate_extras_and_python_version(extras, python_versions)
            }
            MarkerTree::And(expressions) => expressions
                .iter()
                .all(|x| x.evaluate_extras_and_python_version(extras, python_versions)),
            MarkerTree::Or(expressions) => expressions
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
    ) -> (bool, Vec<(MarkerWarningKind, String, String)>) {
        let mut warnings = Vec::new();
        let mut reporter = |kind, warning, marker: &MarkerExpression| {
            warnings.push((kind, warning, marker.to_string()));
        };
        self.report_deprecated_options(&mut reporter);
        let result = self.evaluate_reporter_impl(env, extras, &mut reporter);
        (result, warnings)
    }

    /// Report the deprecated marker from <https://peps.python.org/pep-0345/#environment-markers>
    fn report_deprecated_options(
        &self,
        reporter: &mut impl FnMut(MarkerWarningKind, String, &MarkerExpression),
    ) {
        match self {
            MarkerTree::Expression(expression) => {
                for value in [&expression.l_value, &expression.r_value] {
                    match value {
                        MarkerValue::MarkerEnvString(MarkerValueString::OsNameDeprecated) => {
                            reporter(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "os.name is deprecated in favor of os_name".to_string(),
                                expression,
                            );
                        }
                        MarkerValue::MarkerEnvString(
                            MarkerValueString::PlatformMachineDeprecated,
                        ) => {
                            reporter(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "platform.machine is deprecated in favor of platform_machine"
                                    .to_string(),
                                expression,
                            );
                        }
                        MarkerValue::MarkerEnvString(
                            MarkerValueString::PlatformPythonImplementationDeprecated,
                        ) => {
                            reporter(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "platform.python_implementation is deprecated in favor of platform_python_implementation".to_string(),
                                expression,
                            );
                        }
                        MarkerValue::MarkerEnvString(
                            MarkerValueString::PlatformVersionDeprecated,
                        ) => {
                            reporter(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "platform.version is deprecated in favor of platform_version"
                                    .to_string(),
                                expression,
                            );
                        }
                        MarkerValue::MarkerEnvString(MarkerValueString::SysPlatformDeprecated) => {
                            reporter(
                                MarkerWarningKind::DeprecatedMarkerName,
                                "sys.platform  is deprecated in favor of sys_platform".to_string(),
                                expression,
                            );
                        }
                        _ => {}
                    }
                }
            }
            MarkerTree::And(expressions) => {
                for expression in expressions {
                    expression.report_deprecated_options(reporter);
                }
            }
            MarkerTree::Or(expressions) => {
                for expression in expressions {
                    expression.report_deprecated_options(reporter);
                }
            }
        }
    }
}

impl Display for MarkerTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let format_inner = |expression: &MarkerTree| {
            if matches!(expression, MarkerTree::Expression(_)) {
                format!("{expression}")
            } else {
                format!("({expression})")
            }
        };
        match self {
            MarkerTree::Expression(expression) => write!(f, "{expression}"),
            MarkerTree::And(and_list) => f.write_str(
                &and_list
                    .iter()
                    .map(format_inner)
                    .collect::<Vec<String>>()
                    .join(" and "),
            ),
            MarkerTree::Or(or_list) => f.write_str(
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
fn parse_marker_operator(cursor: &mut Cursor) -> Result<MarkerOperator, Pep508Error> {
    let (start, len) =
        cursor.take_while(|char| !char.is_whitespace() && char != '\'' && char != '"');
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
fn parse_marker_value(cursor: &mut Cursor) -> Result<MarkerValue, Pep508Error> {
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
fn parse_marker_key_op_value(cursor: &mut Cursor) -> Result<MarkerExpression, Pep508Error> {
    cursor.eat_whitespace();
    let lvalue = parse_marker_value(cursor)?;
    cursor.eat_whitespace();
    // "not in" and "in" must be preceded by whitespace. We must already have matched a whitespace
    // when we're here because other `parse_marker_key` would have pulled the characters in and
    // errored
    let operator = parse_marker_operator(cursor)?;
    cursor.eat_whitespace();
    let rvalue = parse_marker_value(cursor)?;
    Ok(MarkerExpression {
        l_value: lvalue,
        operator,
        r_value: rvalue,
    })
}

/// ```text
/// marker_expr   = marker_var:l marker_op:o marker_var:r -> (o, l, r)
///               | wsp* '(' marker:m wsp* ')' -> m
/// ```
fn parse_marker_expr(cursor: &mut Cursor) -> Result<MarkerTree, Pep508Error> {
    cursor.eat_whitespace();
    if let Some(start_pos) = cursor.eat_char('(') {
        let marker = parse_marker_or(cursor)?;
        cursor.next_expect_char(')', start_pos)?;
        Ok(marker)
    } else {
        Ok(MarkerTree::Expression(parse_marker_key_op_value(cursor)?))
    }
}

/// ```text
/// marker_and    = marker_expr:l wsp* 'and' marker_expr:r -> ('and', l, r)
///               | marker_expr:m -> m
/// ```
fn parse_marker_and(cursor: &mut Cursor) -> Result<MarkerTree, Pep508Error> {
    parse_marker_op(cursor, "and", MarkerTree::And, parse_marker_expr)
}

/// ```text
/// marker_or     = marker_and:l wsp* 'or' marker_and:r -> ('or', l, r)
///                   | marker_and:m -> m
/// ```
fn parse_marker_or(cursor: &mut Cursor) -> Result<MarkerTree, Pep508Error> {
    parse_marker_op(cursor, "or", MarkerTree::Or, parse_marker_and)
}

/// Parses both `marker_and` and `marker_or`
fn parse_marker_op(
    cursor: &mut Cursor,
    op: &str,
    op_constructor: fn(Vec<MarkerTree>) -> MarkerTree,
    parse_inner: fn(&mut Cursor) -> Result<MarkerTree, Pep508Error>,
) -> Result<MarkerTree, Pep508Error> {
    // marker_and or marker_expr
    let first_element = parse_inner(cursor)?;
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
                let expression = parse_inner(cursor)?;
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
/// marker        = marker_or
/// ```
pub(crate) fn parse_markers_impl(cursor: &mut Cursor) -> Result<MarkerTree, Pep508Error> {
    let marker = parse_marker_or(cursor)?;
    cursor.eat_whitespace();
    if let Some((pos, unexpected)) = cursor.next() {
        // If we're here, both parse_marker_or and parse_marker_and returned because the next
        // character was neither "and" nor "or"
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(format!(
                "Unexpected character '{unexpected}', expected 'and', 'or' or end of input"
            )),
            start: pos,
            len: cursor.chars.clone().count(),
            input: cursor.to_string(),
        });
    };
    Ok(marker)
}

/// Parses markers such as `python_version < '3.8'` or
/// `python_version == "3.10" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))`
fn parse_markers(markers: &str) -> Result<MarkerTree, Pep508Error> {
    let mut chars = Cursor::new(markers);
    parse_markers_impl(&mut chars)
}

#[cfg(test)]
mod test {
    use crate::marker::{MarkerEnvironment, StringVersion};
    use crate::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValue, MarkerValueString};
    use indoc::indoc;
    use log::Level;
    use std::str::FromStr;

    fn assert_err(input: &str, error: &str) {
        assert_eq!(MarkerTree::from_str(input).unwrap_err().to_string(), error);
    }

    fn env37() -> MarkerEnvironment {
        let v37 = StringVersion::from_str("3.7").unwrap();

        MarkerEnvironment {
            implementation_name: String::new(),
            implementation_version: v37.clone(),
            os_name: "linux".to_string(),
            platform_machine: String::new(),
            platform_python_implementation: String::new(),
            platform_release: String::new(),
            platform_system: String::new(),
            platform_version: String::new(),
            python_full_version: v37.clone(),
            python_version: v37,
            sys_platform: "linux".to_string(),
        }
    }

    /// Copied from <https://github.com/pypa/packaging/blob/85ff971a250dc01db188ef9775499c15553a8c95/tests/test_markers.py#L175-L221>
    #[test]
    fn test_marker_equivalence() {
        let values = [
            (r#"python_version == '2.7'"#, r#"python_version == "2.7""#),
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
        let v27 = StringVersion::from_str("2.7").unwrap();
        let env27 = MarkerEnvironment {
            implementation_name: String::new(),
            implementation_version: v27.clone(),
            os_name: "linux".to_string(),
            platform_machine: String::new(),
            platform_python_implementation: String::new(),
            platform_release: String::new(),
            platform_system: String::new(),
            platform_version: String::new(),
            python_full_version: v27.clone(),
            python_version: v27,
            sys_platform: "linux".to_string(),
        };
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
    fn warnings() {
        let env37 = env37();
        testing_logger::setup();
        let compare_keys = MarkerTree::from_str("platform_version == sys_platform").unwrap();
        compare_keys.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Comparing two markers with each other doesn't make any sense, evaluating to false"
            );
            assert_eq!(captured_logs[0].level, Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let non_pep440 = MarkerTree::from_str("python_version >= '3.9.'").unwrap();
        non_pep440.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Expected PEP 440 version to compare with python_version, found '3.9.', \
                 evaluating to false: after parsing 3.9, found \".\" after it, \
                 which is not part of a valid version"
            );
            assert_eq!(captured_logs[0].level, Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let string_string = MarkerTree::from_str("'b' >= 'a'").unwrap();
        string_string.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            assert_eq!(
                captured_logs[0].body,
                "Comparing two quoted strings with each other doesn't make sense: 'b' >= 'a', evaluating to false"
            );
            assert_eq!(captured_logs[0].level, Level::Warn);
            assert_eq!(captured_logs.len(), 1);
        });
        let string_string = MarkerTree::from_str(r#"os.name == 'posix' and platform.machine == 'x86_64' and platform.python_implementation == 'CPython' and 'Ubuntu' in platform.version and sys.platform == 'linux'"#).unwrap();
        string_string.evaluate(&env37, &[]);
        testing_logger::validate(|captured_logs| {
            let messages: Vec<_> = captured_logs
                .iter()
                .map(|message| {
                    assert_eq!(message.level, Level::Warn);
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
        assert_err(
            r#"python_version == "3.8".* and python_version >= "3.8""#,
            indoc! {r#"
                Unexpected character '.', expected 'and', 'or' or end of input
                python_version == "3.8".* and python_version >= "3.8"
                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"#
            },
        );
        assert_err(
            r#"python_version == "3.8".*"#,
            indoc! {r#"
                Unexpected character '.', expected 'and', 'or' or end of input
                python_version == "3.8".*
                                       ^"#
            },
        );
    }

    #[test]
    fn test_marker_expression() {
        assert_eq!(
            MarkerExpression::from_str(r#"os_name == "nt""#).unwrap(),
            MarkerExpression {
                l_value: MarkerValue::MarkerEnvString(MarkerValueString::OsName),
                operator: MarkerOperator::Equal,
                r_value: MarkerValue::QuotedString("nt".to_string()),
            }
        );
    }

    #[test]
    fn test_marker_expression_to_long() {
        assert_eq!(
            MarkerExpression::from_str(r#"os_name == "nt" and python_version >= "3.8""#)
                .unwrap_err()
                .to_string(),
            indoc! {r#"
                Unexpected character 'a', expected end of input
                os_name == "nt" and python_version >= "3.8"
                                ^^^^^^^^^^^^^^^^^^^^^^^^^^"#
            },
        );
    }

    #[cfg(feature = "serde")]
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
}
