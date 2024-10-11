use std::cmp::Ordering;
use std::ops::Bound;
use std::str::FromStr;

use crate::{
    version, Operator, OperatorParseError, Version, VersionPattern, VersionPatternParseError,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use tracing::warn;

/// Sorted version specifiers, such as `>=2.1,<3`.
///
/// Python requirements can contain multiple version specifier so we need to store them in a list,
/// such as `>1.2,<2.0` being `[">1.2", "<2.0"]`.
///
/// ```rust
/// # use std::str::FromStr;
/// # use uv_pep440::{VersionSpecifiers, Version, Operator};
///
/// let version = Version::from_str("1.19").unwrap();
/// let version_specifiers = VersionSpecifiers::from_str(">=1.16, <2.0").unwrap();
/// assert!(version_specifiers.contains(&version));
/// // VersionSpecifiers derefs into a list of specifiers
/// assert_eq!(version_specifiers.iter().position(|specifier| *specifier.operator() == Operator::LessThan), Some(1));
/// ```
#[derive(
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Debug,
    Clone,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct VersionSpecifiers(Vec<VersionSpecifier>);

impl std::ops::Deref for VersionSpecifiers {
    type Target = [VersionSpecifier];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl VersionSpecifiers {
    /// Matches all versions.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Whether all specifiers match the given version.
    pub fn contains(&self, version: &Version) -> bool {
        self.iter().all(|specifier| specifier.contains(version))
    }

    /// Returns `true` if the specifiers are empty is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sort the specifiers.
    fn from_unsorted(mut specifiers: Vec<VersionSpecifier>) -> Self {
        // TODO(konsti): This seems better than sorting on insert and not getting the size hint,
        // but i haven't measured it.
        specifiers.sort_by(|a, b| a.version().cmp(b.version()));
        Self(specifiers)
    }

    /// Returns the [`VersionSpecifiers`] whose union represents the given range.
    ///
    /// This function is not applicable to ranges involving pre-release versions.
    pub fn from_release_only_bounds<'a>(
        mut bounds: impl Iterator<Item = (&'a Bound<Version>, &'a Bound<Version>)>,
    ) -> Self {
        let mut specifiers = Vec::new();

        let Some((start, mut next)) = bounds.next() else {
            return Self::empty();
        };

        // Add specifiers for the holes between the bounds.
        for (lower, upper) in bounds {
            match (next, lower) {
                // Ex) [3.7, 3.8.5), (3.8.5, 3.9] -> >=3.7,!=3.8.5,<=3.9
                (Bound::Excluded(prev), Bound::Excluded(lower)) if prev == lower => {
                    specifiers.push(VersionSpecifier::not_equals_version(prev.clone()));
                }
                // Ex) [3.7, 3.8), (3.8, 3.9] -> >=3.7,!=3.8.*,<=3.9
                (Bound::Excluded(prev), Bound::Included(lower))
                    if prev.release().len() == 2
                        && lower.release() == [prev.release()[0], prev.release()[1] + 1] =>
                {
                    specifiers.push(VersionSpecifier::not_equals_star_version(prev.clone()));
                }
                _ => {
                    warn!("Ignoring unsupported gap in `requires-python` version: {next:?} -> {lower:?}");
                }
            }
            next = upper;
        }
        let end = next;

        // Add the specifiers for the bounding range.
        specifiers.extend(VersionSpecifier::from_release_only_bounds((start, end)));

        Self::from_unsorted(specifiers)
    }
}

impl FromIterator<VersionSpecifier> for VersionSpecifiers {
    fn from_iter<T: IntoIterator<Item = VersionSpecifier>>(iter: T) -> Self {
        Self::from_unsorted(iter.into_iter().collect())
    }
}

impl FromStr for VersionSpecifiers {
    type Err = VersionSpecifiersParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version_specifiers(s).map(Self::from_unsorted)
    }
}

impl From<VersionSpecifier> for VersionSpecifiers {
    fn from(specifier: VersionSpecifier) -> Self {
        Self(vec![specifier])
    }
}

impl std::fmt::Display for VersionSpecifiers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (idx, version_specifier) in self.0.iter().enumerate() {
            // Separate version specifiers by comma, but we need one comma less than there are
            // specifiers
            if idx == 0 {
                write!(f, "{version_specifier}")?;
            } else {
                write!(f, ", {version_specifier}")?;
            }
        }
        Ok(())
    }
}

impl Default for VersionSpecifiers {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'de> Deserialize<'de> for VersionSpecifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

impl Serialize for VersionSpecifiers {
    #[allow(unstable_name_collisions)]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(
            &self
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(","),
        )
    }
}

/// Error with span information (unicode width) inside the parsed line
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct VersionSpecifiersParseError {
    // Clippy complains about this error type being too big (at time of
    // writing, over 150 bytes). That does seem a little big, so we box things.
    inner: Box<VersionSpecifiersParseErrorInner>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct VersionSpecifiersParseErrorInner {
    /// The underlying error that occurred.
    err: VersionSpecifierParseError,
    /// The string that failed to parse
    line: String,
    /// The starting byte offset into the original string where the error
    /// occurred.
    start: usize,
    /// The ending byte offset into the original string where the error
    /// occurred.
    end: usize,
}

impl std::fmt::Display for VersionSpecifiersParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use unicode_width::UnicodeWidthStr;

        let VersionSpecifiersParseErrorInner {
            ref err,
            ref line,
            start,
            end,
        } = *self.inner;
        writeln!(f, "Failed to parse version: {err}:")?;
        writeln!(f, "{line}")?;
        let indent = line[..start].width();
        let point = line[start..end].width();
        writeln!(f, "{}{}", " ".repeat(indent), "^".repeat(point))?;
        Ok(())
    }
}

impl VersionSpecifiersParseError {
    /// The string that failed to parse
    pub fn line(&self) -> &String {
        &self.inner.line
    }
}

impl std::error::Error for VersionSpecifiersParseError {}

/// A version range such such as `>1.2.3`, `<=4!5.6.7-a8.post9.dev0` or `== 4.1.*`. Parse with
/// `VersionSpecifier::from_str`
///
/// ```rust
/// use std::str::FromStr;
/// use uv_pep440::{Version, VersionSpecifier};
///
/// let version = Version::from_str("1.19").unwrap();
/// let version_specifier = VersionSpecifier::from_str("== 1.*").unwrap();
/// assert!(version_specifier.contains(&version));
/// ```
#[derive(
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    Debug,
    Clone,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct VersionSpecifier {
    /// ~=|==|!=|<=|>=|<|>|===, plus whether the version ended with a star
    pub(crate) operator: Operator,
    /// The whole version part behind the operator
    pub(crate) version: Version,
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl<'de> Deserialize<'de> for VersionSpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl Serialize for VersionSpecifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl VersionSpecifier {
    /// Build from parts, validating that the operator is allowed with that version. The last
    /// parameter indicates a trailing `.*`, to differentiate between `1.1.*` and `1.1`
    pub fn from_pattern(
        operator: Operator,
        version_pattern: VersionPattern,
    ) -> Result<Self, VersionSpecifierBuildError> {
        let star = version_pattern.is_wildcard();
        let version = version_pattern.into_version();

        // Check if there are star versions and if so, switch operator to star version
        let operator = if star {
            match operator.to_star() {
                Some(starop) => starop,
                None => {
                    return Err(BuildErrorKind::OperatorWithStar { operator }.into());
                }
            }
        } else {
            operator
        };

        Self::from_version(operator, version)
    }

    /// Create a new version specifier from an operator and a version.
    pub fn from_version(
        operator: Operator,
        version: Version,
    ) -> Result<Self, VersionSpecifierBuildError> {
        // "Local version identifiers are NOT permitted in this version specifier."
        if version.is_local() && !operator.is_local_compatible() {
            return Err(BuildErrorKind::OperatorLocalCombo { operator, version }.into());
        }

        if operator == Operator::TildeEqual && version.release().len() < 2 {
            return Err(BuildErrorKind::CompatibleRelease.into());
        }

        Ok(Self { operator, version })
    }

    /// `==<version>`
    pub fn equals_version(version: Version) -> Self {
        Self {
            operator: Operator::Equal,
            version,
        }
    }

    /// `==<version>.*`
    pub fn equals_star_version(version: Version) -> Self {
        Self {
            operator: Operator::EqualStar,
            version,
        }
    }

    /// `!=<version>.*`
    pub fn not_equals_star_version(version: Version) -> Self {
        Self {
            operator: Operator::NotEqualStar,
            version,
        }
    }

    /// `!=<version>`
    pub fn not_equals_version(version: Version) -> Self {
        Self {
            operator: Operator::NotEqual,
            version,
        }
    }

    /// `>=<version>`
    pub fn greater_than_equal_version(version: Version) -> Self {
        Self {
            operator: Operator::GreaterThanEqual,
            version,
        }
    }
    /// `><version>`
    pub fn greater_than_version(version: Version) -> Self {
        Self {
            operator: Operator::GreaterThan,
            version,
        }
    }

    /// `<=<version>`
    pub fn less_than_equal_version(version: Version) -> Self {
        Self {
            operator: Operator::LessThanEqual,
            version,
        }
    }

    /// `<<version>`
    pub fn less_than_version(version: Version) -> Self {
        Self {
            operator: Operator::LessThan,
            version,
        }
    }

    /// Get the operator, e.g. `>=` in `>= 2.0.0`
    pub fn operator(&self) -> &Operator {
        &self.operator
    }

    /// Get the version, e.g. `<=` in `<= 2.0.0`
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Get the operator and version parts of this specifier.
    pub fn into_parts(self) -> (Operator, Version) {
        (self.operator, self.version)
    }

    /// Whether the version marker includes a prerelease.
    pub fn any_prerelease(&self) -> bool {
        self.version.any_prerelease()
    }

    /// Returns the version specifiers whose union represents the given range.
    ///
    /// This function is not applicable to ranges involving pre-release versions.
    pub fn from_release_only_bounds(
        bounds: (&Bound<Version>, &Bound<Version>),
    ) -> impl Iterator<Item = VersionSpecifier> {
        let (b1, b2) = match bounds {
            (Bound::Included(v1), Bound::Included(v2)) if v1 == v2 => {
                (Some(VersionSpecifier::equals_version(v1.clone())), None)
            }
            // `v >= 3.7 && v < 3.8` is equivalent to `v == 3.7.*`
            (Bound::Included(v1), Bound::Excluded(v2))
                if v1.release().len() == 2
                    && v2.release() == [v1.release()[0], v1.release()[1] + 1] =>
            {
                (
                    Some(VersionSpecifier::equals_star_version(v1.clone())),
                    None,
                )
            }
            (lower, upper) => (
                VersionSpecifier::from_lower_bound(lower),
                VersionSpecifier::from_upper_bound(upper),
            ),
        };

        b1.into_iter().chain(b2)
    }

    /// Returns a version specifier representing the given lower bound.
    pub fn from_lower_bound(bound: &Bound<Version>) -> Option<VersionSpecifier> {
        match bound {
            Bound::Included(version) => Some(
                VersionSpecifier::from_version(Operator::GreaterThanEqual, version.clone())
                    .unwrap(),
            ),
            Bound::Excluded(version) => Some(
                VersionSpecifier::from_version(Operator::GreaterThan, version.clone()).unwrap(),
            ),
            Bound::Unbounded => None,
        }
    }

    /// Returns a version specifier representing the given upper bound.
    pub fn from_upper_bound(bound: &Bound<Version>) -> Option<VersionSpecifier> {
        match bound {
            Bound::Included(version) => Some(
                VersionSpecifier::from_version(Operator::LessThanEqual, version.clone()).unwrap(),
            ),
            Bound::Excluded(version) => {
                Some(VersionSpecifier::from_version(Operator::LessThan, version.clone()).unwrap())
            }
            Bound::Unbounded => None,
        }
    }

    /// Whether the given version satisfies the version range.
    ///
    /// For example, `>=1.19,<2.0` contains `1.21`, but not `2.0`.
    ///
    /// See:
    /// - <https://peps.python.org/pep-0440/#version-specifiers>
    /// - <https://github.com/pypa/packaging/blob/e184feef1a28a5c574ec41f5c263a3a573861f5a/packaging/specifiers.py#L362-L496>
    pub fn contains(&self, version: &Version) -> bool {
        // "Except where specifically noted below, local version identifiers MUST NOT be permitted
        // in version specifiers, and local version labels MUST be ignored entirely when checking
        // if candidate versions match a given version specifier."
        let (this, other) = if self.version.local().is_empty() {
            // self is already without local
            (self.version.clone(), version.clone().without_local())
        } else {
            (self.version.clone(), version.clone())
        };

        match self.operator {
            Operator::Equal => other == this,
            Operator::EqualStar => {
                this.epoch() == other.epoch()
                    && self
                        .version
                        .release()
                        .iter()
                        .zip(other.release())
                        .all(|(this, other)| this == other)
            }
            #[allow(deprecated)]
            Operator::ExactEqual => {
                #[cfg(feature = "tracing")]
                {
                    tracing::warn!("Using arbitrary equality (`===`) is discouraged");
                }
                self.version.to_string() == version.to_string()
            }
            Operator::NotEqual => other != this,
            Operator::NotEqualStar => {
                this.epoch() != other.epoch()
                    || !this
                        .release()
                        .iter()
                        .zip(version.release())
                        .all(|(this, other)| this == other)
            }
            Operator::TildeEqual => {
                // "For a given release identifier V.N, the compatible release clause is
                // approximately equivalent to the pair of comparison clauses: `>= V.N, == V.*`"
                // First, we test that every but the last digit matches.
                // We know that this must hold true since we checked it in the constructor
                assert!(this.release().len() > 1);
                if this.epoch() != other.epoch() {
                    return false;
                }

                if !this.release()[..this.release().len() - 1]
                    .iter()
                    .zip(other.release())
                    .all(|(this, other)| this == other)
                {
                    return false;
                }

                // According to PEP 440, this ignores the pre-release special rules
                // pypa/packaging disagrees: https://github.com/pypa/packaging/issues/617
                other >= this
            }
            Operator::GreaterThan => Self::greater_than(&this, &other),
            Operator::GreaterThanEqual => Self::greater_than(&this, &other) || other >= this,
            Operator::LessThan => {
                Self::less_than(&this, &other)
                    && !(version::compare_release(this.release(), other.release())
                        == Ordering::Equal
                        && other.any_prerelease())
            }
            Operator::LessThanEqual => Self::less_than(&this, &other) || other <= this,
        }
    }

    fn less_than(this: &Version, other: &Version) -> bool {
        if other.epoch() < this.epoch() {
            return true;
        }

        // This special case is here so that, unless the specifier itself
        // includes is a pre-release version, that we do not accept pre-release
        // versions for the version mentioned in the specifier (e.g. <3.1 should
        // not match 3.1.dev0, but should match 3.0.dev0).
        if !this.any_prerelease()
            && other.is_pre()
            && version::compare_release(this.release(), other.release()) == Ordering::Equal
        {
            return false;
        }

        other < this
    }

    fn greater_than(this: &Version, other: &Version) -> bool {
        if other.epoch() > this.epoch() {
            return true;
        }

        if version::compare_release(this.release(), other.release()) == Ordering::Equal {
            // This special case is here so that, unless the specifier itself
            // includes is a post-release version, that we do not accept
            // post-release versions for the version mentioned in the specifier
            // (e.g. >3.1 should not match 3.0.post0, but should match 3.2.post0).
            if !this.is_post() && other.is_post() {
                return false;
            }

            // We already checked that self doesn't have a local version
            if other.is_local() {
                return false;
            }
        }

        other > this
    }

    /// Whether this version specifier rejects versions below a lower cutoff.
    pub fn has_lower_bound(&self) -> bool {
        match self.operator() {
            Operator::Equal
            | Operator::EqualStar
            | Operator::ExactEqual
            | Operator::TildeEqual
            | Operator::GreaterThan
            | Operator::GreaterThanEqual => true,
            Operator::LessThanEqual
            | Operator::LessThan
            | Operator::NotEqualStar
            | Operator::NotEqual => false,
        }
    }
}

impl FromStr for VersionSpecifier {
    type Err = VersionSpecifierParseError;

    /// Parses a version such as `>= 1.19`, `== 1.1.*`,`~=1.0+abc.5` or `<=1!2012.2`
    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let mut s = unscanny::Scanner::new(spec);
        s.eat_while(|c: char| c.is_whitespace());
        // operator but we don't know yet if it has a star
        let operator = s.eat_while(['=', '!', '~', '<', '>']);
        if operator.is_empty() {
            return Err(ParseErrorKind::MissingOperator.into());
        }
        let operator = Operator::from_str(operator).map_err(ParseErrorKind::InvalidOperator)?;
        s.eat_while(|c: char| c.is_whitespace());
        let version = s.eat_while(|c: char| !c.is_whitespace());
        if version.is_empty() {
            return Err(ParseErrorKind::MissingVersion.into());
        }
        let vpat = version.parse().map_err(ParseErrorKind::InvalidVersion)?;
        let version_specifier =
            Self::from_pattern(operator, vpat).map_err(ParseErrorKind::InvalidSpecifier)?;
        s.eat_while(|c: char| c.is_whitespace());
        if !s.done() {
            return Err(ParseErrorKind::InvalidTrailing(s.after().to_string()).into());
        }
        Ok(version_specifier)
    }
}

impl std::fmt::Display for VersionSpecifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.operator == Operator::EqualStar || self.operator == Operator::NotEqualStar {
            return write!(f, "{}{}.*", self.operator, self.version);
        }
        write!(f, "{}{}", self.operator, self.version)
    }
}

/// An error that can occur when constructing a version specifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionSpecifierBuildError {
    // We box to shrink the error type's size. This in turn keeps Result<T, E>
    // smaller and should lead to overall better codegen.
    kind: Box<BuildErrorKind>,
}

impl std::error::Error for VersionSpecifierBuildError {}

impl std::fmt::Display for VersionSpecifierBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self.kind {
            BuildErrorKind::OperatorLocalCombo {
                operator: ref op,
                ref version,
            } => {
                let local = version
                    .local()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(".");
                write!(
                    f,
                    "Operator {op} is incompatible with versions \
                     containing non-empty local segments (`+{local}`)",
                )
            }
            BuildErrorKind::OperatorWithStar { operator: ref op } => {
                write!(
                    f,
                    "Operator {op} cannot be used with a wildcard version specifier",
                )
            }
            BuildErrorKind::CompatibleRelease => {
                write!(
                    f,
                    "The ~= operator requires at least two segments in the release version"
                )
            }
        }
    }
}

/// The specific kind of error that can occur when building a version specifier
/// from an operator and version pair.
#[derive(Clone, Debug, Eq, PartialEq)]
enum BuildErrorKind {
    /// Occurs when one attempts to build a version specifier with
    /// a version containing a non-empty local segment with and an
    /// incompatible operator.
    OperatorLocalCombo {
        /// The operator given.
        operator: Operator,
        /// The version given.
        version: Version,
    },
    /// Occurs when a version specifier contains a wildcard, but is used with
    /// an incompatible operator.
    OperatorWithStar {
        /// The operator given.
        operator: Operator,
    },
    /// Occurs when the compatible release operator (`~=`) is used with a
    /// version that has fewer than 2 segments in its release version.
    CompatibleRelease,
}

impl From<BuildErrorKind> for VersionSpecifierBuildError {
    fn from(kind: BuildErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }
}

/// An error that can occur when parsing or constructing a version specifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionSpecifierParseError {
    // We box to shrink the error type's size. This in turn keeps Result<T, E>
    // smaller and should lead to overall better codegen.
    kind: Box<ParseErrorKind>,
}

impl std::error::Error for VersionSpecifierParseError {}

impl std::fmt::Display for VersionSpecifierParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // Note that even though we have nested error types here, since we
        // don't expose them through std::error::Error::source, we emit them
        // as part of the error message here. This makes the error a bit
        // more self-contained. And it's not clear how useful it is exposing
        // internal errors.
        match *self.kind {
            ParseErrorKind::InvalidOperator(ref err) => err.fmt(f),
            ParseErrorKind::InvalidVersion(ref err) => err.fmt(f),
            ParseErrorKind::InvalidSpecifier(ref err) => err.fmt(f),
            ParseErrorKind::MissingOperator => {
                write!(f, "Unexpected end of version specifier, expected operator")
            }
            ParseErrorKind::MissingVersion => {
                write!(f, "Unexpected end of version specifier, expected version")
            }
            ParseErrorKind::InvalidTrailing(ref trail) => {
                write!(f, "Trailing `{trail}` is not allowed")
            }
        }
    }
}

/// The specific kind of error that occurs when parsing a single version
/// specifier from a string.
#[derive(Clone, Debug, Eq, PartialEq)]
enum ParseErrorKind {
    InvalidOperator(OperatorParseError),
    InvalidVersion(VersionPatternParseError),
    InvalidSpecifier(VersionSpecifierBuildError),
    MissingOperator,
    MissingVersion,
    InvalidTrailing(String),
}

impl From<ParseErrorKind> for VersionSpecifierParseError {
    fn from(kind: ParseErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }
}

/// Parse a list of specifiers such as `>= 1.0, != 1.3.*, < 2.0`.
pub(crate) fn parse_version_specifiers(
    spec: &str,
) -> Result<Vec<VersionSpecifier>, VersionSpecifiersParseError> {
    let mut version_ranges = Vec::new();
    if spec.is_empty() {
        return Ok(version_ranges);
    }
    let mut start: usize = 0;
    let separator = ",";
    for version_range_spec in spec.split(separator) {
        match VersionSpecifier::from_str(version_range_spec) {
            Err(err) => {
                return Err(VersionSpecifiersParseError {
                    inner: Box::new(VersionSpecifiersParseErrorInner {
                        err,
                        line: spec.to_string(),
                        start,
                        end: start + version_range_spec.len(),
                    }),
                });
            }
            Ok(version_range) => {
                version_ranges.push(version_range);
            }
        }
        start += version_range_spec.len();
        start += separator.len();
    }
    Ok(version_ranges)
}

#[cfg(test)]
mod tests;
