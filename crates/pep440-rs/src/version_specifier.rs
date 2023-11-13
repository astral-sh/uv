#[cfg(feature = "pyo3")]
use crate::version::PyVersion;
use crate::version::VERSION_RE_INNER;
use crate::{version, Operator, Pep440Error, Version};
use once_cell::sync::Lazy;
#[cfg(feature = "pyo3")]
use pyo3::{
    exceptions::{PyIndexError, PyNotImplementedError, PyValueError},
    pyclass,
    pyclass::CompareOp,
    pymethods, Py, PyRef, PyRefMut, PyResult,
};
use regex::Regex;
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
#[cfg(feature = "pyo3")]
use std::collections::hash_map::DefaultHasher;
use std::fmt::Formatter;
use std::fmt::{Debug, Display};
#[cfg(feature = "pyo3")]
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str::FromStr;
use unicode_width::UnicodeWidthStr;

#[cfg(feature = "tracing")]
use tracing::warn;

/// Matches a python version specifier, such as `>=1.19.a1` or `4.1.*`. Extends the PEP 440
/// version regex to version specifiers
static VERSION_SPECIFIER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r#"(?xi)^(?:\s*)(?P<operator>(~=|==|!=|<=|>=|<|>|===))(?:\s*){VERSION_RE_INNER}(?:\s*)$"#,
    ))
    .unwrap()
});

/// A thin wrapper around `Vec<VersionSpecifier>` with a serde implementation
///
/// Python requirements can contain multiple version specifier so we need to store them in a list,
/// such as `>1.2,<2.0` being `[">1.2", "<2.0"]`.
///
/// You can use the serde implementation to e.g. parse `requires-python` from pyproject.toml
///
/// ```rust
/// # use std::str::FromStr;
/// # use pep440_rs::{VersionSpecifiers, Version, Operator};
///
/// let version = Version::from_str("1.19").unwrap();
/// let version_specifiers = VersionSpecifiers::from_str(">=1.16, <2.0").unwrap();
/// assert!(version_specifiers.contains(&version));
/// // VersionSpecifiers derefs into a list of specifiers
/// assert_eq!(version_specifiers.iter().position(|specifier| *specifier.operator() == Operator::LessThan), Some(1));
/// ```
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
#[cfg_attr(feature = "pyo3", pyclass(sequence))]
pub struct VersionSpecifiers(Vec<VersionSpecifier>);

impl Deref for VersionSpecifiers {
    type Target = [VersionSpecifier];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl VersionSpecifiers {
    /// Whether all specifiers match the given version
    pub fn contains(&self, version: &Version) -> bool {
        self.iter().all(|specifier| specifier.contains(version))
    }
}

impl FromIterator<VersionSpecifier> for VersionSpecifiers {
    fn from_iter<T: IntoIterator<Item = VersionSpecifier>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FromStr for VersionSpecifiers {
    type Err = Pep440Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version_specifiers(s).map(Self)
    }
}

impl From<VersionSpecifier> for VersionSpecifiers {
    fn from(specifier: VersionSpecifier) -> Self {
        Self(vec![specifier])
    }
}

impl Display for VersionSpecifiers {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

/// https://pyo3.rs/v0.18.2/class/protocols.html#iterable-objects
#[cfg(feature = "pyo3")]
#[pyclass]
struct VersionSpecifiersIter {
    inner: std::vec::IntoIter<VersionSpecifier>,
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl VersionSpecifiersIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<VersionSpecifier> {
        slf.inner.next()
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl VersionSpecifiers {
    /// PEP 440 parsing
    #[new]
    pub fn __new__(version_specifiers: &str) -> PyResult<Self> {
        Self::from_str(version_specifiers).map_err(|err| PyValueError::new_err(err.to_string()))
    }

    /// PEP 440 serialization
    pub fn __str__(&self) -> String {
        self.to_string()
    }

    /// PEP 440 serialization
    pub fn __repr__(&self) -> String {
        self.to_string()
    }

    /// Get the nth VersionSpecifier
    pub fn __getitem__(&self, idx: usize) -> PyResult<VersionSpecifier> {
        self.0.get(idx).cloned().ok_or_else(|| {
            PyIndexError::new_err(format!(
                "list index {} our of range for len {}",
                idx,
                self.0.len()
            ))
        })
    }

    #[allow(clippy::needless_pass_by_value)]
    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<VersionSpecifiersIter>> {
        let iter = VersionSpecifiersIter {
            inner: slf.0.clone().into_iter(),
        };
        Py::new(slf.py(), iter)
    }

    /// Get the number of VersionSpecifier
    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    /// Whether the version matches all the specifiers
    pub fn __contains__(&self, version: &PyVersion) -> bool {
        self.contains(&version.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for VersionSpecifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

#[cfg(feature = "serde")]
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

/// A version range such such as `>1.2.3`, `<=4!5.6.7-a8.post9.dev0` or `== 4.1.*`. Parse with
/// `VersionSpecifier::from_str`
///
/// ```rust
/// use std::str::FromStr;
/// use pep440_rs::{Version, VersionSpecifier};
///
/// let version = Version::from_str("1.19").unwrap();
/// let version_specifier = VersionSpecifier::from_str("== 1.*").unwrap();
/// assert!(version_specifier.contains(&version));
/// ```
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
#[cfg_attr(feature = "pyo3", pyclass(get_all))]
pub struct VersionSpecifier {
    /// ~=|==|!=|<=|>=|<|>|===, plus whether the version ended with a star
    pub(crate) operator: Operator,
    /// The whole version part behind the operator
    pub(crate) version: Version,
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl VersionSpecifier {
    // Since we don't bring FromStr to python
    /// Parse a PEP 440 version
    #[new]
    pub fn parse(version_specifier: &str) -> PyResult<Self> {
        Self::from_str(version_specifier).map_err(PyValueError::new_err)
    }

    /// See [VersionSpecifier::contains]
    #[pyo3(name = "contains")]
    pub fn py_contains(&self, version: &PyVersion) -> bool {
        self.contains(&version.0)
    }

    /// Whether the version fulfills the specifier
    pub fn __contains__(&self, version: &PyVersion) -> bool {
        self.contains(&version.0)
    }

    /// Returns the normalized representation
    pub fn __str__(&self) -> String {
        self.to_string()
    }

    /// Returns the normalized representation
    pub fn __repr__(&self) -> String {
        format!(r#"<VersionSpecifier("{self}")>"#)
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        if matches!(op, CompareOp::Eq) {
            Ok(self == other)
        } else {
            Err(PyNotImplementedError::new_err(
                "Can only compare VersionSpecifier by equality",
            ))
        }
    }

    /// Returns the normalized representation
    pub fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
#[cfg(feature = "serde")]
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
#[cfg(feature = "serde")]
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
    pub fn new(operator: Operator, version: Version, star: bool) -> Result<Self, String> {
        // "Local version identifiers are NOT permitted in this version specifier."
        if let Some(local) = &version.local {
            if matches!(
                operator,
                Operator::GreaterThan
                    | Operator::GreaterThanEqual
                    | Operator::LessThan
                    | Operator::LessThanEqual
                    | Operator::TildeEqual
                    | Operator::EqualStar
                    | Operator::NotEqualStar
            ) {
                return Err(format!(
                    "You can't mix a {} operator with a local version (`+{}`)",
                    operator,
                    local
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(".")
                ));
            }
        }

        // Check if there are star versions and if so, switch operator to star version
        let operator = if star {
            match operator {
                Operator::Equal => Operator::EqualStar,
                Operator::NotEqual => Operator::NotEqualStar,
                other => {
                    return Err(format!(
                        "Operator {other} must not be used in version ending with a star"
                    ))
                }
            }
        } else {
            operator
        };

        if operator == Operator::TildeEqual && version.release.len() < 2 {
            return Err(
                "The ~= operator requires at least two parts in the release version".to_string(),
            );
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

    /// Get the operator, e.g. `>=` in `>= 2.0.0`
    pub fn operator(&self) -> &Operator {
        &self.operator
    }

    /// Get the version, e.g. `<=` in `<= 2.0.0`
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Whether the version marker includes a prerelease.
    pub fn any_prerelease(&self) -> bool {
        self.version.any_prerelease()
    }
}

impl VersionSpecifier {
    /// Whether the given version satisfies the version range
    ///
    /// e.g. `>=1.19,<2.0` and `1.21` -> true
    /// <https://peps.python.org/pep-0440/#version-specifiers>
    ///
    /// Unlike `pypa/packaging`, prereleases are included by default
    ///
    /// I'm utterly non-confident in the description in PEP 440 and apparently even pip got some
    /// of that wrong, e.g. <https://github.com/pypa/pip/issues/9121> and
    /// <https://github.com/pypa/pip/issues/5503>, and also i'm not sure if it produces the correct
    /// behaviour from a user perspective
    ///
    /// This implementation is as close as possible to
    /// <https://github.com/pypa/packaging/blob/e184feef1a28a5c574ec41f5c263a3a573861f5a/packaging/specifiers.py#L362-L496>
    pub fn contains(&self, version: &Version) -> bool {
        // "Except where specifically noted below, local version identifiers MUST NOT be permitted
        // in version specifiers, and local version labels MUST be ignored entirely when checking
        // if candidate versions match a given version specifier."
        let (this, other) = if self.version.local.is_some() {
            (self.version.clone(), version.clone())
        } else {
            // self is already without local
            (self.version.without_local(), version.without_local())
        };

        match self.operator {
            Operator::Equal => other == this,
            Operator::EqualStar => {
                this.epoch == other.epoch
                    && self
                        .version
                        .release
                        .iter()
                        .zip(&other.release)
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
            Operator::NotEqual => other != this,
            Operator::NotEqualStar => {
                this.epoch != other.epoch
                    || !this
                        .release
                        .iter()
                        .zip(&version.release)
                        .all(|(this, other)| this == other)
            }
            Operator::TildeEqual => {
                // "For a given release identifier V.N, the compatible release clause is
                // approximately equivalent to the pair of comparison clauses: `>= V.N, == V.*`"
                // First, we test that every but the last digit matches.
                // We know that this must hold true since we checked it in the constructor
                assert!(this.release.len() > 1);
                if this.epoch != other.epoch {
                    return false;
                }

                if !this.release[..this.release.len() - 1]
                    .iter()
                    .zip(&other.release)
                    .all(|(this, other)| this == other)
                {
                    return false;
                }

                // According to PEP 440, this ignores the prerelease special rules
                // pypa/packaging disagrees: https://github.com/pypa/packaging/issues/617
                other >= this
            }
            Operator::GreaterThan => Self::greater_than(&this, &other),
            Operator::GreaterThanEqual => Self::greater_than(&this, &other) || other >= this,
            Operator::LessThan => {
                Self::less_than(&this, &other)
                    && !(version::compare_release(&this.release, &other.release) == Ordering::Equal
                        && other.any_prerelease())
            }
            Operator::LessThanEqual => Self::less_than(&this, &other) || other <= this,
        }
    }

    fn less_than(this: &Version, other: &Version) -> bool {
        if other.epoch < this.epoch {
            return true;
        }

        // This special case is here so that, unless the specifier itself
        // includes is a pre-release version, that we do not accept pre-release
        // versions for the version mentioned in the specifier (e.g. <3.1 should
        // not match 3.1.dev0, but should match 3.0.dev0).
        if !this.any_prerelease()
            && other.is_pre()
            && version::compare_release(&this.release, &other.release) == Ordering::Equal
        {
            return false;
        }

        other < this
    }

    fn greater_than(this: &Version, other: &Version) -> bool {
        if other.epoch > this.epoch {
            return true;
        }

        if version::compare_release(&this.release, &other.release) == Ordering::Equal {
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
}

impl FromStr for VersionSpecifier {
    type Err = String;

    /// Parses a version such as `>= 1.19`, `== 1.1.*`,`~=1.0+abc.5` or `<=1!2012.2`
    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let captures = VERSION_SPECIFIER_RE
            .captures(spec)
            .ok_or_else(|| format!("Version specifier `{spec}` doesn't match PEP 440 rules"))?;
        let (version, star) = Version::parse_impl(&captures)?;
        // operator but we don't know yet if it has a star
        let operator = Operator::from_str(&captures["operator"])?;
        let version_specifier = VersionSpecifier::new(operator, version, star)?;
        Ok(version_specifier)
    }
}

impl Display for VersionSpecifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.operator == Operator::EqualStar || self.operator == Operator::NotEqualStar {
            return write!(f, "{}{}.*", self.operator, self.version);
        }
        write!(f, "{}{}", self.operator, self.version)
    }
}

/// Parses a list of specifiers such as `>= 1.0, != 1.3.*, < 2.0`.
///
/// I recommend using [`VersionSpecifiers::from_str`] instead.
///
/// ```rust
/// use std::str::FromStr;
/// use pep440_rs::{parse_version_specifiers, Version};
///
/// let version = Version::from_str("1.19").unwrap();
/// let version_specifiers = parse_version_specifiers(">=1.16, <2.0").unwrap();
/// assert!(version_specifiers.iter().all(|specifier| specifier.contains(&version)));
/// ```
pub fn parse_version_specifiers(spec: &str) -> Result<Vec<VersionSpecifier>, Pep440Error> {
    let mut version_ranges = Vec::new();
    if spec.is_empty() {
        return Ok(version_ranges);
    }
    let mut start: usize = 0;
    let separator = ",";
    for version_range_spec in spec.split(separator) {
        match VersionSpecifier::from_str(version_range_spec) {
            Err(err) => {
                return Err(Pep440Error {
                    message: err,
                    line: spec.to_string(),
                    start,
                    width: version_range_spec.width(),
                });
            }
            Ok(version_range) => {
                version_ranges.push(version_range);
            }
        }
        start += version_range_spec.width();
        start += separator.width();
    }
    Ok(version_ranges)
}

#[cfg(test)]
mod test {
    use crate::{Operator, Version, VersionSpecifier, VersionSpecifiers};
    use indoc::indoc;
    use std::cmp::Ordering;
    use std::str::FromStr;

    /// <https://peps.python.org/pep-0440/#version-matching>
    #[test]
    fn test_equal() {
        let version = Version::from_str("1.1.post1").unwrap();

        assert!(!VersionSpecifier::from_str("== 1.1")
            .unwrap()
            .contains(&version));
        assert!(VersionSpecifier::from_str("== 1.1.post1")
            .unwrap()
            .contains(&version));
        assert!(VersionSpecifier::from_str("== 1.1.*")
            .unwrap()
            .contains(&version));
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
        let operations: Vec<_> = [
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
        .flatten()
        .collect();

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
        let versions: Vec<Version> = VERSIONS_0
            .iter()
            .map(|version| Version::from_str(version).unwrap())
            .collect();
        let specifiers: Vec<_> = SPECIFIERS_OTHER
            .iter()
            .map(|specifier| VersionSpecifier::from_str(specifier).unwrap())
            .collect();

        for (version, expected) in versions.iter().zip(EXPECTED_OTHER) {
            let actual = specifiers
                .iter()
                .map(|specifier| specifier.contains(version))
                .collect::<Vec<bool>>();
            for ((actual, expected), _specifier) in
                actual.iter().zip(expected).zip(SPECIFIERS_OTHER)
            {
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn test_arbitrary_equality() {
        assert!(VersionSpecifier::from_str("=== 1.2a1")
            .unwrap()
            .contains(&Version::from_str("1.2a1").unwrap()));
        assert!(!VersionSpecifier::from_str("=== 1.2a1")
            .unwrap()
            .contains(&Version::from_str("1.2a1+local").unwrap()));
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
            // Test the less than operation
            ("1", "<2"),
            ("2.0", "<2.1"),
            ("2.0.dev0", "<2.1"),
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

        for (version, specifier) in pairs {
            assert!(
                VersionSpecifier::from_str(specifier)
                    .unwrap()
                    .contains(&Version::from_str(version).unwrap()),
                "{version} {specifier}"
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
            result.0,
            [
                VersionSpecifier {
                    operator: Operator::TildeEqual,
                    version: Version {
                        epoch: 0,
                        release: vec![0, 9],
                        pre: None,
                        post: None,
                        dev: None,
                        local: None
                    }
                },
                VersionSpecifier {
                    operator: Operator::GreaterThanEqual,
                    version: Version {
                        epoch: 0,
                        release: vec![1, 0],
                        pre: None,
                        post: None,
                        dev: None,
                        local: None
                    }
                },
                VersionSpecifier {
                    operator: Operator::NotEqualStar,
                    version: Version {
                        epoch: 0,
                        release: vec![1, 3, 4],
                        pre: None,
                        post: None,
                        dev: None,
                        local: None
                    }
                },
                VersionSpecifier {
                    operator: Operator::LessThan,
                    version: Version {
                        epoch: 0,
                        release: vec![2, 0],
                        pre: None,
                        post: None,
                        dev: None,
                        local: None
                    }
                }
            ]
        );
    }

    #[test]
    fn test_parse_error() {
        let result = VersionSpecifiers::from_str("~= 0.9, %‍= 1.0, != 1.3.4.*");
        assert_eq!(
            result.unwrap_err().to_string(),
            indoc! {r#"
                Failed to parse version:
                ~= 0.9, %‍= 1.0, != 1.3.4.*
                       ^^^^^^^
            "#}
        );
    }

    #[test]
    fn test_non_star_after_star() {
        let result = VersionSpecifiers::from_str("== 0.9.*.1");
        assert_eq!(
            result.unwrap_err().message,
            "Version specifier `== 0.9.*.1` doesn't match PEP 440 rules"
        );
    }

    #[test]
    fn test_star_wrong_operator() {
        let result = VersionSpecifiers::from_str(">= 0.9.1.*");
        assert_eq!(
            result.unwrap_err().message,
            "Operator >= must not be used in version ending with a star"
        );
    }

    #[test]
    fn test_regex_mismatch() {
        let result = VersionSpecifiers::from_str("blergh");
        assert_eq!(
            result.unwrap_err().message,
            "Version specifier `blergh` doesn't match PEP 440 rules"
        );
    }

    /// <https://github.com/pypa/packaging/blob/e184feef1a28a5c574ec41f5c263a3a573861f5a/tests/test_specifiers.py#L44-L84>
    #[test]
    fn test_invalid_specifier() {
        let specifiers = [
            // Operator-less specifier
            ("2.0", None),
            // Invalid operator
            ("=>2.0", None),
            // Version-less specifier
            ("==", None),
            // Local segment on operators which don't support them
            (
                "~=1.0+5",
                Some("You can't mix a ~= operator with a local version (`+5`)"),
            ),
            (
                ">=1.0+deadbeef",
                Some("You can't mix a >= operator with a local version (`+deadbeef`)"),
            ),
            (
                "<=1.0+abc123",
                Some("You can't mix a <= operator with a local version (`+abc123`)"),
            ),
            (
                ">1.0+watwat",
                Some("You can't mix a > operator with a local version (`+watwat`)"),
            ),
            (
                "<1.0+1.0",
                Some("You can't mix a < operator with a local version (`+1.0`)"),
            ),
            // Prefix matching on operators which don't support them
            (
                "~=1.0.*",
                Some("Operator ~= must not be used in version ending with a star"),
            ),
            (
                ">=1.0.*",
                Some("Operator >= must not be used in version ending with a star"),
            ),
            (
                "<=1.0.*",
                Some("Operator <= must not be used in version ending with a star"),
            ),
            (
                ">1.0.*",
                Some("Operator > must not be used in version ending with a star"),
            ),
            (
                "<1.0.*",
                Some("Operator < must not be used in version ending with a star"),
            ),
            // Combination of local and prefix matching on operators which do
            // support one or the other
            (
                "==1.0.*+5",
                Some("Version specifier `==1.0.*+5` doesn't match PEP 440 rules"),
            ),
            (
                "!=1.0.*+deadbeef",
                Some("Version specifier `!=1.0.*+deadbeef` doesn't match PEP 440 rules"),
            ),
            // Prefix matching cannot be used with a pre-release, post-release,
            // dev or local version
            (
                "==2.0a1.*",
                Some("You can't have both a trailing `.*` and a prerelease version"),
            ),
            (
                "!=2.0a1.*",
                Some("You can't have both a trailing `.*` and a prerelease version"),
            ),
            (
                "==2.0.post1.*",
                Some("You can't have both a trailing `.*` and a post version"),
            ),
            (
                "!=2.0.post1.*",
                Some("You can't have both a trailing `.*` and a post version"),
            ),
            (
                "==2.0.dev1.*",
                Some("You can't have both a trailing `.*` and a dev version"),
            ),
            (
                "!=2.0.dev1.*",
                Some("You can't have both a trailing `.*` and a dev version"),
            ),
            (
                "==1.0+5.*",
                Some("You can't have both a trailing `.*` and a local version"),
            ),
            (
                "!=1.0+deadbeef.*",
                Some("You can't have both a trailing `.*` and a local version"),
            ),
            // Prefix matching must appear at the end
            (
                "==1.0.*.5",
                Some("Version specifier `==1.0.*.5` doesn't match PEP 440 rules"),
            ),
            // Compatible operator requires 2 digits in the release operator
            (
                "~=1",
                Some("The ~= operator requires at least two parts in the release version"),
            ),
            // Cannot use a prefix matching after a .devN version
            (
                "==1.0.dev1.*",
                Some("You can't have both a trailing `.*` and a dev version"),
            ),
            (
                "!=1.0.dev1.*",
                Some("You can't have both a trailing `.*` and a dev version"),
            ),
        ];
        for (specifier, error) in specifiers {
            if let Some(error) = error {
                assert_eq!(VersionSpecifier::from_str(specifier).unwrap_err(), error);
            } else {
                assert_eq!(
                    VersionSpecifier::from_str(specifier).unwrap_err(),
                    format!("Version specifier `{specifier}` doesn't match PEP 440 rules",)
                );
            }
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
            ">=3.7, <4.0, !=3.9.0"
        );
    }

    /// These occur in the simple api, e.g.
    /// <https://pypi.org/simple/geopandas/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn test_version_specifiers_empty() {
        assert_eq!(VersionSpecifiers::from_str("").unwrap().to_string(), "");
    }
}
