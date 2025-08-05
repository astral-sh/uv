use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{self, Display, Formatter};
use std::ops::{Bound, Deref};
use std::str::FromStr;

use arcstr::ArcStr;
use itertools::Itertools;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use version_ranges::Ranges;

use uv_normalize::{ExtraName, GroupName};
use uv_pep440::{Version, VersionParseError, VersionSpecifier};

use super::algebra::{Edges, INTERNER, NodeId, Variable};
use super::simplify;
use crate::cursor::Cursor;
use crate::marker::lowering::{
    CanonicalMarkerListPair, CanonicalMarkerValueString, CanonicalMarkerValueVersion,
};
use crate::marker::parse;
use crate::{
    CanonicalMarkerValueExtra, MarkerEnvironment, Pep508Error, Pep508ErrorSource, Pep508Url,
    Reporter, TracingReporter,
};

/// Ways in which marker evaluation can fail
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerWarningKind {
    /// Using an old name from PEP 345 instead of the modern equivalent
    /// <https://peps.python.org/pep-0345/#environment-markers>
    DeprecatedMarkerName,
    /// Doing an operation other than `==` and `!=` on a quoted string with `extra`, such as
    /// `extra > "perf"` or `extra == os_name`
    ExtraInvalidComparison,
    /// Doing an operation other than `in` and `not in` on a quoted string with `extra`, such as
    /// `extras > "perf"` or `extras == os_name`
    ExtrasInvalidComparison,
    /// Doing an operation other than `in` and `not in` on a quoted string with `dependency_groups`,
    /// such as `dependency_groups > "perf"` or `dependency_groups == os_name`
    DependencyGroupsInvalidComparison,
    /// Comparing a string valued marker and a string lexicographically, such as `"3.9" > "3.10"`
    LexicographicComparison,
    /// Comparing two markers, such as `os_name != sys_implementation`
    MarkerMarkerComparison,
    /// Failed to parse a PEP 440 version or version specifier, e.g. `>=1<2`
    Pep440Error,
    /// Comparing two strings, such as `"3.9" > "3.10"`
    StringStringComparison,
}

/// Those environment markers with a PEP 440 version as value such as `python_version`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImplementationVersion => f.write_str("implementation_version"),
            Self::PythonFullVersion => f.write_str("python_full_version"),
            Self::PythonVersion => f.write_str("python_version"),
        }
    }
}

/// Those environment markers with an arbitrary string as value such as `sys_platform`
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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

/// Those markers with exclusively `in` and `not in` operators.
///
/// Contains PEP 751 lockfile markers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerValueList {
    /// `extras`. This one is special because it's a list, and user-provided
    Extras,
    /// `dependency_groups`. This one is special because it's a list, and user-provided
    DependencyGroups,
}

impl Display for MarkerValueList {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extras => f.write_str("extras"),
            Self::DependencyGroups => f.write_str("dependency_groups"),
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
    /// Those markers with exclusively `in` and `not in` operators
    MarkerEnvList(MarkerValueList),
    /// `extra`. This one is special because it's a list, and user-provided
    Extra,
    /// Not a constant, but a user given quoted string with a value inside such as '3.8' or "windows"
    QuotedString(ArcStr),
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
            "extras" => Self::MarkerEnvList(MarkerValueList::Extras),
            "dependency_groups" => Self::MarkerEnvList(MarkerValueList::DependencyGroups),
            "extra" => Self::Extra,
            _ => return Err(format!("Invalid key: {s}")),
        };
        Ok(value)
    }
}

impl Display for MarkerValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkerEnvVersion(marker_value_version) => marker_value_version.fmt(f),
            Self::MarkerEnvString(marker_value_string) => marker_value_string.fmt(f),
            Self::MarkerEnvList(marker_value_contains) => marker_value_contains.fmt(f),
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
    pub(crate) fn to_pep440_operator(self) -> Option<uv_pep440::Operator> {
        match self {
            Self::Equal => Some(uv_pep440::Operator::Equal),
            Self::NotEqual => Some(uv_pep440::Operator::NotEqual),
            Self::GreaterThan => Some(uv_pep440::Operator::GreaterThan),
            Self::GreaterEqual => Some(uv_pep440::Operator::GreaterThanEqual),
            Self::LessThan => Some(uv_pep440::Operator::LessThan),
            Self::LessEqual => Some(uv_pep440::Operator::LessThanEqual),
            Self::TildeEqual => Some(uv_pep440::Operator::TildeEqual),
            _ => None,
        }
    }

    /// Inverts this marker operator.
    pub(crate) fn invert(self) -> Self {
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

    /// Negates this marker operator.
    ///
    /// If a negation doesn't exist, which is only the case for ~=, then this
    /// returns `None`.
    pub(crate) fn negate(self) -> Option<Self> {
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

    /// Returns the marker operator and value whose union represents the given range.
    pub fn from_bounds(
        bounds: (&Bound<ArcStr>, &Bound<ArcStr>),
    ) -> impl Iterator<Item = (Self, ArcStr)> {
        let (b1, b2) = match bounds {
            (Bound::Included(v1), Bound::Included(v2)) if v1 == v2 => {
                (Some((Self::Equal, v1.clone())), None)
            }
            (Bound::Excluded(v1), Bound::Excluded(v2)) if v1 == v2 => {
                (Some((Self::NotEqual, v1.clone())), None)
            }
            (lower, upper) => (Self::from_lower_bound(lower), Self::from_upper_bound(upper)),
        };

        b1.into_iter().chain(b2)
    }

    /// Returns a value specifier representing the given lower bound.
    pub fn from_lower_bound(bound: &Bound<ArcStr>) -> Option<(Self, ArcStr)> {
        match bound {
            Bound::Included(value) => Some((Self::GreaterEqual, value.clone())),
            Bound::Excluded(value) => Some((Self::GreaterThan, value.clone())),
            Bound::Unbounded => None,
        }
    }

    /// Returns a value specifier representing the given upper bound.
    pub fn from_upper_bound(bound: &Bound<ArcStr>) -> Option<(Self, ArcStr)> {
        match bound {
            Bound::Included(value) => Some((Self::LessEqual, value.clone())),
            Bound::Excluded(value) => Some((Self::LessThan, value.clone())),
            Bound::Unbounded => None,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
pub struct StringVersion {
    /// Original unchanged string
    pub string: String,
    /// Parsed version
    pub version: Version,
}

impl From<Version> for StringVersion {
    fn from(version: Version) -> Self {
        Self {
            string: version.to_string(),
            version,
        }
    }
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = StringVersion;

            fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                StringVersion::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Deref for StringVersion {
    type Target = Version;

    fn deref(&self) -> &Self::Target {
        &self.version
    }
}

/// The [`ExtraName`] value used in `extra` and `extras` markers.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MarkerValueExtra {
    /// A valid [`ExtraName`].
    Extra(ExtraName),
    /// An invalid name, preserved as an arbitrary string.
    Arbitrary(String),
}

impl MarkerValueExtra {
    /// Returns the [`ExtraName`] for this value, if it is a valid extra.
    pub fn as_extra(&self) -> Option<&ExtraName> {
        match self {
            Self::Extra(extra) => Some(extra),
            Self::Arbitrary(_) => None,
        }
    }

    /// Convert the [`MarkerValueExtra`] to an [`ExtraName`], if possible.
    fn into_extra(self) -> Option<ExtraName> {
        match self {
            Self::Extra(extra) => Some(extra),
            Self::Arbitrary(_) => None,
        }
    }
}

impl Display for MarkerValueExtra {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extra(extra) => extra.fmt(f),
            Self::Arbitrary(string) => string.fmt(f),
        }
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
    /// A version in list expression, e.g. `<version key> in <quoted list of PEP 440 versions>`.
    ///
    /// A special case of [`MarkerExpression::String`] with the [`MarkerOperator::In`] operator for
    /// [`MarkerValueVersion`] values.
    ///
    /// See [`parse::parse_version_in_expr`] for details on the supported syntax.
    ///
    /// Negated expressions, using "not in" are represented using `negated = true`.
    VersionIn {
        key: MarkerValueVersion,
        versions: Vec<Version>,
        operator: ContainerOperator,
    },
    /// An string marker comparison, e.g. `sys_platform == '...'`.
    ///
    /// Inverted string expressions, e.g `'...' == sys_platform`, are also normalized to this form.
    String {
        key: MarkerValueString,
        operator: MarkerOperator,
        value: ArcStr,
    },
    /// `'...' in <key>`, a PEP 751 expression.
    List {
        pair: CanonicalMarkerListPair,
        operator: ContainerOperator,
    },
    /// `extra <extra op> '...'` or `'...' <extra op> extra`.
    Extra {
        name: MarkerValueExtra,
        operator: ExtraOperator,
    },
}

/// The kind of a [`MarkerExpression`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) enum MarkerExpressionKind {
    /// A version expression, e.g. `<version key> <version op> <quoted PEP 440 version>`.
    Version(MarkerValueVersion),
    /// A version `in` expression, e.g. `<version key> in <quoted list of PEP 440 versions>`.
    VersionIn(MarkerValueVersion),
    /// A string marker comparison, e.g. `sys_platform == '...'`.
    String(MarkerValueString),
    /// A list `in` or `not in` expression, e.g. `'...' in dependency_groups`.
    List(MarkerValueList),
    /// An extra expression, e.g. `extra == '...'`.
    Extra,
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
    /// Creates a [`ExtraOperator`] from an equivalent [`MarkerOperator`].
    ///
    /// Returns `None` if the operator is not supported for extras.
    pub(crate) fn from_marker_operator(operator: MarkerOperator) -> Option<Self> {
        match operator {
            MarkerOperator::Equal => Some(Self::Equal),
            MarkerOperator::NotEqual => Some(Self::NotEqual),
            _ => None,
        }
    }

    /// Negates this operator.
    pub(crate) fn negate(&self) -> Self {
        match self {
            Self::Equal => Self::NotEqual,
            Self::NotEqual => Self::Equal,
        }
    }
}

impl Display for ExtraOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
        })
    }
}

/// The operator for a container expression, either 'in' or 'not in'.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ContainerOperator {
    /// `in`
    In,
    /// `not in`
    NotIn,
}

impl ContainerOperator {
    /// Creates a [`ContainerOperator`] from an equivalent [`MarkerOperator`].
    ///
    /// Returns `None` if the operator is not supported for containers.
    pub(crate) fn from_marker_operator(operator: MarkerOperator) -> Option<Self> {
        match operator {
            MarkerOperator::In => Some(Self::In),
            MarkerOperator::NotIn => Some(Self::NotIn),
            _ => None,
        }
    }
}

impl Display for ContainerOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::In => "in",
            Self::NotIn => "not in",
        })
    }
}

impl MarkerExpression {
    /// Parse a [`MarkerExpression`] from a string with the given reporter.
    pub fn parse_reporter(
        s: &str,
        reporter: &mut impl Reporter,
    ) -> Result<Option<Self>, Pep508Error> {
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

    /// Parse a [`MarkerExpression`] from a string.
    ///
    /// Returns `None` if the expression consists entirely of meaningless expressions
    /// that are ignored, such as `os_name ~= 'foo'`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Option<Self>, Pep508Error> {
        Self::parse_reporter(s, &mut TracingReporter)
    }

    /// Return the kind of this marker expression.
    pub(crate) fn kind(&self) -> MarkerExpressionKind {
        match self {
            Self::Version { key, .. } => MarkerExpressionKind::Version(*key),
            Self::VersionIn { key, .. } => MarkerExpressionKind::VersionIn(*key),
            Self::String { key, .. } => MarkerExpressionKind::String(*key),
            Self::List { pair, .. } => MarkerExpressionKind::List(pair.key()),
            Self::Extra { .. } => MarkerExpressionKind::Extra,
        }
    }
}

impl Display for MarkerExpression {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version { key, specifier } => {
                let (op, version) = (specifier.operator(), specifier.version());
                if op == &uv_pep440::Operator::EqualStar || op == &uv_pep440::Operator::NotEqualStar
                {
                    return write!(f, "{key} {op} '{version}.*'");
                }
                write!(f, "{key} {op} '{version}'")
            }
            Self::VersionIn {
                key,
                versions,
                operator,
            } => {
                let versions = versions.iter().map(ToString::to_string).join(" ");
                write!(f, "{key} {operator} '{versions}'")
            }
            Self::String {
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
            Self::List { pair, operator } => {
                write!(f, "'{}' {} {}", pair.value(), operator, pair.key())
            }
            Self::Extra { operator, name } => {
                write!(f, "extra {operator} '{name}'")
            }
        }
    }
}

/// The extra and dependency group names to use when evaluating a marker tree.
#[derive(Debug, Copy, Clone)]
enum ExtrasEnvironment<'a> {
    /// E.g., `extra == '...'`
    Extras(&'a [ExtraName]),
    /// E.g., `'...' in extras` or `'...' in dependency_groups`
    Pep751(&'a [ExtraName], &'a [GroupName]),
}

impl<'a> ExtrasEnvironment<'a> {
    /// Creates a new [`ExtrasEnvironment`] for the given `extra` names.
    fn from_extras(extras: &'a [ExtraName]) -> Self {
        Self::Extras(extras)
    }

    /// Creates a new [`ExtrasEnvironment`] for the given PEP 751 `extras` and `dependency_groups`.
    fn from_pep751(extras: &'a [ExtraName], dependency_groups: &'a [GroupName]) -> Self {
        Self::Pep751(extras, dependency_groups)
    }

    /// Returns the `extra` names in this environment.
    fn extra(&self) -> &[ExtraName] {
        match self {
            Self::Extras(extra) => extra,
            Self::Pep751(..) => &[],
        }
    }

    /// Returns the `extras` names in this environment, as in a PEP 751 lockfile.
    fn extras(&self) -> &[ExtraName] {
        match self {
            Self::Extras(..) => &[],
            Self::Pep751(extras, ..) => extras,
        }
    }

    /// Returns the `dependency_group` group names in this environment, as in a PEP 751 lockfile.
    fn dependency_groups(&self) -> &[GroupName] {
        match self {
            Self::Extras(..) => &[],
            Self::Pep751(.., groups) => groups,
        }
    }
}

/// Represents one or more nested marker expressions with and/or/parentheses.
///
/// Marker trees are canonical, meaning any two functionally equivalent markers
/// will compare equally. Markers also support efficient polynomial-time operations,
/// such as conjunction and disjunction.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct MarkerTree(NodeId);

impl Default for MarkerTree {
    fn default() -> Self {
        Self::TRUE
    }
}

impl<'de> Deserialize<'de> for MarkerTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = MarkerTree;

            fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                MarkerTree::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
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

    /// An empty marker that always evaluates to `true`.
    pub const TRUE: Self = Self(NodeId::TRUE);

    /// An unsatisfiable marker that always evaluates to `false`.
    pub const FALSE: Self = Self(NodeId::FALSE);

    /// Returns a marker tree for a single expression.
    pub fn expression(expr: MarkerExpression) -> Self {
        Self(INTERNER.lock().expression(expr))
    }

    /// Whether the marker always evaluates to `true`.
    ///
    /// If this method returns `true`, it is definitively known that the marker will
    /// evaluate to `true` in any environment. However, this method may return false
    /// negatives, i.e. it may not be able to detect that a marker is always true for
    /// complex expressions.
    pub fn is_true(self) -> bool {
        self.0.is_true()
    }

    /// Whether the marker always evaluates to `false`, i.e. the expression is not
    /// satisfiable in any environment.
    ///
    /// If this method returns `true`, it is definitively known that the marker will
    /// evaluate to `false` in any environment. However, this method may return false
    /// negatives, i.e. it may not be able to detect that a marker is unsatisfiable
    /// for complex expressions.
    pub fn is_false(self) -> bool {
        self.0.is_false()
    }

    /// Returns a new marker tree that is the negation of this one.
    #[must_use]
    pub fn negate(self) -> Self {
        Self(self.0.not())
    }

    /// Combine this marker tree with the one given via a conjunction.
    pub fn and(&mut self, tree: Self) {
        self.0 = INTERNER.lock().and(self.0, tree.0);
    }

    /// Combine this marker tree with the one given via a disjunction.
    pub fn or(&mut self, tree: Self) {
        self.0 = INTERNER.lock().or(self.0, tree.0);
    }

    /// Sets this to a marker equivalent to the implication of this one and the
    /// given consequent.
    ///
    /// If the marker set is always `true`, then it can be said that `self`
    /// implies `consequent`.
    pub fn implies(&mut self, consequent: Self) {
        // This could probably be optimized, but is clearly
        // correct, since logical implication is `-P or Q`.
        *self = self.negate();
        self.or(consequent);
    }

    /// Returns `true` if there is no environment in which both marker trees can apply,
    /// i.e. their conjunction is always `false`.
    ///
    /// If this method returns `true`, it is definitively known that the two markers can
    /// never both evaluate to `true` in a given environment. However, this method may return
    /// false negatives, i.e. it may not be able to detect that two markers are disjoint for
    /// complex expressions.
    pub fn is_disjoint(self, other: Self) -> bool {
        INTERNER.lock().is_disjoint(self.0, other.0)
    }

    /// Returns the contents of this marker tree, if it contains at least one expression.
    ///
    /// If the marker is `true`, this method will return `None`.
    /// If the marker is `false`, the marker is represented as the normalized expression, `python_version < '0'`.
    ///
    /// The returned type implements [`Display`] and [`serde::Serialize`].
    pub fn contents(self) -> Option<MarkerTreeContents> {
        if self.is_true() {
            return None;
        }

        Some(MarkerTreeContents(self))
    }

    /// Returns a simplified string representation of this marker, if it contains at least one
    /// expression.
    ///
    /// If the marker is `true`, this method will return `None`.
    /// If the marker is `false`, the marker is represented as the normalized expression, `python_version < '0'`.
    pub fn try_to_string(self) -> Option<String> {
        self.contents().map(|contents| contents.to_string())
    }

    /// Returns the underlying [`MarkerTreeKind`] of the root node.
    pub fn kind(self) -> MarkerTreeKind<'static> {
        if self.is_true() {
            return MarkerTreeKind::True;
        }

        if self.is_false() {
            return MarkerTreeKind::False;
        }

        let node = INTERNER.shared.node(self.0);
        match &node.var {
            Variable::Version(key) => {
                let Edges::Version { edges: ref map } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::Version(VersionMarkerTree {
                    id: self.0,
                    key: *key,
                    map,
                })
            }
            Variable::String(key) => {
                let Edges::String { edges: ref map } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::String(StringMarkerTree {
                    id: self.0,
                    key: *key,
                    map,
                })
            }
            Variable::In { key, value } => {
                let Edges::Boolean { low, high } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::In(InMarkerTree {
                    key: *key,
                    value,
                    high: high.negate(self.0),
                    low: low.negate(self.0),
                })
            }
            Variable::Contains { key, value } => {
                let Edges::Boolean { low, high } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::Contains(ContainsMarkerTree {
                    key: *key,
                    value,
                    high: high.negate(self.0),
                    low: low.negate(self.0),
                })
            }
            Variable::List(key) => {
                let Edges::Boolean { low, high } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::List(ListMarkerTree {
                    pair: key,
                    high: high.negate(self.0),
                    low: low.negate(self.0),
                })
            }
            Variable::Extra(name) => {
                let Edges::Boolean { low, high } = node.children else {
                    unreachable!()
                };
                MarkerTreeKind::Extra(ExtraMarkerTree {
                    name,
                    high: high.negate(self.0),
                    low: low.negate(self.0),
                })
            }
        }
    }

    /// Returns a simplified DNF expression for this marker tree.
    pub fn to_dnf(self) -> Vec<Vec<MarkerExpression>> {
        simplify::to_dnf(self)
    }

    /// Does this marker apply in the given environment?
    pub fn evaluate(self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        self.evaluate_reporter_impl(
            env,
            ExtrasEnvironment::from_extras(extras),
            &mut TracingReporter,
        )
    }

    /// Evaluate a marker in the context of a PEP 751 lockfile, which exposes several additional
    /// markers (`extras` and `dependency_groups`) that are not available in any other context,
    /// per the spec.
    pub fn evaluate_pep751(
        self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        groups: &[GroupName],
    ) -> bool {
        self.evaluate_reporter_impl(
            env,
            ExtrasEnvironment::from_pep751(extras, groups),
            &mut TracingReporter,
        )
    }

    /// Evaluates this marker tree against an optional environment and a
    /// possibly empty sequence of extras.
    ///
    /// When an environment is not provided, all marker expressions based on
    /// the environment evaluate to `true`. That is, this provides environment
    /// independent marker evaluation. In practice, this means only the extras
    /// are evaluated when an environment is not provided.
    pub fn evaluate_optional_environment(
        self,
        env: Option<&MarkerEnvironment>,
        extras: &[ExtraName],
    ) -> bool {
        match env {
            None => self.evaluate_extras(extras),
            Some(env) => self.evaluate_reporter_impl(
                env,
                ExtrasEnvironment::from_extras(extras),
                &mut TracingReporter,
            ),
        }
    }

    /// Same as [`Self::evaluate`], but instead of using logging to warn, you can pass your own
    /// handler for warnings
    pub fn evaluate_reporter(
        self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
        reporter: &mut impl Reporter,
    ) -> bool {
        self.evaluate_reporter_impl(env, ExtrasEnvironment::from_extras(extras), reporter)
    }

    fn evaluate_reporter_impl(
        self,
        env: &MarkerEnvironment,
        extras: ExtrasEnvironment,
        reporter: &mut impl Reporter,
    ) -> bool {
        match self.kind() {
            MarkerTreeKind::True => return true,
            MarkerTreeKind::False => return false,
            MarkerTreeKind::Version(marker) => {
                for (range, tree) in marker.edges() {
                    if range.contains(env.get_version(marker.key())) {
                        return tree.evaluate_reporter_impl(env, extras, reporter);
                    }
                }
            }
            MarkerTreeKind::String(marker) => {
                for (range, tree) in marker.children() {
                    let l_string = env.get_string(marker.key());

                    if range.as_singleton().is_none() {
                        if let Some((start, end)) = range.bounding_range() {
                            if let Bound::Included(value) | Bound::Excluded(value) = start {
                                reporter.report(
                                    MarkerWarningKind::LexicographicComparison,
                                    format!("Comparing {l_string} and {value} lexicographically"),
                                );
                            }

                            if let Bound::Included(value) | Bound::Excluded(value) = end {
                                reporter.report(
                                    MarkerWarningKind::LexicographicComparison,
                                    format!("Comparing {l_string} and {value} lexicographically"),
                                );
                            }
                        }
                    }

                    if range.contains(l_string) {
                        return tree.evaluate_reporter_impl(env, extras, reporter);
                    }
                }
            }
            MarkerTreeKind::In(marker) => {
                return marker
                    .edge(marker.value().contains(env.get_string(marker.key())))
                    .evaluate_reporter_impl(env, extras, reporter);
            }
            MarkerTreeKind::Contains(marker) => {
                return marker
                    .edge(env.get_string(marker.key()).contains(marker.value()))
                    .evaluate_reporter_impl(env, extras, reporter);
            }
            MarkerTreeKind::Extra(marker) => {
                return marker
                    .edge(extras.extra().contains(marker.name().extra()))
                    .evaluate_reporter_impl(env, extras, reporter);
            }
            MarkerTreeKind::List(marker) => {
                let edge = match marker.pair() {
                    CanonicalMarkerListPair::Extras(extra) => extras.extras().contains(extra),
                    CanonicalMarkerListPair::DependencyGroup(dependency_group) => {
                        extras.dependency_groups().contains(dependency_group)
                    }
                    // Invalid marker expression
                    CanonicalMarkerListPair::Arbitrary { .. } => return false,
                };

                return marker
                    .edge(edge)
                    .evaluate_reporter_impl(env, extras, reporter);
            }
        }

        false
    }

    /// Checks if the requirement should be activated with the given set of active extras without evaluating
    /// the remaining environment markers, i.e. if there is potentially an environment that could activate this
    /// requirement.
    pub fn evaluate_extras(self, extras: &[ExtraName]) -> bool {
        match self.kind() {
            MarkerTreeKind::True => true,
            MarkerTreeKind::False => false,
            MarkerTreeKind::Version(marker) => {
                marker.edges().any(|(_, tree)| tree.evaluate_extras(extras))
            }
            MarkerTreeKind::String(marker) => marker
                .children()
                .any(|(_, tree)| tree.evaluate_extras(extras)),
            MarkerTreeKind::In(marker) => marker
                .children()
                .any(|(_, tree)| tree.evaluate_extras(extras)),
            MarkerTreeKind::Contains(marker) => marker
                .children()
                .any(|(_, tree)| tree.evaluate_extras(extras)),
            MarkerTreeKind::List(marker) => marker
                .children()
                .any(|(_, tree)| tree.evaluate_extras(extras)),
            MarkerTreeKind::Extra(marker) => marker
                .edge(extras.contains(marker.name().extra()))
                .evaluate_extras(extras),
        }
    }

    /// Find a top level `extra == "..."` expression.
    ///
    /// ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part of the
    /// main conjunction.
    pub fn top_level_extra(self) -> Option<MarkerExpression> {
        let mut extra_expression = None;
        for conjunction in self.to_dnf() {
            let found = conjunction.iter().find(|expression| {
                matches!(
                    expression,
                    MarkerExpression::Extra {
                        operator: ExtraOperator::Equal,
                        ..
                    }
                )
            })?;

            // Because the marker tree is in DNF form, we must verify that the extra expression is part
            // of all solutions to this marker.
            if let Some(ref extra_expression) = extra_expression {
                if *extra_expression != *found {
                    return None;
                }

                continue;
            }

            extra_expression = Some(found.clone());
        }

        extra_expression
    }

    /// Find a top level `extra == "..."` name.
    ///
    /// ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part of the
    /// main conjunction.
    pub fn top_level_extra_name(self) -> Option<Cow<'static, ExtraName>> {
        // Fast path: The marker is only a `extra == "..."`.
        if let MarkerTreeKind::Extra(marker) = self.kind() {
            if marker.edge(true).is_true() {
                let CanonicalMarkerValueExtra::Extra(extra) = marker.name;
                return Some(Cow::Borrowed(extra));
            }
        }

        let extra_expression = self.top_level_extra()?;

        match extra_expression {
            MarkerExpression::Extra { name, .. } => name.into_extra().map(Cow::Owned),
            _ => unreachable!(),
        }
    }

    /// Simplify this marker by *assuming* that the Python version range
    /// provided is true and that the complement of it is false.
    ///
    /// For example, with `requires-python = '>=3.8'` and a marker tree of
    /// `python_full_version >= '3.8' and python_full_version <= '3.10'`, this
    /// would result in a marker of `python_full_version <= '3.10'`.
    ///
    /// This is useful when one wants to write "simpler" markers in a
    /// particular context with a bound on the supported Python versions.
    /// In general, the simplified markers returned shouldn't be used for
    /// evaluation. Instead, they should be turned back into their more
    /// "complex" form first.
    ///
    /// Note that simplifying a marker and then complexifying it, even
    /// with the same Python version bounds, is a lossy operation. For
    /// example, simplifying `python_version < '3.7'` with `requires-python
    /// = ">=3.8"` will result in a marker that always returns false (e.g.,
    /// `python_version < '0'`). Therefore, complexifying an always-false
    /// marker will result in a marker that is still always false, despite
    /// the fact that the original marker was true for `<3.7`. Therefore,
    /// simplifying should only be done as a one-way transformation when it is
    /// known that `requires-python` reflects an eternal lower bound on the
    /// results of that simplification. (If `requires-python` changes, then one
    /// should reconstitute all relevant markers from the source data.)
    #[must_use]
    pub fn simplify_python_versions(self, lower: Bound<&Version>, upper: Bound<&Version>) -> Self {
        Self(
            INTERNER
                .lock()
                .simplify_python_versions(self.0, lower, upper),
        )
    }

    /// Complexify marker tree by requiring the given Python version range
    /// to be true in order for this marker tree to evaluate to true in all
    /// circumstances.
    ///
    /// For example, with `requires-python = '>=3.8'` and a marker tree of
    /// `python_full_version <= '3.10'`, this would result in a marker of
    /// `python_full_version >= '3.8' and python_full_version <= '3.10'`.
    #[must_use]
    pub fn complexify_python_versions(
        self,
        lower: Bound<&Version>,
        upper: Bound<&Version>,
    ) -> Self {
        Self(
            INTERNER
                .lock()
                .complexify_python_versions(self.0, lower, upper),
        )
    }

    /// Remove the extras from a marker, returning `None` if the marker tree evaluates to `true`.
    ///
    /// Any `extra` markers that are always `true` given the provided extras will be removed.
    /// Any `extra` markers that are always `false` given the provided extras will be left
    /// unchanged.
    ///
    /// For example, if `dev` is a provided extra, given `sys_platform == 'linux' and extra == 'dev'`,
    /// the marker will be simplified to `sys_platform == 'linux'`.
    #[must_use]
    pub fn simplify_extras(self, extras: &[ExtraName]) -> Self {
        self.simplify_extras_with(|name| extras.contains(name))
    }

    /// Remove negated extras from a marker, returning `None` if the marker
    /// tree evaluates to `true`.
    ///
    /// Any negated `extra` markers that are always `true` given the provided
    /// extras will be removed. Any `extra` markers that are always `false`
    /// given the provided extras will be left unchanged.
    ///
    /// For example, if `dev` is a provided extra, given `sys_platform
    /// == 'linux' and extra != 'dev'`, the marker will be simplified to
    /// `sys_platform == 'linux'`.
    #[must_use]
    pub fn simplify_not_extras(self, extras: &[ExtraName]) -> Self {
        self.simplify_not_extras_with(|name| extras.contains(name))
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
    #[must_use]
    pub fn simplify_extras_with(self, is_extra: impl Fn(&ExtraName) -> bool) -> Self {
        // Because `simplify_extras_with_impl` is recursive, and we need to use
        // our predicate in recursive calls, we need the predicate itself to
        // have some indirection (or else we'd have to clone it). To avoid a
        // recursive type at codegen time, we just introduce the indirection
        // here, but keep the calling API ergonomic.
        self.simplify_extras_with_impl(&is_extra)
    }

    /// Remove negated extras from a marker, returning `None` if the marker tree evaluates to
    /// `true`.
    ///
    /// Any negated `extra` markers that are always `true` given the provided
    /// predicate will be removed. Any `extra` markers that are always `false`
    /// given the provided predicate will be left unchanged.
    ///
    /// For example, if `is_extra('dev')` is true, given
    /// `sys_platform == 'linux' and extra != 'dev'`, the marker will be simplified to
    /// `sys_platform == 'linux'`.
    #[must_use]
    pub fn simplify_not_extras_with(self, is_extra: impl Fn(&ExtraName) -> bool) -> Self {
        // Because `simplify_extras_with_impl` is recursive, and we need to use
        // our predicate in recursive calls, we need the predicate itself to
        // have some indirection (or else we'd have to clone it). To avoid a
        // recursive type at codegen time, we just introduce the indirection
        // here, but keep the calling API ergonomic.
        self.simplify_not_extras_with_impl(&is_extra)
    }

    /// Returns a new `MarkerTree` where all `extra` expressions are removed.
    ///
    /// If the marker only consisted of `extra` expressions, then a marker that
    /// is always true is returned.
    #[must_use]
    pub fn without_extras(self) -> Self {
        Self(INTERNER.lock().without_extras(self.0))
    }

    /// Returns a new `MarkerTree` where only `extra` expressions are removed.
    ///
    /// If the marker did not contain any `extra` expressions, then a marker
    /// that is always true is returned.
    #[must_use]
    pub fn only_extras(self) -> Self {
        Self(INTERNER.lock().only_extras(self.0))
    }

    /// Calls the provided function on every `extra` in this tree.
    ///
    /// The operator provided to the function is guaranteed to be
    /// `MarkerOperator::Equal` or `MarkerOperator::NotEqual`.
    pub fn visit_extras(self, mut f: impl FnMut(MarkerOperator, &ExtraName)) {
        fn imp(tree: MarkerTree, f: &mut impl FnMut(MarkerOperator, &ExtraName)) {
            match tree.kind() {
                MarkerTreeKind::True | MarkerTreeKind::False => {}
                MarkerTreeKind::Version(kind) => {
                    for (tree, _) in simplify::collect_edges(kind.edges()) {
                        imp(tree, f);
                    }
                }
                MarkerTreeKind::String(kind) => {
                    for (tree, _) in simplify::collect_edges(kind.children()) {
                        imp(tree, f);
                    }
                }
                MarkerTreeKind::In(kind) => {
                    for (_, tree) in kind.children() {
                        imp(tree, f);
                    }
                }
                MarkerTreeKind::Contains(kind) => {
                    for (_, tree) in kind.children() {
                        imp(tree, f);
                    }
                }
                MarkerTreeKind::List(kind) => {
                    for (_, tree) in kind.children() {
                        imp(tree, f);
                    }
                }
                MarkerTreeKind::Extra(kind) => {
                    if kind.low.is_false() {
                        f(MarkerOperator::Equal, kind.name().extra());
                    } else {
                        f(MarkerOperator::NotEqual, kind.name().extra());
                    }
                    for (_, tree) in kind.children() {
                        imp(tree, f);
                    }
                }
            }
        }
        imp(self, &mut f);
    }

    fn simplify_extras_with_impl(self, is_extra: &impl Fn(&ExtraName) -> bool) -> Self {
        Self(INTERNER.lock().restrict(self.0, &|var| match var {
            Variable::Extra(name) => is_extra(name.extra()).then_some(true),
            _ => None,
        }))
    }

    fn simplify_not_extras_with_impl(self, is_extra: &impl Fn(&ExtraName) -> bool) -> Self {
        Self(INTERNER.lock().restrict(self.0, &|var| match var {
            Variable::Extra(name) => is_extra(name.extra()).then_some(false),
            _ => None,
        }))
    }
}

impl fmt::Debug for MarkerTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.is_true() {
            return write!(f, "true");
        }
        if self.is_false() {
            return write!(f, "false");
        }
        write!(f, "{}", self.contents().unwrap())
    }
}

impl MarkerTree {
    /// Formats a [`MarkerTree`] as a graph.
    ///
    /// This is useful for debugging when one wants to look at a
    /// representation of a `MarkerTree` that is more faithful to its
    /// internal representation.
    pub fn debug_graph(&self) -> MarkerTreeDebugGraph<'_> {
        MarkerTreeDebugGraph { marker: self }
    }

    /// Formats a [`MarkerTree`] in its "raw" representation.
    ///
    /// This is useful for debugging when one wants to look at a
    /// representation of a `MarkerTree` that is precisely identical
    /// to its internal representation.
    pub fn debug_raw(&self) -> MarkerTreeDebugRaw<'_> {
        MarkerTreeDebugRaw { marker: self }
    }

    fn fmt_graph(self, f: &mut Formatter<'_>, level: usize) -> fmt::Result {
        match self.kind() {
            MarkerTreeKind::True => return write!(f, "true"),
            MarkerTreeKind::False => return write!(f, "false"),
            MarkerTreeKind::Version(kind) => {
                for (tree, range) in simplify::collect_edges(kind.edges()) {
                    writeln!(f)?;
                    for _ in 0..level {
                        write!(f, "  ")?;
                    }

                    write!(f, "{key}{range} -> ", key = kind.key())?;
                    tree.fmt_graph(f, level + 1)?;
                }
            }
            MarkerTreeKind::String(kind) => {
                for (tree, range) in simplify::collect_edges(kind.children()) {
                    writeln!(f)?;
                    for _ in 0..level {
                        write!(f, "  ")?;
                    }

                    write!(f, "{key}{range} -> ", key = kind.key())?;
                    tree.fmt_graph(f, level + 1)?;
                }
            }
            MarkerTreeKind::In(kind) => {
                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} in {} -> ", kind.key(), kind.value())?;
                kind.edge(true).fmt_graph(f, level + 1)?;

                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} not in {} -> ", kind.key(), kind.value())?;
                kind.edge(false).fmt_graph(f, level + 1)?;
            }
            MarkerTreeKind::Contains(kind) => {
                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} in {} -> ", kind.value(), kind.key())?;
                kind.edge(true).fmt_graph(f, level + 1)?;

                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} not in {} -> ", kind.value(), kind.key())?;
                kind.edge(false).fmt_graph(f, level + 1)?;
            }
            MarkerTreeKind::List(kind) => {
                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} in {} -> ", kind.value(), kind.key())?;
                kind.edge(true).fmt_graph(f, level + 1)?;

                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "{} not in {} -> ", kind.value(), kind.key())?;
                kind.edge(false).fmt_graph(f, level + 1)?;
            }
            MarkerTreeKind::Extra(kind) => {
                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "extra == {} -> ", kind.name())?;
                kind.edge(true).fmt_graph(f, level + 1)?;

                writeln!(f)?;
                for _ in 0..level {
                    write!(f, "  ")?;
                }
                write!(f, "extra != {} -> ", kind.name())?;
                kind.edge(false).fmt_graph(f, level + 1)?;
            }
        }

        Ok(())
    }
}

impl PartialOrd for MarkerTree {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MarkerTree {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind().cmp(&other.kind())
    }
}

/// Formats a [`MarkerTree`] as a graph.
///
/// This type is created by the [`MarkerTree::debug_graph`] routine.
#[derive(Clone)]
pub struct MarkerTreeDebugGraph<'a> {
    marker: &'a MarkerTree,
}

impl fmt::Debug for MarkerTreeDebugGraph<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.marker.fmt_graph(f, 0)
    }
}

/// Formats a [`MarkerTree`] using its raw internals.
///
/// This is very verbose and likely only useful if you're working
/// on the internals of this crate.
///
/// This type is created by the [`MarkerTree::debug_raw`] routine.
#[derive(Clone)]
pub struct MarkerTreeDebugRaw<'a> {
    marker: &'a MarkerTree,
}

impl fmt::Debug for MarkerTreeDebugRaw<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let node = INTERNER.shared.node(self.marker.0);
        f.debug_tuple("MarkerTreeDebugRaw").field(node).finish()
    }
}

/// The underlying kind of an arbitrary node in a [`MarkerTree`].
///
/// A marker tree is represented as an algebraic decision tree with two terminal nodes
/// `True` or `False`. The edges of a given node correspond to a particular assignment of
/// a value to that variable.
#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord)]
pub enum MarkerTreeKind<'a> {
    /// An empty marker that always evaluates to `true`.
    True,
    /// An unsatisfiable marker that always evaluates to `false`.
    False,
    /// A version expression.
    Version(VersionMarkerTree<'a>),
    /// A string expression.
    String(StringMarkerTree<'a>),
    /// A string expression with the `in` operator.
    In(InMarkerTree<'a>),
    /// A string expression with the `contains` operator.
    Contains(ContainsMarkerTree<'a>),
    /// A `in` or `not in` expression.
    List(ListMarkerTree<'a>),
    /// An extra expression (e.g., `extra == 'dev'`).
    Extra(ExtraMarkerTree<'a>),
}

/// A version marker node, such as `python_version < '3.7'`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VersionMarkerTree<'a> {
    id: NodeId,
    key: CanonicalMarkerValueVersion,
    map: &'a [(Ranges<Version>, NodeId)],
}

impl VersionMarkerTree<'_> {
    /// The key for this node.
    pub fn key(&self) -> CanonicalMarkerValueVersion {
        self.key
    }

    /// The edges of this node, corresponding to possible output ranges of the given variable.
    pub fn edges(&self) -> impl ExactSizeIterator<Item = (&Ranges<Version>, MarkerTree)> + '_ {
        self.map
            .iter()
            .map(|(range, node)| (range, MarkerTree(node.negate(self.id))))
    }
}

impl PartialOrd for VersionMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VersionMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(&other.key())
            .then_with(|| self.edges().cmp(other.edges()))
    }
}

/// A string marker node, such as `os_name == 'Linux'`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct StringMarkerTree<'a> {
    id: NodeId,
    key: CanonicalMarkerValueString,
    map: &'a [(Ranges<ArcStr>, NodeId)],
}

impl StringMarkerTree<'_> {
    /// The key for this node.
    pub fn key(&self) -> CanonicalMarkerValueString {
        self.key
    }

    /// The edges of this node, corresponding to possible output ranges of the given variable.
    pub fn children(&self) -> impl ExactSizeIterator<Item = (&Ranges<ArcStr>, MarkerTree)> {
        self.map
            .iter()
            .map(|(range, node)| (range, MarkerTree(node.negate(self.id))))
    }
}

impl PartialOrd for StringMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(&other.key())
            .then_with(|| self.children().cmp(other.children()))
    }
}

/// A string marker node with the `in` operator, such as `os_name in 'WindowsLinux'`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct InMarkerTree<'a> {
    key: CanonicalMarkerValueString,
    value: &'a ArcStr,
    high: NodeId,
    low: NodeId,
}

impl InMarkerTree<'_> {
    /// The key (LHS) for this expression.
    pub fn key(&self) -> CanonicalMarkerValueString {
        self.key
    }

    /// The value (RHS) for this expression.
    pub fn value(&self) -> &ArcStr {
        self.value
    }

    /// The edges of this node, corresponding to the boolean evaluation of the expression.
    pub fn children(&self) -> impl Iterator<Item = (bool, MarkerTree)> {
        [(true, MarkerTree(self.high)), (false, MarkerTree(self.low))].into_iter()
    }

    /// Returns the subtree associated with the given edge value.
    pub fn edge(&self, value: bool) -> MarkerTree {
        if value {
            MarkerTree(self.high)
        } else {
            MarkerTree(self.low)
        }
    }
}

impl PartialOrd for InMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(&other.key())
            .then_with(|| self.value().cmp(other.value()))
            .then_with(|| self.children().cmp(other.children()))
    }
}

/// A string marker node with inverse of the `in` operator, such as `'nux' in os_name`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ContainsMarkerTree<'a> {
    key: CanonicalMarkerValueString,
    value: &'a str,
    high: NodeId,
    low: NodeId,
}

impl ContainsMarkerTree<'_> {
    /// The key (LHS) for this expression.
    pub fn key(&self) -> CanonicalMarkerValueString {
        self.key
    }

    /// The value (RHS) for this expression.
    pub fn value(&self) -> &str {
        self.value
    }

    /// The edges of this node, corresponding to the boolean evaluation of the expression.
    pub fn children(&self) -> impl Iterator<Item = (bool, MarkerTree)> {
        [(true, MarkerTree(self.high)), (false, MarkerTree(self.low))].into_iter()
    }

    /// Returns the subtree associated with the given edge value.
    pub fn edge(&self, value: bool) -> MarkerTree {
        if value {
            MarkerTree(self.high)
        } else {
            MarkerTree(self.low)
        }
    }
}

impl PartialOrd for ContainsMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ContainsMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(&other.key())
            .then_with(|| self.value().cmp(other.value()))
            .then_with(|| self.children().cmp(other.children()))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListMarkerTree<'a> {
    // No separate canonical type, the type is already canonical.
    pair: &'a CanonicalMarkerListPair,
    high: NodeId,
    low: NodeId,
}

impl ListMarkerTree<'_> {
    /// The key-value pair for this expression
    pub fn pair(&self) -> &CanonicalMarkerListPair {
        self.pair
    }

    /// The key (RHS) for this expression.
    pub fn key(&self) -> MarkerValueList {
        self.pair.key()
    }

    /// The value (LHS) for this expression.
    pub fn value(&self) -> String {
        self.pair.value()
    }

    /// The edges of this node, corresponding to the boolean evaluation of the expression.
    pub fn children(&self) -> impl Iterator<Item = (bool, MarkerTree)> {
        [(true, MarkerTree(self.high)), (false, MarkerTree(self.low))].into_iter()
    }

    /// Returns the subtree associated with the given edge value.
    pub fn edge(&self, value: bool) -> MarkerTree {
        if value {
            MarkerTree(self.high)
        } else {
            MarkerTree(self.low)
        }
    }
}

impl PartialOrd for ListMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ListMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.pair()
            .cmp(other.pair())
            .then_with(|| self.children().cmp(other.children()))
    }
}

/// A node representing the existence or absence of a given extra, such as `extra == 'bar'`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ExtraMarkerTree<'a> {
    name: &'a CanonicalMarkerValueExtra,
    high: NodeId,
    low: NodeId,
}

impl ExtraMarkerTree<'_> {
    /// Returns the name of the extra in this expression.
    pub fn name(&self) -> &CanonicalMarkerValueExtra {
        self.name
    }

    /// The edges of this node, corresponding to the boolean evaluation of the expression.
    pub fn children(&self) -> impl Iterator<Item = (bool, MarkerTree)> {
        [(true, MarkerTree(self.high)), (false, MarkerTree(self.low))].into_iter()
    }

    /// Returns the subtree associated with the given edge value.
    pub fn edge(&self, value: bool) -> MarkerTree {
        if value {
            MarkerTree(self.high)
        } else {
            MarkerTree(self.low)
        }
    }
}

impl PartialOrd for ExtraMarkerTree<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExtraMarkerTree<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name()
            .cmp(other.name())
            .then_with(|| self.children().cmp(other.children()))
    }
}

/// A marker tree that contains at least one expression.
///
/// See [`MarkerTree::contents`] for details.
#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
pub struct MarkerTreeContents(MarkerTree);

impl From<MarkerTreeContents> for MarkerTree {
    fn from(contents: MarkerTreeContents) -> Self {
        contents.0
    }
}

impl From<Option<MarkerTreeContents>> for MarkerTree {
    fn from(marker: Option<MarkerTreeContents>) -> Self {
        marker.map(|contents| contents.0).unwrap_or_default()
    }
}

impl AsRef<MarkerTree> for MarkerTreeContents {
    fn as_ref(&self) -> &MarkerTree {
        &self.0
    }
}

impl Serialize for MarkerTreeContents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for MarkerTreeContents {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Normalize all `false` expressions to the same trivially false expression.
        if self.0.is_false() {
            return write!(f, "python_version < '0'");
        }

        // Write the output in DNF form.
        let dnf = self.0.to_dnf();
        let format_conjunction = |conjunction: &Vec<MarkerExpression>| {
            conjunction
                .iter()
                .map(MarkerExpression::to_string)
                .collect::<Vec<String>>()
                .join(" and ")
        };

        let expr = match &dnf[..] {
            [conjunction] => format_conjunction(conjunction),
            _ => dnf
                .iter()
                .map(|conjunction| {
                    if conjunction.len() == 1 {
                        format_conjunction(conjunction)
                    } else {
                        format!("({})", format_conjunction(conjunction))
                    }
                })
                .collect::<Vec<String>>()
                .join(" or "),
        };

        f.write_str(&expr)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for MarkerTree {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("MarkerTree")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A PEP 508-compliant marker expression, e.g., `sys_platform == 'Darwin'`"
        })
    }
}

#[cfg(test)]
mod test {
    use std::ops::Bound;
    use std::str::FromStr;

    use insta::assert_snapshot;

    use uv_normalize::ExtraName;
    use uv_pep440::Version;

    use crate::marker::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use crate::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};

    fn parse_err(input: &str) -> String {
        MarkerTree::from_str(input).unwrap_err().to_string()
    }

    fn m(s: &str) -> MarkerTree {
        s.parse().unwrap()
    }

    fn env37() -> MarkerEnvironment {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "",
            implementation_version: "3.7",
            os_name: "linux",
            platform_machine: "x86_64",
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
            assert_eq!(m(a), m(b), "{a} {b}");
        }
    }

    #[test]
    fn simplify_python_versions() {
        assert_eq!(
            m("(extra == 'foo' and sys_platform == 'win32') or extra == 'foo'")
                .simplify_extras(&["foo".parse().unwrap()]),
            MarkerTree::TRUE
        );

        assert_eq!(
            m("(python_version <= '3.11' and sys_platform == 'win32') or python_version > '3.11'")
                .simplify_python_versions(
                    Bound::Excluded(Version::new([3, 12])).as_ref(),
                    Bound::Unbounded.as_ref(),
                ),
            MarkerTree::TRUE
        );

        assert_eq!(
            m("python_version < '3.10'")
                .simplify_python_versions(
                    Bound::Excluded(Version::new([3, 7])).as_ref(),
                    Bound::Unbounded.as_ref(),
                )
                .try_to_string()
                .unwrap(),
            "python_full_version < '3.10'"
        );

        // Note that `3.12.1` will still match.
        assert_eq!(
            m("python_version <= '3.12'")
                .simplify_python_versions(
                    Bound::Excluded(Version::new([3, 12])).as_ref(),
                    Bound::Unbounded.as_ref(),
                )
                .try_to_string()
                .unwrap(),
            "python_full_version < '3.13'"
        );

        assert_eq!(
            m("python_full_version <= '3.12'").simplify_python_versions(
                Bound::Excluded(Version::new([3, 12])).as_ref(),
                Bound::Unbounded.as_ref(),
            ),
            MarkerTree::FALSE
        );

        assert_eq!(
            m("python_full_version <= '3.12.1'")
                .simplify_python_versions(
                    Bound::Excluded(Version::new([3, 12])).as_ref(),
                    Bound::Unbounded.as_ref(),
                )
                .try_to_string()
                .unwrap(),
            "python_full_version <= '3.12.1'"
        );
    }

    #[test]
    fn release_only() {
        assert!(m("python_full_version > '3.10' or python_full_version <= '3.10'").is_true());
        assert!(
            m("python_full_version > '3.10' or python_full_version <= '3.10'")
                .negate()
                .is_false()
        );
        assert!(m("python_full_version > '3.10' and python_full_version <= '3.10'").is_false());
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
    fn test_version_in_evaluation() {
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

        let marker = MarkerTree::from_str("python_version in \"2.7 3.2 3.3\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_version in \"2.7 3.7\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_version in \"2.4 3.8 4.0\"").unwrap();
        assert!(!marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_version not in \"2.7 3.2 3.3\"").unwrap();
        assert!(!marker.evaluate(&env27, &[]));
        assert!(marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_version not in \"2.7 3.7\"").unwrap();
        assert!(!marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_version not in \"2.4 3.8 4.0\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("python_full_version in \"2.7\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("implementation_version in \"2.7 3.2 3.3\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("implementation_version in \"2.7 3.7\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("implementation_version not in \"2.7 3.7\"").unwrap();
        assert!(!marker.evaluate(&env27, &[]));
        assert!(!marker.evaluate(&env37, &[]));

        let marker = MarkerTree::from_str("implementation_version not in \"2.4 3.8 4.0\"").unwrap();
        assert!(marker.evaluate(&env27, &[]));
        assert!(marker.evaluate(&env37, &[]));
    }

    #[test]
    #[cfg(feature = "tracing")]
    #[tracing_test::traced_test]
    fn warnings1() {
        let env37 = env37();
        let compare_keys = MarkerTree::from_str("platform_version == sys_platform").unwrap();
        compare_keys.evaluate(&env37, &[]);
        logs_contain(
            "Comparing two markers with each other doesn't make any sense, will evaluate to false",
        );
    }

    #[test]
    #[cfg(feature = "tracing")]
    #[tracing_test::traced_test]
    fn warnings2() {
        let env37 = env37();
        let non_pep440 = MarkerTree::from_str("python_version >= '3.9.'").unwrap();
        non_pep440.evaluate(&env37, &[]);
        logs_contain(
            "Expected PEP 440 version to compare with python_version, found `3.9.`, \
             will evaluate to false: after parsing `3.9`, found `.`, which is \
             not part of a valid version",
        );
    }

    #[test]
    #[cfg(feature = "tracing")]
    #[tracing_test::traced_test]
    fn warnings3() {
        let env37 = env37();
        let string_string = MarkerTree::from_str("'b' >= 'a'").unwrap();
        string_string.evaluate(&env37, &[]);
        logs_contain(
            "Comparing two quoted strings with each other doesn't make sense: 'b' >= 'a', will evaluate to false",
        );
    }

    #[test]
    #[cfg(feature = "tracing")]
    #[tracing_test::traced_test]
    fn warnings4() {
        let env37 = env37();
        let string_string = MarkerTree::from_str(r"os.name == 'posix' and platform.machine == 'x86_64' and platform.python_implementation == 'CPython' and 'Ubuntu' in platform.version and sys.platform == 'linux'").unwrap();
        string_string.evaluate(&env37, &[]);
        logs_assert(|lines: &[&str]| {
            let lines: Vec<_> = lines
                .iter()
                .map(|s| s.split_once("  ").unwrap().1)
                .collect();
            let expected = [
                "WARN warnings4: uv_pep508: os.name is deprecated in favor of os_name",
                "WARN warnings4: uv_pep508: platform.machine is deprecated in favor of platform_machine",
                "WARN warnings4: uv_pep508: platform.python_implementation is deprecated in favor of platform_python_implementation",
                "WARN warnings4: uv_pep508: platform.version is deprecated in favor of platform_version",
                "WARN warnings4: uv_pep508: sys.platform is deprecated in favor of sys_platform",
                "WARN warnings4: uv_pep508: Comparing linux and posix lexicographically",
            ];
            if lines == expected {
                Ok(())
            } else {
                Err(format!("{lines:#?}"))
            }
        });
    }

    #[test]
    fn test_not_in() {
        MarkerTree::from_str("'posix' not in os_name").unwrap();
    }

    #[test]
    fn test_marker_version_inverted() {
        let env37 = env37();
        let result = MarkerTree::from_str("python_version > '3.6'")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(result);

        let result = MarkerTree::from_str("'3.6' > python_version")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(!result);

        // Meaningless expressions are ignored, so this is always true.
        let result = MarkerTree::from_str("'3.*' == python_version")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(result);
    }

    #[test]
    fn test_marker_string_inverted() {
        let env37 = env37();
        let result = MarkerTree::from_str("'nux' in sys_platform")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(result);

        let result = MarkerTree::from_str("sys_platform in 'nux'")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(!result);
    }

    #[test]
    fn test_marker_version_star() {
        let env37 = env37();
        let result = MarkerTree::from_str("python_version == '3.7.*'")
            .unwrap()
            .evaluate(&env37, &[]);
        assert!(result);
    }

    #[test]
    fn test_tilde_equal() {
        let env37 = env37();
        let result = MarkerTree::from_str("python_version ~= '3.7'")
            .unwrap()
            .evaluate(&env37, &[]);
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
            MarkerExpression::from_str(r#"os_name == "nt""#)
                .unwrap()
                .unwrap(),
            MarkerExpression::String {
                key: MarkerValueString::OsName,
                operator: MarkerOperator::Equal,
                value: arcstr::literal!("nt")
            }
        );
    }

    #[test]
    fn test_marker_expression_inverted() {
        assert_eq!(
            MarkerTree::from_str(
                r#""nt" in os_name and '3.7' >= python_version and python_full_version >= '3.7'"#
            )
            .unwrap()
            .contents()
            .unwrap()
            .to_string(),
            "python_full_version == '3.7.*' and 'nt' in os_name",
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
        let expected = MarkerTree::from_str(r#"os_name == "nt""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" or extra == "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"os_name == "nt" or extra == "dev""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, MarkerTree::TRUE);

        // Given `extra == "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"extra == "dev""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, MarkerTree::TRUE);

        // Given `extra == "dev" and extra == "test"`, simplify to `extra == "test"`.
        let markers = MarkerTree::from_str(r#"extra == "dev" and extra == "test""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected = MarkerTree::from_str(r#"extra == "test""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" and extra == "test"`, don't simplify.
        let markers = MarkerTree::from_str(r#"os_name == "nt" and extra == "test""#).unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, markers);

        // Given `os_name == "nt" and (python_version == "3.7" or extra == "dev")`, simplify to
        // `os_name == "nt".
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" and (python_version == "3.7" or extra == "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected = MarkerTree::from_str(r#"os_name == "nt""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" or (python_version == "3.7" and extra == "dev")`, simplify to
        // `os_name == "nt" or python_version == "3.7"`.
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" or (python_version == "3.7" and extra == "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected =
            MarkerTree::from_str(r#"os_name == "nt" or python_version == "3.7""#).unwrap();
        assert_eq!(simplified, expected);
    }

    #[test]
    fn test_simplify_not_extras() {
        // Given `os_name == "nt" and extra != "dev"`, simplify to `os_name == "nt"`.
        let markers = MarkerTree::from_str(r#"os_name == "nt" and extra != "dev""#).unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected = MarkerTree::from_str(r#"os_name == "nt""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" or extra != "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"os_name == "nt" or extra != "dev""#).unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, MarkerTree::TRUE);

        // Given `extra != "dev"`, remove the marker entirely.
        let markers = MarkerTree::from_str(r#"extra != "dev""#).unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, MarkerTree::TRUE);

        // Given `extra != "dev" and extra != "test"`, simplify to `extra != "test"`.
        let markers = MarkerTree::from_str(r#"extra != "dev" and extra != "test""#).unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected = MarkerTree::from_str(r#"extra != "test""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" and extra != "test"`, don't simplify.
        let markers = MarkerTree::from_str(r#"os_name != "nt" and extra != "test""#).unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        assert_eq!(simplified, markers);

        // Given `os_name == "nt" and (python_version == "3.7" or extra != "dev")`, simplify to
        // `os_name == "nt".
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" and (python_version == "3.7" or extra != "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected = MarkerTree::from_str(r#"os_name == "nt""#).unwrap();
        assert_eq!(simplified, expected);

        // Given `os_name == "nt" or (python_version == "3.7" and extra != "dev")`, simplify to
        // `os_name == "nt" or python_version == "3.7"`.
        let markers = MarkerTree::from_str(
            r#"os_name == "nt" or (python_version == "3.7" and extra != "dev")"#,
        )
        .unwrap();
        let simplified = markers.simplify_not_extras(&[ExtraName::from_str("dev").unwrap()]);
        let expected =
            MarkerTree::from_str(r#"os_name == "nt" or python_version == "3.7""#).unwrap();
        assert_eq!(simplified, expected);
    }

    #[test]
    fn test_marker_simplification() {
        assert_false("python_version == '3.9.1'");
        assert_true("python_version != '3.9.1'");

        // This is an edge case that happens to be supported, but is not critical to support.
        assert_simplifies(
            "python_version in '3.9.0'",
            "python_full_version == '3.9.*'",
        );
        // e.g., using a version that is not PEP 440 compliant is considered arbitrary
        assert_true("python_version in 'foo'");
        // e.g., including `*` versions, which would require tracking a version specifier
        assert_true("python_version in '3.9.*'");
        // e.g., when non-whitespace separators are present
        assert_true("python_version in '3.9, 3.10'");
        assert_true("python_version in '3.9,3.10'");
        assert_true("python_version in '3.9 or 3.10'");

        // This is an edge case that happens to be supported, but is not critical to support.
        assert_simplifies(
            "python_version in '3.9 3.10.0 3.11'",
            "python_full_version >= '3.9' and python_full_version < '3.12'",
        );

        assert_simplifies("python_version == '3.9'", "python_full_version == '3.9.*'");
        assert_simplifies(
            "python_version == '3.9.0'",
            "python_full_version == '3.9.*'",
        );
        assert_simplifies(
            "python_version == '3.9.0.*'",
            "python_full_version == '3.9.*'",
        );
        assert_simplifies(
            "python_version == '3.*'",
            "python_full_version >= '3' and python_full_version < '4'",
        );

        // `<version> in`
        // e.g., when the range is not contiguous
        assert_simplifies(
            "python_version in '3.9 3.11'",
            "python_full_version == '3.9.*' or python_full_version == '3.11.*'",
        );
        // e.g., when the range is contiguous
        assert_simplifies(
            "python_version in '3.9 3.10 3.11'",
            "python_full_version >= '3.9' and python_full_version < '3.12'",
        );
        // e.g., with `implementation_version` instead of `python_version`
        assert_simplifies(
            "implementation_version in '3.9 3.11'",
            "implementation_version == '3.9' or implementation_version == '3.11'",
        );

        // '<version> not in'
        // e.g., when the range is not contiguous
        assert_simplifies(
            "python_version not in '3.9 3.11'",
            "python_full_version < '3.9' or python_full_version == '3.10.*' or python_full_version >= '3.12'",
        );
        // e.g, when the range is contiguous
        assert_simplifies(
            "python_version not in '3.9 3.10 3.11'",
            "python_full_version < '3.9' or python_full_version >= '3.12'",
        );
        // e.g., with `implementation_version` instead of `python_version`
        assert_simplifies(
            "implementation_version not in '3.9 3.11'",
            "implementation_version != '3.9' and implementation_version != '3.11'",
        );

        assert_simplifies("python_version != '3.9'", "python_full_version != '3.9.*'");

        assert_simplifies("python_version >= '3.9.0'", "python_full_version >= '3.9'");
        assert_simplifies("python_version <= '3.9.0'", "python_full_version < '3.10'");

        assert_simplifies(
            "python_version == '3.*'",
            "python_full_version >= '3' and python_full_version < '4'",
        );
        assert_simplifies(
            "python_version == '3.0.*'",
            "python_full_version == '3.0.*'",
        );

        assert_simplifies(
            "python_version < '3.17' or python_version < '3.18'",
            "python_full_version < '3.18'",
        );

        assert_simplifies(
            "python_version > '3.17' or python_version > '3.18' or python_version > '3.12'",
            "python_full_version >= '3.13'",
        );

        // a quirk of how pubgrub works, but this is considered part of normalization
        assert_simplifies(
            "python_version > '3.17.post4' or python_version > '3.18.post4'",
            "python_full_version >= '3.18'",
        );

        assert_simplifies(
            "python_version < '3.17' and python_version < '3.18'",
            "python_full_version < '3.17'",
        );

        assert_simplifies(
            "python_version <= '3.18' and python_version == '3.18'",
            "python_full_version == '3.18.*'",
        );

        assert_simplifies(
            "python_version <= '3.18' or python_version == '3.18'",
            "python_full_version < '3.19'",
        );

        assert_simplifies(
            "python_version <= '3.15' or (python_version <= '3.17' and python_version < '3.16')",
            "python_full_version < '3.16'",
        );

        assert_simplifies(
            "(python_version > '3.17' or python_version > '3.16') and python_version > '3.15'",
            "python_full_version >= '3.17'",
        );

        assert_simplifies(
            "(python_version > '3.17' or python_version > '3.16') and python_version > '3.15' and implementation_version == '1'",
            "implementation_version == '1' and python_full_version >= '3.17'",
        );

        assert_simplifies(
            "('3.17' < python_version or '3.16' < python_version) and '3.15' < python_version and implementation_version == '1'",
            "implementation_version == '1' and python_full_version >= '3.17'",
        );

        assert_simplifies("extra == 'a' or extra == 'a'", "extra == 'a'");
        assert_simplifies(
            "extra == 'a' and extra == 'a' or extra == 'b'",
            "extra == 'a' or extra == 'b'",
        );

        assert!(m("python_version < '3.17' and '3.18' == python_version").is_false());

        // flatten nested expressions
        assert_simplifies(
            "((extra == 'a' and extra == 'b') and extra == 'c') and extra == 'b'",
            "extra == 'a' and extra == 'b' and extra == 'c'",
        );

        assert_simplifies(
            "((extra == 'a' or extra == 'b') or extra == 'c') or extra == 'b'",
            "extra == 'a' or extra == 'b' or extra == 'c'",
        );

        // complex expressions
        assert_simplifies(
            "extra == 'a' or (extra == 'a' and extra == 'b')",
            "extra == 'a'",
        );

        assert_simplifies(
            "extra == 'a' and (extra == 'a' or extra == 'b')",
            "extra == 'a'",
        );

        assert_simplifies(
            "(extra == 'a' and (extra == 'a' or extra == 'b')) or extra == 'd'",
            "extra == 'a' or extra == 'd'",
        );

        assert_simplifies(
            "((extra == 'a' and extra == 'b') or extra == 'c') or extra == 'b'",
            "extra == 'b' or extra == 'c'",
        );

        assert_simplifies(
            "((extra == 'a' or extra == 'b') and extra == 'c') and extra == 'b'",
            "extra == 'b' and extra == 'c'",
        );

        assert_simplifies(
            "((extra == 'a' or extra == 'b') and extra == 'c') or extra == 'b'",
            "extra == 'b' or (extra == 'a' and extra == 'c')",
        );

        // post-normalization filtering
        assert_simplifies(
            "(python_version < '3.1' or python_version < '3.2') and (python_version < '3.2' or python_version == '3.3')",
            "python_full_version < '3.2'",
        );

        // normalize out redundant ranges
        assert_true("python_version < '3.12.0rc1' or python_version >= '3.12.0rc1'");

        assert_true(
            "extra == 'a' or (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
        );

        assert_simplifies(
            "extra == 'a' and (python_version < '3.12.0rc1' or python_version >= '3.12.0rc1')",
            "extra == 'a'",
        );

        // normalize `!=` operators
        assert_true("python_version != '3.10' or python_version < '3.12'");

        assert_simplifies(
            "python_version != '3.10' or python_version > '3.12'",
            "python_full_version != '3.10.*'",
        );

        assert_simplifies(
            "python_version != '3.8' and python_version < '3.10'",
            "python_full_version < '3.8' or python_full_version == '3.9.*'",
        );

        assert_simplifies(
            "python_version != '3.8' and python_version != '3.9'",
            "python_full_version < '3.8' or python_full_version >= '3.10'",
        );

        // normalize out redundant expressions
        assert_true("sys_platform == 'win32' or sys_platform != 'win32'");

        assert_true("'win32' == sys_platform or sys_platform != 'win32'");

        assert_true(
            "sys_platform == 'win32' or sys_platform == 'win32' or sys_platform != 'win32'",
        );

        assert!(m("sys_platform == 'win32' and sys_platform != 'win32'").is_false());
    }

    /// This tests the difference between simplifying extras and simplifying
    /// other stringly-valued marker attributes.
    ///
    /// In particular, this demonstrates that `extra != "foo"` would actually
    /// be more clearly written as `"foo" not in extras`. And similarly, `extra
    /// == "foo"` would be written as `"foo" in extras`. This is different from
    /// other attributes, like `sys_platform`, where the test is just string
    /// equality. That is, `extra` is a set where as `sys_platform` is just a
    /// single value.
    #[test]
    fn test_simplification_extra_versus_other() {
        // Here, the `extra != 'foo'` cannot be simplified out, because
        // `extra == 'foo'` can be true even when `extra == 'bar'`' is true.
        assert_simplifies(
            r#"extra != "foo" and (extra == "bar" or extra == "baz")"#,
            "(extra == 'bar' and extra != 'foo') or (extra == 'baz' and extra != 'foo')",
        );
        // But here, the `sys_platform != 'foo'` can be simplified out, because
        // it is strictly disjoint with
        // `sys_platform == "bar" or sys_platform == "baz"`.
        assert_simplifies(
            r#"sys_platform != "foo" and (sys_platform == "bar" or sys_platform == "baz")"#,
            "sys_platform == 'bar' or sys_platform == 'baz'",
        );

        // Another case I experimented with and wanted to verify.
        assert_simplifies(
            r#"(extra != "bar" and (extra == "foo" or extra == "baz"))
            or ((extra != "foo" and extra != "bar") and extra == "baz")"#,
            "(extra != 'bar' and extra == 'baz') or (extra != 'bar' and extra == 'foo')",
        );
    }

    #[test]
    fn test_python_version_equal_star() {
        // Input, equivalent with python_version, equivalent with python_full_version
        let cases = [
            ("3.*", "3.*", "3.*"),
            ("3.0.*", "3.0", "3.0.*"),
            ("3.0.0.*", "3.0", "3.0.*"),
            ("3.9.*", "3.9", "3.9.*"),
            ("3.9.0.*", "3.9", "3.9.*"),
            ("3.9.0.0.*", "3.9", "3.9.*"),
        ];
        for (input, equal_python_version, equal_python_full_version) in cases {
            assert_eq!(
                m(&format!("python_version == '{input}'")),
                m(&format!("python_version == '{equal_python_version}'")),
                "{input} {equal_python_version}"
            );
            assert_eq!(
                m(&format!("python_version == '{input}'")),
                m(&format!(
                    "python_full_version == '{equal_python_full_version}'"
                )),
                "{input} {equal_python_full_version}"
            );
        }

        let cases_false = ["3.9.1.*", "3.9.1.0.*", "3.9.1.0.0.*"];
        for input in cases_false {
            assert!(
                m(&format!("python_version == '{input}'")).is_false(),
                "{input}"
            );
        }
    }

    #[test]
    fn test_tilde_equal_normalization() {
        assert_eq!(
            m("python_version ~= '3.10.0'"),
            m("python_version >= '3.10.0' and python_version < '3.11.0'")
        );

        // Two digit versions such as `python_version` get padded with a zero, so they can never
        // match
        assert_eq!(m("python_version ~= '3.10.1'"), MarkerTree::FALSE);

        assert_eq!(
            m("python_version ~= '3.10'"),
            m("python_version >= '3.10' and python_version < '4.0'")
        );

        assert_eq!(
            m("python_full_version ~= '3.10.0'"),
            m("python_full_version >= '3.10.0' and python_full_version < '3.11.0'")
        );

        assert_eq!(
            m("python_full_version ~= '3.10'"),
            m("python_full_version >= '3.10' and python_full_version < '4.0'")
        );
    }

    /// This tests marker implication.
    ///
    /// Specifically, these test cases come from a [bug] where `foo` and `bar`
    /// are conflicting extras, but results in an ambiguous lock file. This
    /// test takes `not(extra == 'foo' and extra == 'bar')` as world knowledge.
    /// That is, since they are declared as conflicting, they cannot both be
    /// true. This is in turn used to determine whether particular markers are
    /// implied by this world knowledge and thus can be removed.
    ///
    /// [bug]: <https://github.com/astral-sh/uv/issues/9289>
    #[test]
    fn test_extra_implication() {
        assert!(implies(
            "extra != 'foo' or extra != 'bar'",
            "extra != 'foo' or extra != 'bar'",
        ));
        assert!(!implies(
            "extra != 'foo' or extra != 'bar'",
            "extra != 'foo'",
        ));
        assert!(!implies(
            "extra != 'foo' or extra != 'bar'",
            "extra != 'bar' and extra == 'foo'",
        ));

        // This simulates the case when multiple groups of conflicts
        // are combined into one "world knowledge" marker.
        assert!(implies(
            "(extra != 'foo' or extra != 'bar') and (extra != 'baz' or extra != 'quux')",
            "extra != 'foo' or extra != 'bar'",
        ));
        assert!(!implies(
            "(extra != 'foo' or extra != 'bar') and (extra != 'baz' or extra != 'quux')",
            "extra != 'foo'",
        ));
        assert!(!implies(
            "(extra != 'foo' or extra != 'bar') and (extra != 'baz' or extra != 'quux')",
            "extra != 'bar' and extra == 'foo'",
        ));
    }

    #[test]
    fn test_marker_negation() {
        assert_eq!(
            m("python_version > '3.6'").negate(),
            m("python_version <= '3.6'")
        );

        assert_eq!(
            m("'3.6' < python_version").negate(),
            m("python_version <= '3.6'")
        );

        assert_eq!(
            m("python_version != '3.6' and os_name == 'Linux'").negate(),
            m("python_version == '3.6' or os_name != 'Linux'")
        );

        assert_eq!(
            m("python_version == '3.6' and os_name != 'Linux'").negate(),
            m("python_version != '3.6' or os_name == 'Linux'")
        );

        assert_eq!(
            m("python_version != '3.6.*' and os_name == 'Linux'").negate(),
            m("python_version == '3.6.*' or os_name != 'Linux'")
        );

        assert_eq!(
            m("python_version == '3.6.*'").negate(),
            m("python_version != '3.6.*'")
        );
        assert_eq!(
            m("python_version != '3.6.*'").negate(),
            m("python_version == '3.6.*'")
        );

        assert_eq!(
            m("python_version ~= '3.6'").negate(),
            m("python_version < '3.6' or python_version != '3.*'")
        );
        assert_eq!(
            m("'3.6' ~= python_version").negate(),
            m("python_version < '3.6' or python_version != '3.*'")
        );
        assert_eq!(
            m("python_version ~= '3.6.2'").negate(),
            m("python_version < '3.6.2' or python_version != '3.6.*'")
        );

        assert_eq!(
            m("sys_platform == 'linux'").negate(),
            m("sys_platform != 'linux'")
        );
        assert_eq!(
            m("'linux' == sys_platform").negate(),
            m("sys_platform != 'linux'")
        );

        // ~= is nonsense on string markers, so the markers is ignored and always
        // evaluates to true. Thus the negation always returns false.
        assert_eq!(m("sys_platform ~= 'linux'").negate(), MarkerTree::FALSE);

        // As above, arbitrary exprs remain arbitrary.
        assert_eq!(m("'foo' == 'bar'").negate(), MarkerTree::FALSE);

        // Conjunctions
        assert_eq!(
            m("os_name == 'bar' and os_name == 'foo'").negate(),
            m("os_name != 'bar' or os_name != 'foo'")
        );
        // Disjunctions
        assert_eq!(
            m("os_name == 'bar' or os_name == 'foo'").negate(),
            m("os_name != 'bar' and os_name != 'foo'")
        );

        // Always true negates to always false!
        assert_eq!(
            m("python_version >= '3.6' or python_version < '3.6'").negate(),
            m("python_version < '3.6' and python_version >= '3.6'")
        );
    }

    #[test]
    fn test_complex_marker_simplification() {
        // This expression should simplify to:
        // `(implementation_name == 'pypy' and sys_platform != 'win32')
        //   or (sys_platform == 'win32' or os_name != 'nt')
        //   or (implementation != 'pypy' or os_name == 'nt')`
        //
        // However, simplifying this expression is NP-complete and requires an exponential
        // algorithm such as Quine-McCluskey, which is not currently implemented.
        assert_simplifies(
            "(implementation_name == 'pypy' and sys_platform != 'win32')
                or (implementation_name != 'pypy' and sys_platform == 'win32')
                or (sys_platform == 'win32' and os_name != 'nt')
                or (sys_platform != 'win32' and os_name == 'nt')",
            "(implementation_name == 'pypy' and sys_platform != 'win32') \
                or (implementation_name != 'pypy' and sys_platform == 'win32') \
                or (os_name != 'nt' and sys_platform == 'win32') \
                or (os_name == 'nt' and sys_platform != 'win32')",
        );

        // This is a case we can simplify fully, but it's dependent on the variable order.
        // The expression is equivalent to `sys_platform == 'x' or (os_name == 'Linux' and platform_system == 'win32')`.
        assert_simplifies(
            "(os_name == 'Linux' and platform_system == 'win32')
                or (os_name == 'Linux' and platform_system == 'win32' and sys_platform == 'a')
                or (os_name == 'Linux' and platform_system == 'win32' and sys_platform == 'x')
                or (os_name != 'Linux' and platform_system == 'win32' and sys_platform == 'x')
                or (os_name == 'Linux' and platform_system != 'win32' and sys_platform == 'x')
                or (os_name != 'Linux' and platform_system != 'win32' and sys_platform == 'x')",
            "(os_name == 'Linux' and platform_system == 'win32') or sys_platform == 'x'",
        );

        assert_simplifies("python_version > '3.7'", "python_full_version >= '3.8'");

        assert_simplifies(
            "(python_version <= '3.7' and os_name == 'Linux') or python_version > '3.7'",
            "python_full_version >= '3.8' or os_name == 'Linux'",
        );

        // Again, the extra `<3.7` and `>=3.9` expressions cannot be seen as redundant due to them being interdependent.
        // TODO(ibraheem): We might be able to simplify these by checking for the negation of the combined ranges before we split them.
        assert_simplifies(
            "(os_name == 'Linux' and sys_platform == 'win32') \
                or (os_name != 'Linux' and sys_platform == 'win32' and python_version == '3.7') \
                or (os_name != 'Linux' and sys_platform == 'win32' and python_version == '3.8')",
            "(python_full_version >= '3.7' and python_full_version < '3.9' and sys_platform == 'win32') or (os_name == 'Linux' and sys_platform == 'win32')",
        );

        assert_simplifies(
            "(implementation_name != 'pypy' and os_name == 'nt' and sys_platform == 'darwin') or (os_name == 'nt' and sys_platform == 'win32')",
            "os_name == 'nt' and sys_platform == 'win32'",
        );

        assert_simplifies(
            "(sys_platform == 'darwin' or sys_platform == 'win32') and ((implementation_name != 'pypy' and os_name == 'nt' and sys_platform == 'darwin') or (os_name == 'nt' and sys_platform == 'win32'))",
            "os_name == 'nt' and sys_platform == 'win32'",
        );

        assert_simplifies(
            "(sys_platform == 'darwin' or sys_platform == 'win32')
                and ((platform_version != '1' and os_name == 'nt' and sys_platform == 'darwin') or (os_name == 'nt' and sys_platform == 'win32'))",
            "os_name == 'nt' and sys_platform == 'win32'",
        );

        assert_simplifies(
            "(os_name == 'nt' and sys_platform == 'win32') \
                or (os_name != 'nt' and platform_version == '1' and (sys_platform == 'win32' or sys_platform == 'win64'))",
            "(os_name != 'nt' and platform_version == '1' and sys_platform == 'win64') \
                or (os_name == 'nt' and sys_platform == 'win32') \
                or (platform_version == '1' and sys_platform == 'win32')",
        );

        assert_simplifies(
            "(os_name == 'nt' and sys_platform == 'win32') or (os_name != 'nt' and (sys_platform == 'win32' or sys_platform == 'win64'))",
            "(os_name != 'nt' and sys_platform == 'win64') or sys_platform == 'win32'",
        );
    }

    #[test]
    fn test_requires_python() {
        fn simplified(marker: &str) -> MarkerTree {
            let lower = Bound::Included(Version::new([3, 8]));
            let upper = Bound::Unbounded;
            m(marker).simplify_python_versions(lower.as_ref(), upper.as_ref())
        }

        assert_eq!(simplified("python_version >= '3.8'"), MarkerTree::TRUE);
        assert_eq!(
            simplified("python_version >= '3.8' or sys_platform == 'win32'"),
            MarkerTree::TRUE
        );

        assert_eq!(
            simplified("python_version >= '3.8' and sys_platform == 'win32'"),
            m("sys_platform == 'win32'"),
        );

        assert_eq!(
            simplified("python_version == '3.8'")
                .try_to_string()
                .unwrap(),
            "python_full_version < '3.9'"
        );

        assert_eq!(
            simplified("python_version <= '3.10'")
                .try_to_string()
                .unwrap(),
            "python_full_version < '3.11'"
        );
    }

    #[test]
    fn test_extra_disjointness() {
        assert!(!is_disjoint("extra == 'a'", "python_version == '1'"));

        assert!(!is_disjoint("extra == 'a'", "extra == 'a'"));
        assert!(!is_disjoint("extra == 'a'", "extra == 'b'"));
        assert!(!is_disjoint("extra == 'b'", "extra == 'a'"));
        assert!(!is_disjoint("extra == 'b'", "extra != 'a'"));
        assert!(!is_disjoint("extra != 'b'", "extra == 'a'"));
        assert!(is_disjoint("extra != 'b'", "extra == 'b'"));
        assert!(is_disjoint("extra == 'b'", "extra != 'b'"));
    }

    #[test]
    fn test_arbitrary_disjointness() {
        // `python_version == 'Linux'` is nonsense and ignored, thus the first marker
        // is always `true` and not disjoint.
        assert!(!is_disjoint(
            "python_version == 'Linux'",
            "python_full_version == '3.7.1'"
        ));
    }

    #[test]
    fn test_version_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "python_full_version == '3.7.1'"
        ));

        test_version_bounds_disjointness("python_full_version");

        assert!(!is_disjoint(
            "python_full_version == '3.7.*'",
            "python_full_version == '3.7.1'"
        ));

        assert!(is_disjoint(
            "python_version == '3.7'",
            "python_full_version == '3.8'"
        ));

        assert!(!is_disjoint(
            "python_version == '3.7'",
            "python_full_version == '3.7.2'"
        ));

        assert!(is_disjoint(
            "python_version > '3.7'",
            "python_full_version == '3.7.1'"
        ));

        assert!(!is_disjoint(
            "python_version <= '3.7'",
            "python_full_version == '3.7.1'"
        ));
    }

    #[test]
    fn test_string_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'Linux'",
            "platform_version == '3.7.1'"
        ));
        assert!(!is_disjoint(
            "implementation_version == '3.7.0'",
            "python_full_version == '3.7.1'"
        ));

        // basic version bounds checking should still work with lexicographical comparisons
        test_version_bounds_disjointness("platform_version");

        assert!(is_disjoint("os_name == 'Linux'", "os_name == 'OSX'"));
        assert!(is_disjoint("os_name <= 'Linux'", "os_name == 'OSX'"));

        assert!(!is_disjoint(
            "os_name in 'OSXLinuxWindows'",
            "os_name == 'OSX'"
        ));
        assert!(!is_disjoint("'OSX' in os_name", "'Linux' in os_name"));

        // complicated `in` intersections are not supported
        assert!(!is_disjoint("os_name in 'OSX'", "os_name in 'Linux'"));
        assert!(!is_disjoint(
            "os_name in 'OSXLinux'",
            "os_name == 'Windows'"
        ));

        assert!(is_disjoint(
            "os_name in 'Windows'",
            "os_name not in 'Windows'"
        ));
        assert!(is_disjoint(
            "'Windows' in os_name",
            "'Windows' not in os_name"
        ));

        assert!(!is_disjoint("'Windows' in os_name", "'Windows' in os_name"));
        assert!(!is_disjoint("'Linux' in os_name", "os_name not in 'Linux'"));
        assert!(!is_disjoint("'Linux' not in os_name", "os_name in 'Linux'"));

        assert!(!is_disjoint(
            "os_name == 'Linux' and os_name != 'OSX'",
            "os_name == 'Linux'"
        ));
        assert!(is_disjoint(
            "os_name == 'Linux' and os_name != 'OSX'",
            "os_name == 'OSX'"
        ));

        assert!(!is_disjoint(
            "extra == 'Linux' and extra != 'OSX'",
            "extra == 'Linux'"
        ));
        assert!(is_disjoint(
            "extra == 'Linux' and extra != 'OSX'",
            "extra == 'OSX'"
        ));

        assert!(!is_disjoint(
            "extra == 'x1' and extra != 'x2'",
            "extra == 'x1'"
        ));
        assert!(is_disjoint(
            "extra == 'x1' and extra != 'x2'",
            "extra == 'x2'"
        ));
    }

    #[test]
    fn is_disjoint_commutative() {
        let m1 = m("extra == 'Linux' and extra != 'OSX'");
        let m2 = m("extra == 'Linux'");
        assert!(!m2.is_disjoint(m1));
        assert!(!m1.is_disjoint(m2));
    }

    #[test]
    fn test_combined_disjointness() {
        assert!(!is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "os_name == 'a'"
        ));
        assert!(!is_disjoint(
            "os_name == 'a' or platform_version == '1'",
            "os_name == 'a'"
        ));

        assert!(is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "os_name == 'a' and platform_version == '2'"
        ));
        assert!(is_disjoint(
            "os_name == 'a' and platform_version == '1'",
            "'2' == platform_version and os_name == 'a'"
        ));
        assert!(!is_disjoint(
            "os_name == 'a' or platform_version == '1'",
            "os_name == 'a' or platform_version == '2'"
        ));

        assert!(is_disjoint(
            "sys_platform == 'darwin' and implementation_name == 'pypy'",
            "sys_platform == 'bar' or implementation_name == 'foo'",
        ));
        assert!(is_disjoint(
            "sys_platform == 'bar' or implementation_name == 'foo'",
            "sys_platform == 'darwin' and implementation_name == 'pypy'",
        ));

        assert!(is_disjoint(
            "python_version >= '3.7' and implementation_name == 'pypy'",
            "python_version < '3.7'"
        ));
        assert!(is_disjoint(
            "implementation_name == 'pypy' and python_version >= '3.7'",
            "implementation_name != 'pypy'"
        ));
        assert!(is_disjoint(
            "implementation_name != 'pypy' and python_version >= '3.7'",
            "implementation_name == 'pypy'"
        ));
    }

    #[test]
    fn test_arbitrary() {
        assert!(m("'wat' == 'wat'").is_true());
        assert!(m("os_name ~= 'wat'").is_true());
        assert!(m("python_version == 'Linux'").is_true());
        assert!(m("os_name ~= 'wat' or 'wat' == 'wat' and python_version == 'Linux'").is_true());
    }

    #[test]
    fn test_is_false() {
        assert!(m("python_version < '3.10' and python_version >= '3.10'").is_false());
        assert!(
            m("(python_version < '3.10' and python_version >= '3.10') \
              or (python_version < '3.9' and python_version >= '3.9')")
            .is_false()
        );

        assert!(!m("python_version < '3.10'").is_false());
        assert!(!m("python_version < '0'").is_false());
        assert!(!m("python_version < '3.10' and python_version >= '3.9'").is_false());
        assert!(!m("python_version < '3.10' or python_version >= '3.11'").is_false());
    }

    fn test_version_bounds_disjointness(version: &str) {
        assert!(!is_disjoint(
            format!("{version} > '2.7.0'"),
            format!("{version} == '3.6.0'")
        ));
        assert!(!is_disjoint(
            format!("{version} >= '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));
        assert!(!is_disjoint(
            format!("{version} >= '3.7.0'"),
            format!("'3.7.1' == {version}")
        ));

        assert!(is_disjoint(
            format!("{version} >= '3.7.1'"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("'3.7.1' <= {version}"),
            format!("{version} == '3.7.0'")
        ));

        assert!(is_disjoint(
            format!("{version} < '3.7.0'"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("'3.7.0' > {version}"),
            format!("{version} == '3.7.0'")
        ));
        assert!(is_disjoint(
            format!("{version} < '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));

        assert!(is_disjoint(
            format!("{version} == '3.7.0'"),
            format!("{version} == '3.7.1'")
        ));
        assert!(is_disjoint(
            format!("{version} == '3.7.0'"),
            format!("{version} != '3.7.0'")
        ));
    }

    fn assert_simplifies(left: &str, right: &str) {
        assert_eq!(m(left), m(right), "{left} != {right}");
        assert_eq!(m(left).try_to_string().unwrap(), right, "{left} != {right}");
    }

    fn assert_true(marker: &str) {
        assert!(m(marker).is_true(), "{marker} != true");
    }

    fn assert_false(marker: &str) {
        assert!(m(marker).is_false(), "{marker} != false");
    }

    fn is_disjoint(left: impl AsRef<str>, right: impl AsRef<str>) -> bool {
        let (left, right) = (m(left.as_ref()), m(right.as_ref()));
        left.is_disjoint(right) && right.is_disjoint(left)
    }

    fn implies(antecedent: &str, consequent: &str) -> bool {
        let mut marker = m(antecedent);
        marker.implies(m(consequent));
        marker.is_true()
    }

    #[test]
    fn complexified_markers() {
        // Takes optional lower (inclusive) and upper (exclusive)
        // bounds representing `requires-python` and a "simplified"
        // marker, and returns the "complexified" marker. That is, a
        // marker that embeds the `requires-python` constraint into it.
        let complexify =
            |lower: Option<[u64; 2]>, upper: Option<[u64; 2]>, marker: &str| -> MarkerTree {
                let lower = lower
                    .map(|release| Bound::Included(Version::new(release)))
                    .unwrap_or(Bound::Unbounded);
                let upper = upper
                    .map(|release| Bound::Excluded(Version::new(release)))
                    .unwrap_or(Bound::Unbounded);
                m(marker).complexify_python_versions(lower.as_ref(), upper.as_ref())
            };

        assert_eq!(
            complexify(None, None, "python_full_version < '3.10'"),
            m("python_full_version < '3.10'"),
        );
        assert_eq!(
            complexify(Some([3, 8]), None, "python_full_version < '3.10'"),
            m("python_full_version >= '3.8' and python_full_version < '3.10'"),
        );
        assert_eq!(
            complexify(None, Some([3, 8]), "python_full_version < '3.10'"),
            m("python_full_version < '3.8'"),
        );
        assert_eq!(
            complexify(Some([3, 8]), Some([3, 8]), "python_full_version < '3.10'"),
            // Kinda weird, but this normalizes to `false`, just like the above.
            m("python_full_version < '0' and python_full_version > '0'"),
        );

        assert_eq!(
            complexify(Some([3, 11]), None, "python_full_version < '3.10'"),
            // Kinda weird, but this normalizes to `false`, just like the above.
            m("python_full_version < '0' and python_full_version > '0'"),
        );
        assert_eq!(
            complexify(Some([3, 11]), None, "python_full_version >= '3.10'"),
            m("python_full_version >= '3.11'"),
        );
        assert_eq!(
            complexify(Some([3, 11]), None, "python_full_version >= '3.12'"),
            m("python_full_version >= '3.12'"),
        );

        assert_eq!(
            complexify(None, Some([3, 11]), "python_full_version > '3.12'"),
            // Kinda weird, but this normalizes to `false`, just like the above.
            m("python_full_version < '0' and python_full_version > '0'"),
        );
        assert_eq!(
            complexify(None, Some([3, 11]), "python_full_version <= '3.12'"),
            m("python_full_version < '3.11'"),
        );
        assert_eq!(
            complexify(None, Some([3, 11]), "python_full_version <= '3.10'"),
            m("python_full_version <= '3.10'"),
        );

        assert_eq!(
            complexify(Some([3, 11]), None, "python_full_version == '3.8'"),
            // Kinda weird, but this normalizes to `false`, just like the above.
            m("python_full_version < '0' and python_full_version > '0'"),
        );
        assert_eq!(
            complexify(
                Some([3, 11]),
                None,
                "python_full_version == '3.8' or python_full_version == '3.12'"
            ),
            m("python_full_version == '3.12'"),
        );
        assert_eq!(
            complexify(
                Some([3, 11]),
                None,
                "python_full_version == '3.8' \
                 or python_full_version == '3.11' \
                 or python_full_version == '3.12'"
            ),
            m("python_full_version == '3.11' or python_full_version == '3.12'"),
        );

        // Tests a tricky case where if a marker is always true, then
        // complexifying it will proceed correctly by adding the
        // requires-python constraint. This is a regression test for
        // an early implementation that special cased the "always
        // true" case to return "always true" regardless of the
        // requires-python bounds.
        assert_eq!(
            complexify(
                Some([3, 12]),
                None,
                "python_full_version < '3.10' or python_full_version >= '3.10'"
            ),
            m("python_full_version >= '3.12'"),
        );
    }

    #[test]
    fn simplified_markers() {
        // Takes optional lower (inclusive) and upper (exclusive)
        // bounds representing `requires-python` and a "complexified"
        // marker, and returns the "simplified" marker. That is, a
        // marker that assumes `requires-python` is true.
        let simplify =
            |lower: Option<[u64; 2]>, upper: Option<[u64; 2]>, marker: &str| -> MarkerTree {
                let lower = lower
                    .map(|release| Bound::Included(Version::new(release)))
                    .unwrap_or(Bound::Unbounded);
                let upper = upper
                    .map(|release| Bound::Excluded(Version::new(release)))
                    .unwrap_or(Bound::Unbounded);
                m(marker).simplify_python_versions(lower.as_ref(), upper.as_ref())
            };

        assert_eq!(
            simplify(
                Some([3, 8]),
                None,
                "python_full_version >= '3.8' and python_full_version < '3.10'"
            ),
            m("python_full_version < '3.10'"),
        );
        assert_eq!(
            simplify(Some([3, 8]), None, "python_full_version < '3.7'"),
            // Kinda weird, but this normalizes to `false`, just like the above.
            m("python_full_version < '0' and python_full_version > '0'"),
        );
        assert_eq!(
            simplify(
                Some([3, 8]),
                Some([3, 11]),
                "python_full_version == '3.7.*' \
                 or python_full_version == '3.8.*' \
                 or python_full_version == '3.10.*' \
                 or python_full_version == '3.11.*' \
                "
            ),
            // Given `requires-python = '>=3.8,<3.11'`, only `3.8.*`
            // and `3.10.*` can possibly be true. So this simplifies
            // to `!= 3.9.*`.
            m("python_full_version != '3.9.*'"),
        );
        assert_eq!(
            simplify(
                Some([3, 8]),
                None,
                "python_full_version >= '3.8' and sys_platform == 'win32'"
            ),
            m("sys_platform == 'win32'"),
        );
        assert_eq!(
            simplify(
                Some([3, 8]),
                None,
                "python_full_version >= '3.9' \
                 and (sys_platform == 'win32' or python_full_version >= '3.8')",
            ),
            m("python_full_version >= '3.9'"),
        );
    }

    #[test]
    fn without_extras() {
        assert_eq!(
            m("os_name == 'Linux'").without_extras(),
            m("os_name == 'Linux'"),
        );
        assert!(m("extra == 'foo'").without_extras().is_true());
        assert_eq!(
            m("os_name == 'Linux' and extra == 'foo'").without_extras(),
            m("os_name == 'Linux'"),
        );

        assert!(
            m("
                (os_name == 'Linux' and extra == 'foo')
                or (os_name != 'Linux' and extra == 'bar')")
            .without_extras()
            .is_true()
        );

        assert_eq!(
            m("os_name == 'Linux' and extra != 'foo'").without_extras(),
            m("os_name == 'Linux'"),
        );

        assert!(
            m("extra != 'extra-project-bar' and extra == 'extra-project-foo'")
                .without_extras()
                .is_true()
        );

        assert_eq!(
            m("(os_name == 'Darwin' and extra == 'foo') \
               or (sys_platform == 'Linux' and extra != 'foo')")
            .without_extras(),
            m("os_name == 'Darwin' or sys_platform == 'Linux'"),
        );
    }

    #[test]
    fn only_extras() {
        assert!(m("os_name == 'Linux'").only_extras().is_true());
        assert_eq!(m("extra == 'foo'").only_extras(), m("extra == 'foo'"));
        assert_eq!(
            m("os_name == 'Linux' and extra == 'foo'").only_extras(),
            m("extra == 'foo'"),
        );
        assert!(
            m("
                (os_name == 'foo' and extra == 'foo')
                or (os_name == 'bar' and extra != 'foo')")
            .only_extras()
            .is_true()
        );
        assert_eq!(
            m("
                (os_name == 'Linux' and extra == 'foo')
                or (os_name != 'Linux' and extra == 'bar')")
            .only_extras(),
            m("extra == 'foo' or extra == 'bar'"),
        );

        assert_eq!(
            m("
                (implementation_name != 'pypy' and os_name == 'nt' and sys_platform == 'darwin')
                or (os_name == 'nt' and sys_platform == 'win32')")
            .only_extras(),
            m("os_name == 'Linux' or os_name != 'Linux'"),
        );
    }

    #[test]
    fn visit_extras() {
        let collect = |m: MarkerTree| -> Vec<(&'static str, String)> {
            let mut collected = vec![];
            m.visit_extras(|op, extra| {
                let op = match op {
                    MarkerOperator::Equal => "==",
                    MarkerOperator::NotEqual => "!=",
                    _ => unreachable!(),
                };
                collected.push((op, extra.as_str().to_string()));
            });
            collected
        };
        assert_eq!(collect(m("os_name == 'Linux'")), vec![]);
        assert_eq!(
            collect(m("extra == 'foo'")),
            vec![("==", "foo".to_string())]
        );
        assert_eq!(
            collect(m("extra == 'Linux' and extra == 'foo'")),
            vec![("==", "foo".to_string()), ("==", "linux".to_string())]
        );
        assert_eq!(
            collect(m("os_name == 'Linux' and extra == 'foo'")),
            vec![("==", "foo".to_string())]
        );

        let marker = "
            python_full_version >= '3.12'
            and sys_platform == 'darwin'
            and extra == 'extra-27-resolution-markers-for-days-cpu'
            and extra != 'extra-27-resolution-markers-for-days-cu124'
        ";
        assert_eq!(
            collect(m(marker)),
            vec![
                ("==", "extra-27-resolution-markers-for-days-cpu".to_string()),
                (
                    "!=",
                    "extra-27-resolution-markers-for-days-cu124".to_string()
                ),
            ]
        );
    }

    /// Case a: There is no version `3` (no trailing zero) in the interner yet.
    #[test]
    fn marker_normalization_a() {
        let left_tree = m("python_version == '3.0.*'");
        let left = left_tree.try_to_string().unwrap();
        let right = "python_full_version == '3.0.*'";
        assert_eq!(left, right, "{left} != {right}");
    }

    /// Case b: There is already a version `3` (no trailing zero) in the interner.
    #[test]
    fn marker_normalization_b() {
        m("python_version >= '3' and python_version <= '3.0'");

        let left_tree = m("python_version == '3.0.*'");
        let left = left_tree.try_to_string().unwrap();
        let right = "python_full_version == '3.0.*'";
        assert_eq!(left, right, "{left} != {right}");
    }

    #[test]
    fn marker_normalization_c() {
        let left_tree = MarkerTree::from_str("python_version == '3.10.0.*'").unwrap();
        let left = left_tree.try_to_string().unwrap();
        let right = "python_full_version == '3.10.*'";
        assert_eq!(left, right, "{left} != {right}");
    }
}
