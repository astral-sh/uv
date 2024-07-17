use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

#[cfg(feature = "pyo3")]
use pyo3::{basic::CompareOp, pyclass, pymethods};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use pep440_rs::{Version, VersionParseError, VersionPattern, VersionSpecifier};
use uv_normalize::ExtraName;

use crate::cursor::Cursor;
use crate::marker::parse;
use crate::{
    MarkerEnvironment, Pep508Error, Pep508ErrorSource, Pep508Url, Reporter, TracingReporter,
};

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
    /// The inverse of the `in` operator.
    ///
    /// This is not a valid operator when parsing but is used for normalizing
    /// marker trees.
    Contains,
    /// The inverse of the `not in` operator.
    ///
    /// This is not a valid operator when parsing but is used for normalizing
    /// marker trees.
    NotContains,
}

impl MarkerOperator {
    /// Compare two versions, returning `None` for `in` and `not in`.
    pub(crate) fn to_pep440_operator(self) -> Option<pep440_rs::Operator> {
        match self {
            Self::Equal => Some(pep440_rs::Operator::Equal),
            Self::NotEqual => Some(pep440_rs::Operator::NotEqual),
            Self::GreaterThan => Some(pep440_rs::Operator::GreaterThan),
            Self::GreaterEqual => Some(pep440_rs::Operator::GreaterThanEqual),
            Self::LessThan => Some(pep440_rs::Operator::LessThan),
            Self::LessEqual => Some(pep440_rs::Operator::LessThanEqual),
            Self::TildeEqual => Some(pep440_rs::Operator::TildeEqual),
            _ => None,
        }
    }

    /// Negates this marker operator.
    ///
    /// If a negation doesn't exist, which is only the case for ~=, then this
    /// returns `None`.
    fn negate(self) -> Option<MarkerOperator> {
        Some(match self {
            Self::Equal => Self::NotEqual,
            Self::NotEqual => Self::Equal,
            Self::TildeEqual => return None,
            Self::LessThan => Self::GreaterEqual,
            Self::LessEqual => Self::GreaterThan,
            Self::GreaterThan => Self::LessEqual,
            Self::GreaterEqual => Self::LessThan,
            Self::In => Self::NotIn,
            Self::NotIn => Self::In,
            Self::Contains => Self::NotContains,
            Self::NotContains => Self::Contains,
        })
    }

    /// Inverts this marker operator.
    pub(crate) fn invert(self) -> MarkerOperator {
        match self {
            Self::LessThan => Self::GreaterThan,
            Self::LessEqual => Self::GreaterEqual,
            Self::GreaterThan => Self::LessThan,
            Self::GreaterEqual => Self::LessEqual,
            Self::Equal => Self::Equal,
            Self::NotEqual => Self::NotEqual,
            Self::TildeEqual => Self::TildeEqual,
            Self::In => Self::Contains,
            Self::NotIn => Self::NotContains,
            Self::Contains => Self::In,
            Self::NotContains => Self::NotIn,
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
            Self::In | Self::Contains => "in",
            Self::NotIn | Self::NotContains => "not in",
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

/// Represents one clause such as `python_version > "3.8"`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum MarkerExpression {
    /// A version expression, e.g. `<version key> <version op> <quoted PEP 440 version>`.
    ///
    /// Inverted version expressions, such as `<version> <version op> <version key>`, are also
    /// normalized to this form.
    Version {
        key: MarkerValueVersion,
        specifier: VersionSpecifier,
    },
    /// An string marker comparison, e.g. `sys_platform == '...'`.
    ///
    /// Inverted string expressions, e.g `'...' == sys_platform`, are also normalized to this form.
    String {
        key: MarkerValueString,
        operator: MarkerOperator,
        value: String,
    },
    /// `extra <extra op> '...'` or `'...' <extra op> extra`.
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
    pub(crate) fn from_marker_operator(operator: MarkerOperator) -> Option<ExtraOperator> {
        match operator {
            MarkerOperator::Equal => Some(ExtraOperator::Equal),
            MarkerOperator::NotEqual => Some(ExtraOperator::NotEqual),
            _ => None,
        }
    }

    fn negate(&self) -> ExtraOperator {
        match *self {
            ExtraOperator::Equal => ExtraOperator::NotEqual,
            ExtraOperator::NotEqual => ExtraOperator::Equal,
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
        let expression = parse::parse_marker_key_op_value(&mut chars, reporter)?;
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

    /// Negates this marker expression.
    ///
    /// In most cases, this returns a `MarkerTree::Expression`, but in some
    /// cases it can be more complicated than that. For example, the negation
    /// of a compatible version constraint is a disjunction.
    ///
    /// Additionally, in some cases, the negation reflects the "spirit" of what
    /// the marker expression is. For example, the negation of an "arbitrary"
    /// expression will still result in an expression that is always false.
    fn negate(&self) -> MarkerTree {
        match *self {
            MarkerExpression::Version {
                ref key,
                ref specifier,
            } => {
                let (op, version) = (specifier.operator(), specifier.version().clone());
                match op.negate() {
                    None => negate_compatible_version(key.clone(), version),
                    Some(op) => {
                        // OK because this can only fail with either local versions,
                        // which we avoid by construction, or if the op is ~=, which
                        // is never the result of negating an op.
                        let specifier =
                            VersionSpecifier::from_version(op, version.without_local()).unwrap();
                        let expr = MarkerExpression::Version {
                            key: key.clone(),
                            specifier,
                        };
                        MarkerTree::Expression(expr)
                    }
                }
            }
            MarkerExpression::String {
                ref key,
                ref operator,
                ref value,
            } => {
                let expr = MarkerExpression::String {
                    key: key.clone(),
                    // negating ~= doesn't make sense in this context, but
                    // I believe it is technically allowed, so we just leave
                    // it as-is.
                    operator: operator.negate().unwrap_or(MarkerOperator::TildeEqual),
                    value: value.clone(),
                };
                MarkerTree::Expression(expr)
            }
            MarkerExpression::Extra {
                ref operator,
                ref name,
            } => {
                let expr = MarkerExpression::Extra {
                    operator: operator.negate(),
                    name: name.clone(),
                };
                MarkerTree::Expression(expr)
            }
            // "arbitrary" expressions always return false, and while the
            // negation logically implies they should always return true, we do
            // not do that here because it violates the spirit of a meaningly
            // or "arbitrary" marker. We flip the operator but do nothing else.
            MarkerExpression::Arbitrary {
                ref l_value,
                ref operator,
                ref r_value,
            } => {
                let expr = MarkerExpression::Arbitrary {
                    l_value: l_value.clone(),
                    // negating ~= doesn't make sense in this context, but
                    // I believe it is technically allowed, so we just leave
                    // it as-is.
                    operator: operator.negate().unwrap_or(MarkerOperator::TildeEqual),
                    r_value: r_value.clone(),
                };
                MarkerTree::Expression(expr)
            }
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
            MarkerOperator::Contains => l_string.contains(r_string),
            MarkerOperator::NotContains => !l_string.contains(r_string),
        }
    }

    /// Creates an instance of [`MarkerExpression::Arbitrary`] with the given values.
    pub(crate) fn arbitrary(
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
                let (op, version) = (specifier.operator(), specifier.version());
                if op == &pep440_rs::Operator::EqualStar || op == &pep440_rs::Operator::NotEqualStar
                {
                    return write!(f, "{key} {op} '{version}.*'");
                }
                write!(f, "{key} {op} '{version}'")
            }
            MarkerExpression::String {
                key,
                operator,
                value,
            } => {
                if matches!(
                    operator,
                    MarkerOperator::Contains | MarkerOperator::NotContains
                ) {
                    return write!(f, "'{value}' {} {key}", operator.invert());
                }

                write!(f, "{key} {operator} '{value}'")
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
        parse::parse_markers(markers, &mut TracingReporter)
    }
}

impl MarkerTree {
    /// Like [`FromStr::from_str`], but the caller chooses the return type generic.
    pub fn parse_str<T: Pep508Url>(markers: &str) -> Result<Self, Pep508Error<T>> {
        parse::parse_markers(markers, &mut TracingReporter)
    }

    /// Parse a [`MarkerTree`] from a string with the given reporter.
    pub fn parse_reporter(
        markers: &str,
        reporter: &mut impl Reporter,
    ) -> Result<Self, Pep508Error> {
        parse::parse_markers(markers, reporter)
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

    /// Returns a new marker tree that is the negation of this one.
    #[must_use]
    pub fn negate(&self) -> MarkerTree {
        match *self {
            MarkerTree::Expression(ref expr) => expr.negate(),
            MarkerTree::And(ref trees) => {
                let mut negated = MarkerTree::Or(Vec::with_capacity(trees.len()));
                for tree in trees {
                    negated.or(tree.negate());
                }
                negated
            }
            MarkerTree::Or(ref trees) => {
                let mut negated = MarkerTree::And(Vec::with_capacity(trees.len()));
                for tree in trees {
                    negated.and(tree.negate());
                }
                negated
            }
        }
    }

    /// Combine this marker tree with the one given via a conjunction.
    ///
    /// This does some shallow flattening. That is, if `self` is a conjunction
    /// already, then `tree` is added to it instead of creating a new
    /// conjunction.
    pub fn and(&mut self, tree: MarkerTree) {
        if self == &tree {
            return;
        }
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
            if exprs.len() == 1 {
                *self = exprs.pop().unwrap();
            }
        }
    }

    /// Combine this marker tree with the one given via a disjunction.
    ///
    /// This does some shallow flattening. That is, if `self` is a disjunction
    /// already, then `tree` is added to it instead of creating a new
    /// disjunction.
    pub fn or(&mut self, tree: MarkerTree) {
        if self == &tree {
            return;
        }
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
            if exprs.len() == 1 {
                *self = exprs.pop().unwrap();
            }
        }
    }

    /// Find a top level `extra == "..."` expression.
    ///
    /// ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part of the
    /// main conjunction.
    pub fn top_level_extra(&self) -> Option<&MarkerExpression> {
        match &self {
            MarkerTree::Expression(extra_expression @ MarkerExpression::Extra { .. }) => {
                Some(extra_expression)
            }
            MarkerTree::And(and) => and.iter().find_map(|marker| {
                if let MarkerTree::Expression(extra_expression @ MarkerExpression::Extra { .. }) =
                    marker
                {
                    Some(extra_expression)
                } else {
                    None
                }
            }),
            MarkerTree::Expression(_) | MarkerTree::Or(_) => None,
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

/// Negates a compatible version marker expression, from its component parts.
///
/// Here, we consider `key ~= V.N` to be equivalent to
/// `key >= V.N and key == V.*`. So the negation returned is
/// `key < V.N or key != V.*`.
fn negate_compatible_version(key: MarkerValueVersion, version: Version) -> MarkerTree {
    assert!(
        version.release().len() > 1,
        "~= requires more than 1 release version number"
    );
    // I believe we're already guaranteed that this is true,
    // because we're only here if this version was combined
    // with ~=, which cannot be used with local versions anyway.
    // But this ensures correctness and should be pretty cheap.
    let version = version.without_local();
    let pattern = VersionPattern::wildcard(Version::new(
        &version.release()[..version.release().len() - 1],
    ));
    // OK because this can only fail for local versions or when using
    // ~=, but neither is the case here.
    let disjunct1 = VersionSpecifier::from_version(pep440_rs::Operator::LessThan, version).unwrap();
    // And this is OK because it only fails if the above would fail
    // (which we know it doesn't) or if the operator is not compatible
    // with wildcards, but != is.
    let disjunct2 = VersionSpecifier::from_pattern(pep440_rs::Operator::NotEqual, pattern).unwrap();
    MarkerTree::Or(vec![
        MarkerTree::Expression(MarkerExpression::Version {
            key: key.clone(),
            specifier: disjunct1,
        }),
        MarkerTree::Expression(MarkerExpression::Version {
            key,
            specifier: disjunct2,
        }),
    ])
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
    fn test_marker_negation() {
        let neg = |marker_string: &str| -> String {
            let tree: MarkerTree = marker_string.parse().unwrap();
            tree.negate().to_string()
        };

        assert_eq!(neg("python_version > '3.6'"), "python_version <= '3.6'");
        assert_eq!(neg("'3.6' < python_version"), "python_version <= '3.6'");

        assert_eq!(
            neg("python_version == '3.6.*'"),
            "python_version != '3.6.*'"
        );
        assert_eq!(
            neg("python_version != '3.6.*'"),
            "python_version == '3.6.*'"
        );

        assert_eq!(
            neg("python_version ~= '3.6'"),
            "python_version < '3.6' or python_version != '3.*'"
        );
        assert_eq!(
            neg("'3.6' ~= python_version"),
            "python_version < '3.6' or python_version != '3.*'"
        );
        assert_eq!(
            neg("python_version ~= '3.6.2'"),
            "python_version < '3.6.2' or python_version != '3.6.*'"
        );

        assert_eq!(neg("sys_platform == 'linux'"), "sys_platform != 'linux'");
        assert_eq!(neg("'linux' == sys_platform"), "sys_platform != 'linux'");

        // ~= is nonsense on string markers. Evaluation always returns false
        // in this case, so technically negation would be an expression that
        // always returns true. But, as we do with "arbitrary" markers, we
        // don't let the negation of nonsense become sensible.
        assert_eq!(neg("sys_platform ~= 'linux'"), "sys_platform ~= 'linux'");

        // As above, arbitrary exprs remain arbitrary.
        assert_eq!(neg("'foo' == 'bar'"), "'foo' != 'bar'");

        // Conjunctions
        assert_eq!(
            neg("os_name == 'bar' and os_name == 'foo'"),
            "os_name != 'bar' or os_name != 'foo'"
        );
        // Disjunctions
        assert_eq!(
            neg("os_name == 'bar' or os_name == 'foo'"),
            "os_name != 'bar' and os_name != 'foo'"
        );

        // Always true negates to always false!
        assert_eq!(
            neg("python_version >= '3.6' or python_version < '3.6'"),
            "python_version < '3.6' and python_version >= '3.6'"
        );
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

        let (result, warnings) = MarkerTree::from_str("'3.*' == python_version")
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
                MarkerTree::Expression(MarkerExpression::String {
                    value: "nt".to_string(),
                    operator: MarkerOperator::Contains,
                    key: MarkerValueString::OsName,
                }),
                MarkerTree::Expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonVersion,
                    specifier: VersionSpecifier::from_pattern(
                        pep440_rs::Operator::LessThanEqual,
                        "3.7".parse().unwrap()
                    )
                    .unwrap()
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
