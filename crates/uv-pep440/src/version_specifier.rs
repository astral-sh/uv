use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::Formatter;
use std::ops::Bound;
use std::str::FromStr;

use crate::{
    Operator, OperatorParseError, Version, VersionPattern, VersionPatternParseError, version,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
#[cfg(feature = "tracing")]
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
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Hash)]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "rkyv", rkyv(derive(Debug)))]
pub struct VersionSpecifiers(Box<[VersionSpecifier]>);

impl std::ops::Deref for VersionSpecifiers {
    type Target = [VersionSpecifier];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl VersionSpecifiers {
    /// Matches all versions.
    pub fn empty() -> Self {
        Self(Box::new([]))
    }

    /// The number of specifiers.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether all specifiers match the given version.
    pub fn contains(&self, version: &Version) -> bool {
        self.iter().all(|specifier| specifier.contains(version))
    }

    /// Returns `true` if there are no specifiers.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sort the specifiers.
    fn from_unsorted(mut specifiers: Vec<VersionSpecifier>) -> Self {
        // TODO(konsti): This seems better than sorting on insert and not getting the size hint,
        // but i haven't measured it.
        specifiers.sort_by(|a, b| a.version().cmp(b.version()));
        Self(specifiers.into_boxed_slice())
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
            let specifier = match (next, lower) {
                // Ex) [3.7, 3.8.5), (3.8.5, 3.9] -> >=3.7,!=3.8.5,<=3.9
                (Bound::Excluded(prev), Bound::Excluded(lower)) if prev == lower => {
                    Some(VersionSpecifier::not_equals_version(prev.clone()))
                }
                // Ex) [3.7, 3.8), (3.8, 3.9] -> >=3.7,!=3.8.*,<=3.9
                (Bound::Excluded(prev), Bound::Included(lower)) => {
                    match *prev.only_release_trimmed().release() {
                        [major] if *lower.only_release_trimmed().release() == [major, 1] => {
                            Some(VersionSpecifier::not_equals_star_version(Version::new([
                                major, 0,
                            ])))
                        }
                        [major, minor]
                            if *lower.only_release_trimmed().release() == [major, minor + 1] =>
                        {
                            Some(VersionSpecifier::not_equals_star_version(Version::new([
                                major, minor,
                            ])))
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(specifier) = specifier {
                specifiers.push(specifier);
            } else {
                #[cfg(feature = "tracing")]
                warn!(
                    "Ignoring unsupported gap in `requires-python` version: {next:?} -> {lower:?}"
                );
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

impl IntoIterator for VersionSpecifiers {
    type Item = VersionSpecifier;
    type IntoIter = std::vec::IntoIter<VersionSpecifier>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
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
        Self(Box::new([specifier]))
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
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = VersionSpecifiers;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                VersionSpecifiers::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Serialize for VersionSpecifiers {
    #[allow(unstable_name_collisions)]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(
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

/// A version range such as `>1.2.3`, `<=4!5.6.7-a8.post9.dev0` or `== 4.1.*`. Parse with
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
#[derive(Eq, Ord, PartialEq, PartialOrd, Debug, Clone, Hash)]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "rkyv", rkyv(derive(Debug)))]
pub struct VersionSpecifier {
    /// ~=|==|!=|<=|>=|<|>|===, plus whether the version ended with a star
    pub(crate) operator: Operator,
    /// The whole version part behind the operator
    pub(crate) version: Version,
}

impl<'de> Deserialize<'de> for VersionSpecifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = VersionSpecifier;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                VersionSpecifier::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
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

    /// Remove all non-release parts of the version.
    ///
    /// The marker decision diagram relies on the assumption that the negation of a marker tree is
    /// the complement of the marker space. However, pre-release versions violate this assumption.
    ///
    /// For example, the marker `python_full_version > '3.9' or python_full_version <= '3.9'`
    /// does not match `python_full_version == 3.9.0a0` and so cannot simplify to `true`. However,
    /// its negation, `python_full_version > '3.9' and python_full_version <= '3.9'`, also does not
    /// match `3.9.0a0` and simplifies to `false`, which violates the algebra decision diagrams
    /// rely on. For this reason we ignore pre-release versions entirely when evaluating markers.
    ///
    /// Note that `python_version` cannot take on pre-release values as it is truncated to just the
    /// major and minor version segments. Thus using release-only specifiers is definitely necessary
    /// for `python_version` to fully simplify any ranges, such as
    /// `python_version > '3.9' or python_version <= '3.9'`, which is always `true` for
    /// `python_version`. For `python_full_version` however, this decision is a semantic change.
    ///
    /// For Python versions, the major.minor is considered the API version, so unlike the rules
    /// for package versions in PEP 440, we Python `3.9.0a0` is acceptable for `>= "3.9"`.
    #[must_use]
    pub fn only_release(self) -> Self {
        Self {
            operator: self.operator,
            version: self.version.only_release(),
        }
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

    /// Get the version, e.g. `2.0.0` in `<= 2.0.0`
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
            (Bound::Included(v1), Bound::Excluded(v2)) => {
                match *v1.only_release_trimmed().release() {
                    [major] if *v2.only_release_trimmed().release() == [major, 1] => {
                        let version = Version::new([major, 0]);
                        (Some(VersionSpecifier::equals_star_version(version)), None)
                    }
                    [major, minor]
                        if *v2.only_release_trimmed().release() == [major, minor + 1] =>
                    {
                        let version = Version::new([major, minor]);
                        (Some(VersionSpecifier::equals_star_version(version)), None)
                    }
                    _ => (
                        VersionSpecifier::from_lower_bound(&Bound::Included(v1.clone())),
                        VersionSpecifier::from_upper_bound(&Bound::Excluded(v2.clone())),
                    ),
                }
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
        let this = self.version();
        let other = if this.local().is_empty() && !version.local().is_empty() {
            Cow::Owned(version.clone().without_local())
        } else {
            Cow::Borrowed(version)
        };

        match self.operator {
            Operator::Equal => other.as_ref() == this,
            Operator::EqualStar => {
                this.epoch() == other.epoch()
                    && self
                        .version
                        .release()
                        .iter()
                        .zip(&*other.release())
                        .all(|(this, other)| this == other)
            }
            #[allow(deprecated)]
            Operator::ExactEqual => {
                #[cfg(feature = "tracing")]
                {
                    warn!("Using arbitrary equality (`===`) is discouraged");
                }
                self.version.to_string() == version.to_string()
            }
            Operator::NotEqual => this != other.as_ref(),
            Operator::NotEqualStar => {
                this.epoch() != other.epoch()
                    || !this
                        .release()
                        .iter()
                        .zip(&*version.release())
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
                    .zip(&*other.release())
                    .all(|(this, other)| this == other)
                {
                    return false;
                }

                // According to PEP 440, this ignores the pre-release special rules
                // pypa/packaging disagrees: https://github.com/pypa/packaging/issues/617
                other.as_ref() >= this
            }
            Operator::GreaterThan => {
                if other.epoch() > this.epoch() {
                    return true;
                }

                if version::compare_release(&this.release(), &other.release()) == Ordering::Equal {
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

                other.as_ref() > this
            }
            Operator::GreaterThanEqual => other.as_ref() >= this,
            Operator::LessThan => {
                if other.epoch() < this.epoch() {
                    return true;
                }

                // The exclusive ordered comparison <V MUST NOT allow a pre-release of the specified
                // version unless the specified version is itself a pre-release. E.g., <3.1 should
                // not match 3.1.dev0, but should match both 3.0.dev0 and 3.0, while <3.1.dev1 does
                // match 3.1.dev0, 3.0.dev0 and 3.0.
                if version::compare_release(&this.release(), &other.release()) == Ordering::Equal
                    && !this.any_prerelease()
                    && other.any_prerelease()
                {
                    return false;
                }

                other.as_ref() < this
            }
            Operator::LessThanEqual => other.as_ref() <= this,
        }
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
            // Attempt to parse the version from the rest of the scanner to provide a more useful error message in MissingOperator.
            // If it is not able to be parsed (i.e. not a valid version), it will just be None and no additional info will be added to the error message.
            s.eat_while(|c: char| c.is_whitespace());
            let version = s.eat_while(|c: char| !c.is_whitespace());
            s.eat_while(|c: char| c.is_whitespace());
            return Err(ParseErrorKind::MissingOperator(VersionOperatorBuildError {
                version_pattern: VersionPattern::from_str(version).ok(),
            })
            .into());
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
                let local = version.local();
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct VersionOperatorBuildError {
    version_pattern: Option<VersionPattern>,
}

impl std::error::Error for VersionOperatorBuildError {}

impl std::fmt::Display for VersionOperatorBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Unexpected end of version specifier, expected operator")?;
        if let Some(version_pattern) = &self.version_pattern {
            let version_specifier =
                VersionSpecifier::from_pattern(Operator::Equal, version_pattern.clone()).unwrap();
            write!(f, ". Did you mean `{version_specifier}`?")?;
        }
        Ok(())
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
            ParseErrorKind::MissingOperator(ref err) => err.fmt(f),
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
    MissingOperator(VersionOperatorBuildError),
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

/// A simple `~=` version specifier with a major, minor and (optional) patch version, e.g., `~=3.13`
/// or `~=3.13.0`.
#[derive(Clone, Debug)]
pub struct TildeVersionSpecifier<'a> {
    inner: Cow<'a, VersionSpecifier>,
}

impl<'a> TildeVersionSpecifier<'a> {
    /// Create a new [`TildeVersionSpecifier`] from a [`VersionSpecifier`] value.
    ///
    /// If a [`Operator::TildeEqual`] is not used, or the version includes more than minor and patch
    /// segments, this will return [`None`].
    pub fn from_specifier(specifier: VersionSpecifier) -> Option<TildeVersionSpecifier<'a>> {
        TildeVersionSpecifier::new(Cow::Owned(specifier))
    }

    /// Create a new [`TildeVersionSpecifier`] from a [`VersionSpecifier`] reference.
    ///
    /// See [`TildeVersionSpecifier::from_specifier`].
    pub fn from_specifier_ref(
        specifier: &'a VersionSpecifier,
    ) -> Option<TildeVersionSpecifier<'a>> {
        TildeVersionSpecifier::new(Cow::Borrowed(specifier))
    }

    fn new(specifier: Cow<'a, VersionSpecifier>) -> Option<Self> {
        if specifier.operator != Operator::TildeEqual {
            return None;
        }
        if specifier.version().release().len() < 2 || specifier.version().release().len() > 3 {
            return None;
        }
        if specifier.version().any_prerelease()
            || specifier.version().is_local()
            || specifier.version().is_post()
        {
            return None;
        }
        Some(Self { inner: specifier })
    }

    /// Whether a patch version is present in this tilde version specifier.
    pub fn has_patch(&self) -> bool {
        self.inner.version.release().len() == 3
    }

    /// Construct the lower and upper bounding version specifiers for this tilde version specifier,
    /// e.g., for `~=3.13` this would return `>=3.13` and `<4` and for `~=3.13.0` it would
    /// return `>=3.13.0` and `<3.14`.
    pub fn bounding_specifiers(&self) -> (VersionSpecifier, VersionSpecifier) {
        let release = self.inner.version().release();
        let lower = self.inner.version.clone();
        let upper = if self.has_patch() {
            Version::new([release[0], release[1] + 1])
        } else {
            Version::new([release[0] + 1])
        };
        (
            VersionSpecifier::greater_than_equal_version(lower),
            VersionSpecifier::less_than_version(upper),
        )
    }

    /// Construct a new tilde `VersionSpecifier` with the given patch version appended.
    pub fn with_patch_version(&self, patch: u64) -> TildeVersionSpecifier {
        let mut release = self.inner.version.release().to_vec();
        if self.has_patch() {
            release.pop();
        }
        release.push(patch);
        TildeVersionSpecifier::from_specifier(
            VersionSpecifier::from_version(Operator::TildeEqual, Version::new(release))
                .expect("We should always derive a valid new version specifier"),
        )
        .expect("We should always derive a new tilde version specifier")
    }
}

impl std::fmt::Display for TildeVersionSpecifier<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, str::FromStr};

    use indoc::indoc;

    use crate::LocalSegment;

    use super::*;

    /// <https://peps.python.org/pep-0440/#version-matching>
    #[test]
    fn test_equal() {
        let version = Version::from_str("1.1.post1").unwrap();

        assert!(
            !VersionSpecifier::from_str("== 1.1")
                .unwrap()
                .contains(&version)
        );
        assert!(
            VersionSpecifier::from_str("== 1.1.post1")
                .unwrap()
                .contains(&version)
        );
        assert!(
            VersionSpecifier::from_str("== 1.1.*")
                .unwrap()
                .contains(&version)
        );
    }

    const VERSIONS_ALL: &[&str] = &[
        // Implicit epoch of 0
        "1.0.dev456",
        "1.0a1",
        "1.0a2.dev456",
        "1.0a12.dev456",
        "1.0a12",
        "1.0b1.dev456",
        "1.0b2",
        "1.0b2.post345.dev456",
        "1.0b2.post345",
        "1.0b2-346",
        "1.0c1.dev456",
        "1.0c1",
        "1.0rc2",
        "1.0c3",
        "1.0",
        "1.0.post456.dev34",
        "1.0.post456",
        "1.1.dev1",
        "1.2+123abc",
        "1.2+123abc456",
        "1.2+abc",
        "1.2+abc123",
        "1.2+abc123def",
        "1.2+1234.abc",
        "1.2+123456",
        "1.2.r32+123456",
        "1.2.rev33+123456",
        // Explicit epoch of 1
        "1!1.0.dev456",
        "1!1.0a1",
        "1!1.0a2.dev456",
        "1!1.0a12.dev456",
        "1!1.0a12",
        "1!1.0b1.dev456",
        "1!1.0b2",
        "1!1.0b2.post345.dev456",
        "1!1.0b2.post345",
        "1!1.0b2-346",
        "1!1.0c1.dev456",
        "1!1.0c1",
        "1!1.0rc2",
        "1!1.0c3",
        "1!1.0",
        "1!1.0.post456.dev34",
        "1!1.0.post456",
        "1!1.1.dev1",
        "1!1.2+123abc",
        "1!1.2+123abc456",
        "1!1.2+abc",
        "1!1.2+abc123",
        "1!1.2+abc123def",
        "1!1.2+1234.abc",
        "1!1.2+123456",
        "1!1.2.r32+123456",
        "1!1.2.rev33+123456",
    ];

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L666-L707>
    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L709-L750>
    ///
    /// These tests are a lot shorter than the pypa/packaging version since we implement all
    /// comparisons through one method
    #[test]
    fn test_operators_true() {
        let versions: Vec<Version> = VERSIONS_ALL
            .iter()
            .map(|version| Version::from_str(version).unwrap())
            .collect();

        // Below we'll generate every possible combination of VERSIONS_ALL that
        // should be true for the given operator
        let operations = [
            // Verify that the less than (<) operator works correctly
            versions
                .iter()
                .enumerate()
                .flat_map(|(i, x)| {
                    versions[i + 1..]
                        .iter()
                        .map(move |y| (x, y, Ordering::Less))
                })
                .collect::<Vec<_>>(),
            // Verify that the equal (==) operator works correctly
            versions
                .iter()
                .map(move |x| (x, x, Ordering::Equal))
                .collect::<Vec<_>>(),
            // Verify that the greater than (>) operator works correctly
            versions
                .iter()
                .enumerate()
                .flat_map(|(i, x)| versions[..i].iter().map(move |y| (x, y, Ordering::Greater)))
                .collect::<Vec<_>>(),
        ]
        .into_iter()
        .flatten();

        for (a, b, ordering) in operations {
            assert_eq!(a.cmp(b), ordering, "{a} {ordering:?} {b}");
        }
    }

    const VERSIONS_0: &[&str] = &[
        "1.0.dev456",
        "1.0a1",
        "1.0a2.dev456",
        "1.0a12.dev456",
        "1.0a12",
        "1.0b1.dev456",
        "1.0b2",
        "1.0b2.post345.dev456",
        "1.0b2.post345",
        "1.0b2-346",
        "1.0c1.dev456",
        "1.0c1",
        "1.0rc2",
        "1.0c3",
        "1.0",
        "1.0.post456.dev34",
        "1.0.post456",
        "1.1.dev1",
        "1.2+123abc",
        "1.2+123abc456",
        "1.2+abc",
        "1.2+abc123",
        "1.2+abc123def",
        "1.2+1234.abc",
        "1.2+123456",
        "1.2.r32+123456",
        "1.2.rev33+123456",
    ];

    const SPECIFIERS_OTHER: &[&str] = &[
        "== 1.*", "== 1.0.*", "== 1.1.*", "== 1.2.*", "== 2.*", "~= 1.0", "~= 1.0b1", "~= 1.1",
        "~= 1.2", "~= 2.0",
    ];

    const EXPECTED_OTHER: &[[bool; 10]] = &[
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, false, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, false, true, false, false, false,
        ],
        [
            true, true, false, false, false, true, true, false, false, false,
        ],
        [
            true, true, false, false, false, true, true, false, false, false,
        ],
        [
            true, true, false, false, false, true, true, false, false, false,
        ],
        [
            true, false, true, false, false, true, true, false, false, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
        [
            true, false, false, true, false, true, true, true, true, false,
        ],
    ];

    /// Test for tilde equal (~=) and star equal (== x.y.*) recorded from pypa/packaging
    ///
    /// Well, except for <https://github.com/pypa/packaging/issues/617>
    #[test]
    fn test_operators_other() {
        let versions = VERSIONS_0
            .iter()
            .map(|version| Version::from_str(version).unwrap());
        let specifiers: Vec<_> = SPECIFIERS_OTHER
            .iter()
            .map(|specifier| VersionSpecifier::from_str(specifier).unwrap())
            .collect();

        for (version, expected) in versions.zip(EXPECTED_OTHER) {
            let actual = specifiers
                .iter()
                .map(|specifier| specifier.contains(&version));
            for ((actual, expected), _specifier) in actual.zip(expected).zip(SPECIFIERS_OTHER) {
                assert_eq!(actual, *expected);
            }
        }
    }

    #[test]
    fn test_arbitrary_equality() {
        assert!(
            VersionSpecifier::from_str("=== 1.2a1")
                .unwrap()
                .contains(&Version::from_str("1.2a1").unwrap())
        );
        assert!(
            !VersionSpecifier::from_str("=== 1.2a1")
                .unwrap()
                .contains(&Version::from_str("1.2a1+local").unwrap())
        );
    }

    #[test]
    fn test_specifiers_true() {
        let pairs = [
            // Test the equality operation
            ("2.0", "==2"),
            ("2.0", "==2.0"),
            ("2.0", "==2.0.0"),
            ("2.0+deadbeef", "==2"),
            ("2.0+deadbeef", "==2.0"),
            ("2.0+deadbeef", "==2.0.0"),
            ("2.0+deadbeef", "==2+deadbeef"),
            ("2.0+deadbeef", "==2.0+deadbeef"),
            ("2.0+deadbeef", "==2.0.0+deadbeef"),
            ("2.0+deadbeef.0", "==2.0.0+deadbeef.00"),
            // Test the equality operation with a prefix
            ("2.dev1", "==2.*"),
            ("2a1", "==2.*"),
            ("2a1.post1", "==2.*"),
            ("2b1", "==2.*"),
            ("2b1.dev1", "==2.*"),
            ("2c1", "==2.*"),
            ("2c1.post1.dev1", "==2.*"),
            ("2c1.post1.dev1", "==2.0.*"),
            ("2rc1", "==2.*"),
            ("2rc1", "==2.0.*"),
            ("2", "==2.*"),
            ("2", "==2.0.*"),
            ("2", "==0!2.*"),
            ("0!2", "==2.*"),
            ("2.0", "==2.*"),
            ("2.0.0", "==2.*"),
            ("2.1+local.version", "==2.1.*"),
            // Test the in-equality operation
            ("2.1", "!=2"),
            ("2.1", "!=2.0"),
            ("2.0.1", "!=2"),
            ("2.0.1", "!=2.0"),
            ("2.0.1", "!=2.0.0"),
            ("2.0", "!=2.0+deadbeef"),
            // Test the in-equality operation with a prefix
            ("2.0", "!=3.*"),
            ("2.1", "!=2.0.*"),
            // Test the greater than equal operation
            ("2.0", ">=2"),
            ("2.0", ">=2.0"),
            ("2.0", ">=2.0.0"),
            ("2.0.post1", ">=2"),
            ("2.0.post1.dev1", ">=2"),
            ("3", ">=2"),
            // Test the less than equal operation
            ("2.0", "<=2"),
            ("2.0", "<=2.0"),
            ("2.0", "<=2.0.0"),
            ("2.0.dev1", "<=2"),
            ("2.0a1", "<=2"),
            ("2.0a1.dev1", "<=2"),
            ("2.0b1", "<=2"),
            ("2.0b1.post1", "<=2"),
            ("2.0c1", "<=2"),
            ("2.0c1.post1.dev1", "<=2"),
            ("2.0rc1", "<=2"),
            ("1", "<=2"),
            // Test the greater than operation
            ("3", ">2"),
            ("2.1", ">2.0"),
            ("2.0.1", ">2"),
            ("2.1.post1", ">2"),
            ("2.1+local.version", ">2"),
            ("2.post2", ">2.post1"),
            // Test the less than operation
            ("1", "<2"),
            ("2.0", "<2.1"),
            ("2.0.dev0", "<2.1"),
            // https://github.com/astral-sh/uv/issues/12834
            ("0.1a1", "<0.1a2"),
            ("0.1dev1", "<0.1dev2"),
            ("0.1dev1", "<0.1a1"),
            // Test the compatibility operation
            ("1", "~=1.0"),
            ("1.0.1", "~=1.0"),
            ("1.1", "~=1.0"),
            ("1.9999999", "~=1.0"),
            ("1.1", "~=1.0a1"),
            ("2022.01.01", "~=2022.01.01"),
            // Test that epochs are handled sanely
            ("2!1.0", "~=2!1.0"),
            ("2!1.0", "==2!1.*"),
            ("2!1.0", "==2!1.0"),
            ("2!1.0", "!=1.0"),
            ("1.0", "!=2!1.0"),
            ("1.0", "<=2!0.1"),
            ("2!1.0", ">=2.0"),
            ("1.0", "<2!0.1"),
            ("2!1.0", ">2.0"),
            // Test some normalization rules
            ("2.0.5", ">2.0dev"),
        ];

        for (s_version, s_spec) in pairs {
            let version = s_version.parse::<Version>().unwrap();
            let spec = s_spec.parse::<VersionSpecifier>().unwrap();
            assert!(
                spec.contains(&version),
                "{s_version} {s_spec}\nversion repr: {:?}\nspec version repr: {:?}",
                version.as_bloated_debug(),
                spec.version.as_bloated_debug(),
            );
        }
    }

    #[test]
    fn test_specifier_false() {
        let pairs = [
            // Test the equality operation
            ("2.1", "==2"),
            ("2.1", "==2.0"),
            ("2.1", "==2.0.0"),
            ("2.0", "==2.0+deadbeef"),
            // Test the equality operation with a prefix
            ("2.0", "==3.*"),
            ("2.1", "==2.0.*"),
            // Test the in-equality operation
            ("2.0", "!=2"),
            ("2.0", "!=2.0"),
            ("2.0", "!=2.0.0"),
            ("2.0+deadbeef", "!=2"),
            ("2.0+deadbeef", "!=2.0"),
            ("2.0+deadbeef", "!=2.0.0"),
            ("2.0+deadbeef", "!=2+deadbeef"),
            ("2.0+deadbeef", "!=2.0+deadbeef"),
            ("2.0+deadbeef", "!=2.0.0+deadbeef"),
            ("2.0+deadbeef.0", "!=2.0.0+deadbeef.00"),
            // Test the in-equality operation with a prefix
            ("2.dev1", "!=2.*"),
            ("2a1", "!=2.*"),
            ("2a1.post1", "!=2.*"),
            ("2b1", "!=2.*"),
            ("2b1.dev1", "!=2.*"),
            ("2c1", "!=2.*"),
            ("2c1.post1.dev1", "!=2.*"),
            ("2c1.post1.dev1", "!=2.0.*"),
            ("2rc1", "!=2.*"),
            ("2rc1", "!=2.0.*"),
            ("2", "!=2.*"),
            ("2", "!=2.0.*"),
            ("2.0", "!=2.*"),
            ("2.0.0", "!=2.*"),
            // Test the greater than equal operation
            ("2.0.dev1", ">=2"),
            ("2.0a1", ">=2"),
            ("2.0a1.dev1", ">=2"),
            ("2.0b1", ">=2"),
            ("2.0b1.post1", ">=2"),
            ("2.0c1", ">=2"),
            ("2.0c1.post1.dev1", ">=2"),
            ("2.0rc1", ">=2"),
            ("1", ">=2"),
            // Test the less than equal operation
            ("2.0.post1", "<=2"),
            ("2.0.post1.dev1", "<=2"),
            ("3", "<=2"),
            // Test the greater than operation
            ("1", ">2"),
            ("2.0.dev1", ">2"),
            ("2.0a1", ">2"),
            ("2.0a1.post1", ">2"),
            ("2.0b1", ">2"),
            ("2.0b1.dev1", ">2"),
            ("2.0c1", ">2"),
            ("2.0c1.post1.dev1", ">2"),
            ("2.0rc1", ">2"),
            ("2.0", ">2"),
            ("2.post2", ">2"),
            ("2.0.post1", ">2"),
            ("2.0.post1.dev1", ">2"),
            ("2.0+local.version", ">2"),
            // Test the less than operation
            ("2.0.dev1", "<2"),
            ("2.0a1", "<2"),
            ("2.0a1.post1", "<2"),
            ("2.0b1", "<2"),
            ("2.0b2.dev1", "<2"),
            ("2.0c1", "<2"),
            ("2.0c1.post1.dev1", "<2"),
            ("2.0rc1", "<2"),
            ("2.0", "<2"),
            ("2.post1", "<2"),
            ("2.post1.dev1", "<2"),
            ("3", "<2"),
            // Test the compatibility operation
            ("2.0", "~=1.0"),
            ("1.1.0", "~=1.0.0"),
            ("1.1.post1", "~=1.0.0"),
            // Test that epochs are handled sanely
            ("1.0", "~=2!1.0"),
            ("2!1.0", "~=1.0"),
            ("2!1.0", "==1.0"),
            ("1.0", "==2!1.0"),
            ("2!1.0", "==1.*"),
            ("1.0", "==2!1.*"),
            ("2!1.0", "!=2!1.0"),
        ];
        for (version, specifier) in pairs {
            assert!(
                !VersionSpecifier::from_str(specifier)
                    .unwrap()
                    .contains(&Version::from_str(version).unwrap()),
                "{version} {specifier}"
            );
        }
    }

    #[test]
    fn test_parse_version_specifiers() {
        let result = VersionSpecifiers::from_str("~= 0.9, >= 1.0, != 1.3.4.*, < 2.0").unwrap();
        assert_eq!(
            result.0.as_ref(),
            [
                VersionSpecifier {
                    operator: Operator::TildeEqual,
                    version: Version::new([0, 9]),
                },
                VersionSpecifier {
                    operator: Operator::GreaterThanEqual,
                    version: Version::new([1, 0]),
                },
                VersionSpecifier {
                    operator: Operator::NotEqualStar,
                    version: Version::new([1, 3, 4]),
                },
                VersionSpecifier {
                    operator: Operator::LessThan,
                    version: Version::new([2, 0]),
                }
            ]
        );
    }

    #[test]
    fn test_parse_error() {
        let result = VersionSpecifiers::from_str("~= 0.9, %‍= 1.0, != 1.3.4.*");
        assert_eq!(
            result.unwrap_err().to_string(),
            indoc! {r"
            Failed to parse version: Unexpected end of version specifier, expected operator:
            ~= 0.9, %‍= 1.0, != 1.3.4.*
                   ^^^^^^^
        "}
        );
    }

    #[test]
    fn test_parse_specifier_missing_operator_error() {
        let result = VersionSpecifiers::from_str("3.12");
        assert_eq!(
            result.unwrap_err().to_string(),
            indoc! {"
            Failed to parse version: Unexpected end of version specifier, expected operator. Did you mean `==3.12`?:
            3.12
            ^^^^
            "}
        );
    }

    #[test]
    fn test_parse_specifier_missing_operator_invalid_version_error() {
        let result = VersionSpecifiers::from_str("blergh");
        assert_eq!(
            result.unwrap_err().to_string(),
            indoc! {r"
            Failed to parse version: Unexpected end of version specifier, expected operator:
            blergh
            ^^^^^^
            "}
        );
    }

    #[test]
    fn test_non_star_after_star() {
        let result = VersionSpecifiers::from_str("== 0.9.*.1");
        assert_eq!(
            result.unwrap_err().inner.err,
            ParseErrorKind::InvalidVersion(version::PatternErrorKind::WildcardNotTrailing.into())
                .into(),
        );
    }

    #[test]
    fn test_star_wrong_operator() {
        let result = VersionSpecifiers::from_str(">= 0.9.1.*");
        assert_eq!(
            result.unwrap_err().inner.err,
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::GreaterThanEqual,
                }
                .into()
            )
            .into(),
        );
    }

    #[test]
    fn test_invalid_word() {
        let result = VersionSpecifiers::from_str("blergh");
        assert_eq!(
            result.unwrap_err().inner.err,
            ParseErrorKind::MissingOperator(VersionOperatorBuildError {
                version_pattern: None
            })
            .into(),
        );
    }

    /// <https://github.com/pypa/packaging/blob/e184feef1a28a5c574ec41f5c263a3a573861f5a/tests/test_specifiers.py#L44-L84>
    #[test]
    fn test_invalid_specifier() {
        let specifiers = [
            // Operator-less specifier
            (
                "2.0",
                ParseErrorKind::MissingOperator(VersionOperatorBuildError {
                    version_pattern: VersionPattern::from_str("2.0").ok(),
                })
                .into(),
            ),
            // Invalid operator
            (
                "=>2.0",
                ParseErrorKind::InvalidOperator(OperatorParseError {
                    got: "=>".to_string(),
                })
                .into(),
            ),
            // Version-less specifier
            ("==", ParseErrorKind::MissingVersion.into()),
            // Local segment on operators which don't support them
            (
                "~=1.0+5",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorLocalCombo {
                        operator: Operator::TildeEqual,
                        version: Version::new([1, 0])
                            .with_local_segments(vec![LocalSegment::Number(5)]),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                ">=1.0+deadbeef",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorLocalCombo {
                        operator: Operator::GreaterThanEqual,
                        version: Version::new([1, 0]).with_local_segments(vec![
                            LocalSegment::String("deadbeef".to_string()),
                        ]),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "<=1.0+abc123",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorLocalCombo {
                        operator: Operator::LessThanEqual,
                        version: Version::new([1, 0])
                            .with_local_segments(vec![LocalSegment::String("abc123".to_string())]),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                ">1.0+watwat",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorLocalCombo {
                        operator: Operator::GreaterThan,
                        version: Version::new([1, 0])
                            .with_local_segments(vec![LocalSegment::String("watwat".to_string())]),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "<1.0+1.0",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorLocalCombo {
                        operator: Operator::LessThan,
                        version: Version::new([1, 0]).with_local_segments(vec![
                            LocalSegment::Number(1),
                            LocalSegment::Number(0),
                        ]),
                    }
                    .into(),
                )
                .into(),
            ),
            // Prefix matching on operators which don't support them
            (
                "~=1.0.*",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorWithStar {
                        operator: Operator::TildeEqual,
                    }
                    .into(),
                )
                .into(),
            ),
            (
                ">=1.0.*",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorWithStar {
                        operator: Operator::GreaterThanEqual,
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "<=1.0.*",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorWithStar {
                        operator: Operator::LessThanEqual,
                    }
                    .into(),
                )
                .into(),
            ),
            (
                ">1.0.*",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorWithStar {
                        operator: Operator::GreaterThan,
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "<1.0.*",
                ParseErrorKind::InvalidSpecifier(
                    BuildErrorKind::OperatorWithStar {
                        operator: Operator::LessThan,
                    }
                    .into(),
                )
                .into(),
            ),
            // Combination of local and prefix matching on operators which do
            // support one or the other
            (
                "==1.0.*+5",
                ParseErrorKind::InvalidVersion(
                    version::PatternErrorKind::WildcardNotTrailing.into(),
                )
                .into(),
            ),
            (
                "!=1.0.*+deadbeef",
                ParseErrorKind::InvalidVersion(
                    version::PatternErrorKind::WildcardNotTrailing.into(),
                )
                .into(),
            ),
            // Prefix matching cannot be used with a pre-release, post-release,
            // dev or local version
            (
                "==2.0a1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0a1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "!=2.0a1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0a1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "==2.0.post1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0.post1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "!=2.0.post1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0.post1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "==2.0.dev1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0.dev1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "!=2.0.dev1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "2.0.dev1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "==1.0+5.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::LocalEmpty { precursor: '.' }.into(),
                )
                .into(),
            ),
            (
                "!=1.0+deadbeef.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::LocalEmpty { precursor: '.' }.into(),
                )
                .into(),
            ),
            // Prefix matching must appear at the end
            (
                "==1.0.*.5",
                ParseErrorKind::InvalidVersion(
                    version::PatternErrorKind::WildcardNotTrailing.into(),
                )
                .into(),
            ),
            // Compatible operator requires 2 digits in the release operator
            (
                "~=1",
                ParseErrorKind::InvalidSpecifier(BuildErrorKind::CompatibleRelease.into()).into(),
            ),
            // Cannot use a prefix matching after a .devN version
            (
                "==1.0.dev1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "1.0.dev1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
            (
                "!=1.0.dev1.*",
                ParseErrorKind::InvalidVersion(
                    version::ErrorKind::UnexpectedEnd {
                        version: "1.0.dev1".to_string(),
                        remaining: ".*".to_string(),
                    }
                    .into(),
                )
                .into(),
            ),
        ];
        for (specifier, error) in specifiers {
            assert_eq!(VersionSpecifier::from_str(specifier).unwrap_err(), error);
        }
    }

    #[test]
    fn test_display_start() {
        assert_eq!(
            VersionSpecifier::from_str("==     1.1.*")
                .unwrap()
                .to_string(),
            "==1.1.*"
        );
        assert_eq!(
            VersionSpecifier::from_str("!=     1.1.*")
                .unwrap()
                .to_string(),
            "!=1.1.*"
        );
    }

    #[test]
    fn test_version_specifiers_str() {
        assert_eq!(
            VersionSpecifiers::from_str(">= 3.7").unwrap().to_string(),
            ">=3.7"
        );
        assert_eq!(
            VersionSpecifiers::from_str(">=3.7, <      4.0, != 3.9.0")
                .unwrap()
                .to_string(),
            ">=3.7, !=3.9.0, <4.0"
        );
    }

    /// These occur in the simple api, e.g.
    /// <https://pypi.org/simple/geopandas/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn test_version_specifiers_empty() {
        assert_eq!(VersionSpecifiers::from_str("").unwrap().to_string(), "");
    }

    /// All non-ASCII version specifiers are invalid, but the user can still
    /// attempt to parse a non-ASCII string as a version specifier. This
    /// ensures no panics occur and that the error reported has correct info.
    #[test]
    fn non_ascii_version_specifier() {
        let s = "💩";
        let err = s.parse::<VersionSpecifiers>().unwrap_err();
        assert_eq!(err.inner.start, 0);
        assert_eq!(err.inner.end, 4);

        // The first test here is plain ASCII and it gives the
        // expected result: the error starts at codepoint 12,
        // which is the start of `>5.%`.
        let s = ">=3.7, <4.0,>5.%";
        let err = s.parse::<VersionSpecifiers>().unwrap_err();
        assert_eq!(err.inner.start, 12);
        assert_eq!(err.inner.end, 16);
        // In this case, we replace a single ASCII codepoint
        // with U+3000 IDEOGRAPHIC SPACE. Its *visual* width is
        // 2 despite it being a single codepoint. This causes
        // the offsets in the error reporting logic to become
        // incorrect.
        //
        // ... it did. This bug was fixed by switching to byte
        // offsets.
        let s = ">=3.7,\u{3000}<4.0,>5.%";
        let err = s.parse::<VersionSpecifiers>().unwrap_err();
        assert_eq!(err.inner.start, 14);
        assert_eq!(err.inner.end, 18);
    }

    /// Tests the human readable error messages generated from an invalid
    /// sequence of version specifiers.
    #[test]
    fn error_message_version_specifiers_parse_error() {
        let specs = ">=1.2.3, 5.4.3, >=3.4.5";
        let err = VersionSpecifierParseError {
            kind: Box::new(ParseErrorKind::MissingOperator(VersionOperatorBuildError {
                version_pattern: VersionPattern::from_str("5.4.3").ok(),
            })),
        };
        let inner = Box::new(VersionSpecifiersParseErrorInner {
            err,
            line: specs.to_string(),
            start: 8,
            end: 14,
        });
        let err = VersionSpecifiersParseError { inner };
        assert_eq!(err, VersionSpecifiers::from_str(specs).unwrap_err());
        assert_eq!(
            err.to_string(),
            "\
Failed to parse version: Unexpected end of version specifier, expected operator. Did you mean `==5.4.3`?:
>=1.2.3, 5.4.3, >=3.4.5
        ^^^^^^
"
        );
    }

    /// Tests the human readable error messages generated when building an
    /// invalid version specifier.
    #[test]
    fn error_message_version_specifier_build_error() {
        let err = VersionSpecifierBuildError {
            kind: Box::new(BuildErrorKind::CompatibleRelease),
        };
        let op = Operator::TildeEqual;
        let v = Version::new([5]);
        let vpat = VersionPattern::verbatim(v);
        assert_eq!(err, VersionSpecifier::from_pattern(op, vpat).unwrap_err());
        assert_eq!(
            err.to_string(),
            "The ~= operator requires at least two segments in the release version"
        );
    }

    /// Tests the human readable error messages generated from parsing invalid
    /// version specifier.
    #[test]
    fn error_message_version_specifier_parse_error() {
        let err = VersionSpecifierParseError {
            kind: Box::new(ParseErrorKind::InvalidSpecifier(
                VersionSpecifierBuildError {
                    kind: Box::new(BuildErrorKind::CompatibleRelease),
                },
            )),
        };
        assert_eq!(err, VersionSpecifier::from_str("~=5").unwrap_err());
        assert_eq!(
            err.to_string(),
            "The ~= operator requires at least two segments in the release version"
        );
    }
}
