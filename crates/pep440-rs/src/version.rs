#[cfg(feature = "pyo3")]
use pyo3::{
    basic::CompareOp, exceptions::PyValueError, pyclass, pymethods, FromPyObject, IntoPy, PyAny,
    PyObject, PyResult, Python,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::sync::LazyLock;
use std::{
    borrow::Borrow,
    cmp::Ordering,
    hash::{Hash, Hasher},
    str::FromStr,
    sync::Arc,
};

/// One of `~=` `==` `!=` `<=` `>=` `<` `>` `===`
#[derive(
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    Debug,
    Hash,
    Clone,
    Copy,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
#[cfg_attr(feature = "pyo3", pyclass)]
pub enum Operator {
    /// `== 1.2.3`
    Equal,
    /// `== 1.2.*`
    EqualStar,
    /// `===` (discouraged)
    ///
    /// <https://peps.python.org/pep-0440/#arbitrary-equality>
    ///
    /// "Use of this operator is heavily discouraged and tooling MAY display a warning when it is used"
    // clippy doesn't like this: #[deprecated = "Use of this operator is heavily discouraged"]
    ExactEqual,
    /// `!= 1.2.3`
    NotEqual,
    /// `!= 1.2.*`
    NotEqualStar,
    /// `~=`
    TildeEqual,
    /// `<`
    LessThan,
    /// `<=`
    LessThanEqual,
    /// `>`
    GreaterThan,
    /// `>=`
    GreaterThanEqual,
}

impl Operator {
    /// Negates this operator, if a negation exists, so that it has the
    /// opposite meaning.
    ///
    /// This returns a negated operator in every case except for the `~=`
    /// operator. In that case, `None` is returned and callers may need to
    /// handle its negation at a higher level. (For example, if it's negated
    /// in the context of a marker expression, then the "compatible" version
    /// constraint can be split into its component parts and turned into a
    /// disjunction of the negation of each of those parts.)
    ///
    /// Note that this routine is not reversible in all cases. For example
    /// `Operator::ExactEqual` negates to `Operator::NotEqual`, and
    /// `Operator::NotEqual` in turn negates to `Operator::Equal`.
    pub fn negate(self) -> Option<Operator> {
        Some(match self {
            Operator::Equal => Operator::NotEqual,
            Operator::EqualStar => Operator::NotEqualStar,
            Operator::ExactEqual => Operator::NotEqual,
            Operator::NotEqual => Operator::Equal,
            Operator::NotEqualStar => Operator::EqualStar,
            Operator::TildeEqual => return None,
            Operator::LessThan => Operator::GreaterThanEqual,
            Operator::LessThanEqual => Operator::GreaterThan,
            Operator::GreaterThan => Operator::LessThanEqual,
            Operator::GreaterThanEqual => Operator::LessThan,
        })
    }

    /// Returns true if and only if this operator can be used in a version
    /// specifier with a version containing a non-empty local segment.
    ///
    /// Specifically, this comes from the "Local version identifiers are
    /// NOT permitted in this version specifier." phrasing in the version
    /// specifiers [spec].
    ///
    /// [spec]: https://packaging.python.org/en/latest/specifications/version-specifiers/
    pub(crate) fn is_local_compatible(self) -> bool {
        !matches!(
            self,
            Self::GreaterThan
                | Self::GreaterThanEqual
                | Self::LessThan
                | Self::LessThanEqual
                | Self::TildeEqual
                | Self::EqualStar
                | Self::NotEqualStar
        )
    }

    /// Returns the wildcard version of this operator, if appropriate.
    ///
    /// This returns `None` when this operator doesn't have an analogous
    /// wildcard operator.
    pub(crate) fn to_star(self) -> Option<Self> {
        match self {
            Self::Equal => Some(Self::EqualStar),
            Self::NotEqual => Some(Self::NotEqualStar),
            _ => None,
        }
    }

    /// Returns `true` if this operator represents a wildcard.
    pub fn is_star(self) -> bool {
        matches!(self, Self::EqualStar | Self::NotEqualStar)
    }
}

impl FromStr for Operator {
    type Err = OperatorParseError;

    /// Notably, this does not know about star versions, it just assumes the base operator
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let operator = match s {
            "==" => Self::Equal,
            "===" => {
                #[cfg(feature = "tracing")]
                {
                    tracing::warn!("Using arbitrary equality (`===`) is discouraged");
                }
                #[allow(deprecated)]
                Self::ExactEqual
            }
            "!=" => Self::NotEqual,
            "~=" => Self::TildeEqual,
            "<" => Self::LessThan,
            "<=" => Self::LessThanEqual,
            ">" => Self::GreaterThan,
            ">=" => Self::GreaterThanEqual,
            other => {
                return Err(OperatorParseError {
                    got: other.to_string(),
                })
            }
        };
        Ok(operator)
    }
}

impl std::fmt::Display for Operator {
    /// Note the `EqualStar` is also `==`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let operator = match self {
            Self::Equal => "==",
            // Beware, this doesn't print the star
            Self::EqualStar => "==",
            #[allow(deprecated)]
            Self::ExactEqual => "===",
            Self::NotEqual => "!=",
            Self::NotEqualStar => "!=",
            Self::TildeEqual => "~=",
            Self::LessThan => "<",
            Self::LessThanEqual => "<=",
            Self::GreaterThan => ">",
            Self::GreaterThanEqual => ">=",
        };

        write!(f, "{operator}")
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl Operator {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __str__(&self) -> String {
        self.to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        self.to_string()
    }
}

/// An error that occurs when parsing an invalid version specifier operator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorParseError {
    pub(crate) got: String,
}

impl std::error::Error for OperatorParseError {}

impl std::fmt::Display for OperatorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "no such comparison operator {:?}, must be one of ~= == != <= >= < > ===",
            self.got
        )
    }
}

// NOTE: I did a little bit of experimentation to determine what most version
// numbers actually look like. The idea here is that if we know what most look
// like, then we can optimize our representation for the common case, while
// falling back to something more complete for any cases that fall outside of
// that.
//
// The experiment downloaded PyPI's distribution metadata from Google BigQuery,
// and then counted the number of versions with various qualities:
//
//     total: 11264078
//     release counts:
//         01: 51204 (0.45%)
//         02: 754520 (6.70%)
//         03: 9757602 (86.63%)
//         04: 527403 (4.68%)
//         05: 77994 (0.69%)
//         06: 91346 (0.81%)
//         07: 1421 (0.01%)
//         08: 205 (0.00%)
//         09: 72 (0.00%)
//         10: 2297 (0.02%)
//         11: 5 (0.00%)
//         12: 2 (0.00%)
//         13: 4 (0.00%)
//         20: 2 (0.00%)
//         39: 1 (0.00%)
//     JUST release counts:
//         01: 48297 (0.43%)
//         02: 604692 (5.37%)
//         03: 8460917 (75.11%)
//         04: 465354 (4.13%)
//         05: 49293 (0.44%)
//         06: 25909 (0.23%)
//         07: 1413 (0.01%)
//         08: 192 (0.00%)
//         09: 72 (0.00%)
//         10: 2292 (0.02%)
//         11: 5 (0.00%)
//         12: 2 (0.00%)
//         13: 4 (0.00%)
//         20: 2 (0.00%)
//         39: 1 (0.00%)
//     non-zero epochs: 1902 (0.02%)
//     pre-releases: 752184 (6.68%)
//     post-releases: 134383 (1.19%)
//     dev-releases: 765099 (6.79%)
//     locals: 1 (0.00%)
//     fitsu8: 10388430 (92.23%)
//     sweetspot: 10236089 (90.87%)
//
// The "JUST release counts" corresponds to versions that only have a release
// component and nothing else. The "fitsu8" property indicates that all numbers
// (except for local numeric segments) fit into `u8`. The "sweetspot" property
// consists of any version number with no local part, 4 or fewer parts in the
// release version and *all* numbers fit into a u8.
//
// This somewhat confirms what one might expect: the vast majority of versions
// (75%) are precisely in the format of `x.y.z`. That is, a version with only a
// release version of 3 components.
//
// ---AG

/// A version number such as `1.2.3` or `4!5.6.7-a8.post9.dev0`.
///
/// Beware that the sorting implemented with [Ord] and [Eq] is not consistent with the operators
/// from PEP 440, i.e. compare two versions in rust with `>` gives a different result than a
/// `VersionSpecifier` with `>` as operator.
///
/// Parse with [`Version::from_str`]:
///
/// ```rust
/// use std::str::FromStr;
/// use pep440_rs::Version;
///
/// let version = Version::from_str("1.19").unwrap();
/// ```
#[derive(Clone, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
pub struct Version {
    inner: Arc<VersionInner>,
}

#[derive(Clone, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
enum VersionInner {
    Small { small: VersionSmall },
    Full { full: VersionFull },
}

impl Version {
    /// Create a new version from an iterator of segments in the release part
    /// of a version.
    ///
    /// # Panics
    ///
    /// When the iterator yields no elements.
    #[inline]
    pub fn new<I, R>(release_numbers: I) -> Self
    where
        I: IntoIterator<Item = R>,
        R: Borrow<u64>,
    {
        Self {
            inner: Arc::new(VersionInner::Small {
                small: VersionSmall::new(),
            }),
        }
        .with_release(release_numbers)
    }

    /// Whether this is an alpha/beta/rc or dev version
    #[inline]
    pub fn any_prerelease(&self) -> bool {
        self.is_pre() || self.is_dev()
    }

    /// Whether this is a stable version (i.e., _not_ an alpha/beta/rc or dev version)
    #[inline]
    pub fn is_stable(&self) -> bool {
        !self.is_pre() && !self.is_dev()
    }

    /// Whether this is an alpha/beta/rc version
    #[inline]
    pub fn is_pre(&self) -> bool {
        self.pre().is_some()
    }

    /// Whether this is a dev version
    #[inline]
    pub fn is_dev(&self) -> bool {
        self.dev().is_some()
    }

    /// Whether this is a post version
    #[inline]
    pub fn is_post(&self) -> bool {
        self.post().is_some()
    }

    /// Whether this is a local version (e.g. `1.2.3+localsuffixesareweird`)
    ///
    /// When true, it is guaranteed that the slice returned by
    /// [`Version::local`] is non-empty.
    #[inline]
    pub fn is_local(&self) -> bool {
        !self.local().is_empty()
    }

    /// Returns the epoch of this version.
    #[inline]
    pub fn epoch(&self) -> u64 {
        match *self.inner {
            VersionInner::Small { ref small } => small.epoch(),
            VersionInner::Full { ref full } => full.epoch,
        }
    }

    /// Returns the release number part of the version.
    #[inline]
    pub fn release(&self) -> &[u64] {
        match *self.inner {
            VersionInner::Small { ref small } => small.release(),
            VersionInner::Full { ref full, .. } => &full.release,
        }
    }

    /// Returns the pre-release part of this version, if it exists.
    #[inline]
    pub fn pre(&self) -> Option<Prerelease> {
        match *self.inner {
            VersionInner::Small { ref small } => small.pre(),
            VersionInner::Full { ref full } => full.pre,
        }
    }

    /// Returns the post-release part of this version, if it exists.
    #[inline]
    pub fn post(&self) -> Option<u64> {
        match *self.inner {
            VersionInner::Small { ref small } => small.post(),
            VersionInner::Full { ref full } => full.post,
        }
    }

    /// Returns the dev-release part of this version, if it exists.
    #[inline]
    pub fn dev(&self) -> Option<u64> {
        match *self.inner {
            VersionInner::Small { ref small } => small.dev(),
            VersionInner::Full { ref full } => full.dev,
        }
    }

    /// Returns the local segments in this version, if any exist.
    #[inline]
    pub fn local(&self) -> &[LocalSegment] {
        match *self.inner {
            VersionInner::Small { ref small } => small.local(),
            VersionInner::Full { ref full } => &full.local,
        }
    }

    /// Returns the min-release part of this version, if it exists.
    ///
    /// The "min" component is internal-only, and does not exist in PEP 440.
    /// The version `1.0min0` is smaller than all other `1.0` versions,
    /// like `1.0a1`, `1.0dev0`, etc.
    #[inline]
    pub fn min(&self) -> Option<u64> {
        match *self.inner {
            VersionInner::Small { ref small } => small.min(),
            VersionInner::Full { ref full } => full.min,
        }
    }

    /// Returns the max-release part of this version, if it exists.
    ///
    /// The "max" component is internal-only, and does not exist in PEP 440.
    /// The version `1.0max0` is larger than all other `1.0` versions,
    /// like `1.0.post1`, `1.0+local`, etc.
    #[inline]
    pub fn max(&self) -> Option<u64> {
        match *self.inner {
            VersionInner::Small { ref small } => small.max(),
            VersionInner::Full { ref full } => full.max,
        }
    }

    /// Set the release numbers and return the updated version.
    ///
    /// Usually one can just use `Version::new` to create a new version with
    /// the updated release numbers, but this is useful when one wants to
    /// preserve the other components of a version number while only changing
    /// the release numbers.
    ///
    /// # Panics
    ///
    /// When the iterator yields no elements.
    #[inline]
    #[must_use]
    pub fn with_release<I, R>(mut self, release_numbers: I) -> Self
    where
        I: IntoIterator<Item = R>,
        R: Borrow<u64>,
    {
        self.clear_release();
        for n in release_numbers {
            self.push_release(*n.borrow());
        }
        assert!(
            !self.release().is_empty(),
            "release must have non-zero size"
        );
        self
    }

    /// Push the given release number into this version. It will become the
    /// last number in the release component.
    #[inline]
    fn push_release(&mut self, n: u64) {
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.push_release(n) {
                return;
            }
        }
        self.make_full().release.push(n);
    }

    /// Clears the release component of this version so that it has no numbers.
    ///
    /// Generally speaking, this empty state should not be exposed to callers
    /// since all versions should have at least one release number.
    #[inline]
    fn clear_release(&mut self) {
        match Arc::make_mut(&mut self.inner) {
            VersionInner::Small { ref mut small } => small.clear_release(),
            VersionInner::Full { ref mut full } => {
                full.release.clear();
            }
        }
    }

    /// Set the epoch and return the updated version.
    #[inline]
    #[must_use]
    pub fn with_epoch(mut self, value: u64) -> Self {
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_epoch(value) {
                return self;
            }
        }
        self.make_full().epoch = value;
        self
    }

    /// Set the pre-release component and return the updated version.
    #[inline]
    #[must_use]
    pub fn with_pre(mut self, value: Option<Prerelease>) -> Self {
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_pre(value) {
                return self;
            }
        }
        self.make_full().pre = value;
        self
    }

    /// Set the post-release component and return the updated version.
    #[inline]
    #[must_use]
    pub fn with_post(mut self, value: Option<u64>) -> Self {
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_post(value) {
                return self;
            }
        }
        self.make_full().post = value;
        self
    }

    /// Set the dev-release component and return the updated version.
    #[inline]
    #[must_use]
    pub fn with_dev(mut self, value: Option<u64>) -> Self {
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_dev(value) {
                return self;
            }
        }
        self.make_full().dev = value;
        self
    }

    /// Set the local segments and return the updated version.
    #[inline]
    #[must_use]
    pub fn with_local(mut self, value: Vec<LocalSegment>) -> Self {
        if value.is_empty() {
            self.without_local()
        } else {
            self.make_full().local = value;
            self
        }
    }

    /// For PEP 440 specifier matching: "Except where specifically noted below,
    /// local version identifiers MUST NOT be permitted in version specifiers,
    /// and local version labels MUST be ignored entirely when checking if
    /// candidate versions match a given version specifier."
    #[inline]
    #[must_use]
    pub fn without_local(mut self) -> Self {
        // A "small" version is already guaranteed not to have a local
        // component, so we only need to do anything if we have a "full"
        // version.
        if let VersionInner::Full { ref mut full } = Arc::make_mut(&mut self.inner) {
            full.local.clear();
        }
        self
    }

    /// Return the version with any segments apart from the release removed.
    #[inline]
    #[must_use]
    pub fn only_release(&self) -> Self {
        Self::new(self.release().iter().copied())
    }

    /// Set the min-release component and return the updated version.
    ///
    /// The "min" component is internal-only, and does not exist in PEP 440.
    /// The version `1.0min0` is smaller than all other `1.0` versions,
    /// like `1.0a1`, `1.0dev0`, etc.
    #[inline]
    #[must_use]
    pub fn with_min(mut self, value: Option<u64>) -> Self {
        debug_assert!(!self.is_pre(), "min is not allowed on pre-release versions");
        debug_assert!(!self.is_dev(), "min is not allowed on dev versions");
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_min(value) {
                return self;
            }
        }
        self.make_full().min = value;
        self
    }

    /// Set the max-release component and return the updated version.
    ///
    /// The "max" component is internal-only, and does not exist in PEP 440.
    /// The version `1.0max0` is larger than all other `1.0` versions,
    /// like `1.0.post1`, `1.0+local`, etc.
    #[inline]
    #[must_use]
    pub fn with_max(mut self, value: Option<u64>) -> Self {
        debug_assert!(
            !self.is_post(),
            "max is not allowed on post-release versions"
        );
        debug_assert!(!self.is_dev(), "max is not allowed on dev versions");
        if let VersionInner::Small { ref mut small } = Arc::make_mut(&mut self.inner) {
            if small.set_max(value) {
                return self;
            }
        }
        self.make_full().max = value;
        self
    }

    /// Convert this version to a "full" representation in-place and return a
    /// mutable borrow to the full type.
    fn make_full(&mut self) -> &mut VersionFull {
        if let VersionInner::Small { ref small } = *self.inner {
            let full = VersionFull {
                epoch: small.epoch(),
                release: small.release().to_vec(),
                min: small.min(),
                max: small.max(),
                pre: small.pre(),
                post: small.post(),
                dev: small.dev(),
                local: vec![],
            };
            *self = Self {
                inner: Arc::new(VersionInner::Full { full }),
            };
        }
        match Arc::make_mut(&mut self.inner) {
            VersionInner::Full { ref mut full } => full,
            VersionInner::Small { .. } => unreachable!(),
        }
    }

    /// Performs a "slow" but complete comparison between two versions.
    ///
    /// This comparison is done using only the public API of a `Version`, and
    /// is thus independent of its specific representation. This is useful
    /// to use when comparing two versions that aren't *both* the small
    /// representation.
    #[cold]
    #[inline(never)]
    fn cmp_slow(&self, other: &Self) -> Ordering {
        match self.epoch().cmp(&other.epoch()) {
            Ordering::Less => {
                return Ordering::Less;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                return Ordering::Greater;
            }
        }

        match compare_release(self.release(), other.release()) {
            Ordering::Less => {
                return Ordering::Less;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                return Ordering::Greater;
            }
        }

        // release is equal, so compare the other parts
        sortable_tuple(self).cmp(&sortable_tuple(other))
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

/// Shows normalized version
impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let epoch = if self.epoch() == 0 {
            String::new()
        } else {
            format!("{}!", self.epoch())
        };
        let release = self
            .release()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(".");
        let pre = self
            .pre()
            .as_ref()
            .map(|Prerelease { kind, number }| format!("{kind}{number}"))
            .unwrap_or_default();
        let post = self
            .post()
            .map(|post| format!(".post{post}"))
            .unwrap_or_default();
        let dev = self
            .dev()
            .map(|dev| format!(".dev{dev}"))
            .unwrap_or_default();
        let local = if self.local().is_empty() {
            String::new()
        } else {
            format!(
                "+{}",
                self.local()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join(".")
            )
        };
        write!(f, "{epoch}{release}{pre}{post}{dev}{local}")
    }
}

impl std::fmt::Debug for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{self}\"")
    }
}

impl PartialEq<Self> for Version {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Hash for Version {
    /// Custom implementation to ignoring trailing zero because `PartialEq` zero pads
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch().hash(state);
        // Skip trailing zeros
        for i in self.release().iter().rev().skip_while(|x| **x == 0) {
            i.hash(state);
        }
        self.pre().hash(state);
        self.dev().hash(state);
        self.post().hash(state);
        self.local().hash(state);
    }
}

impl PartialOrd<Self> for Version {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// 1.0.dev456 < 1.0a1 < 1.0a2.dev456 < 1.0a12.dev456 < 1.0a12 < 1.0b1.dev456 < 1.0b2
    /// < 1.0b2.post345.dev456 < 1.0b2.post345 < 1.0b2-346 < 1.0c1.dev456 < 1.0c1 < 1.0rc2 < 1.0c3
    /// < 1.0 < 1.0.post456.dev34 < 1.0.post456
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        match (&*self.inner, &*other.inner) {
            (VersionInner::Small { small: small1 }, VersionInner::Small { small: small2 }) => {
                small1.repr.cmp(&small2.repr)
            }
            _ => self.cmp_slow(other),
        }
    }
}

impl FromStr for Version {
    type Err = VersionParseError;

    /// Parses a version such as `1.19`, `1.0a1`,`1.0+abc.5` or `1!2012.2`
    ///
    /// Note that this doesn't allow wildcard versions.
    fn from_str(version: &str) -> Result<Self, Self::Err> {
        Parser::new(version.as_bytes()).parse()
    }
}

/// A "small" representation of a version.
///
/// This representation is used for a (very common) subset of versions: the
/// set of all versions with ~small numbers and no local component. The
/// representation is designed to be (somewhat) compact, but also laid out in
/// a way that makes comparisons between two small versions equivalent to a
/// simple `memcmp`.
///
/// The methods on this type encapsulate the representation. Since this type
/// cannot represent the full range of all versions, setters on this type will
/// return `false` if the value could not be stored. In this case, callers
/// should generally convert a version into its "full" representation and then
/// set the value on the full type.
///
/// # Representation
///
/// At time of writing, this representation supports versions that meet all of
/// the following criteria:
///
/// * The epoch must be `0`.
/// * The release portion must have 4 or fewer segments.
/// * All release segments, except for the first, must be representable in a
///   `u8`. The first segment must be representable in a `u16`. (This permits
///   calendar versions, like `2023.03`, to be represented.)
/// * There is *at most* one of the following components: pre, dev or post.
/// * If there is a pre segment, then its numeric value is less than 64.
/// * If there is a dev or post segment, then its value is less than `u8::MAX`.
/// * There are zero "local" segments.
///
/// The above constraints were chosen as a balancing point between being able
/// to represent all parts of a version in a very small amount of space,
/// and for supporting as many versions in the wild as possible. There is,
/// however, another constraint in play here: comparisons between two `Version`
/// values. It turns out that we do a lot of them as part of resolution, and
/// the cheaper we can make that, the better. This constraint pushes us
/// toward using as little space as possible. Indeed, here, comparisons are
/// implemented via `u64::cmp`.
///
/// We pack versions fitting the above constraints into a `u64` in such a way
/// that it preserves the ordering between versions as prescribed in PEP 440.
/// Namely:
///
/// * Bytes 6 and 7 correspond to the first release segment as a `u16`.
/// * Bytes 5, 4 and 3 correspond to the second, third and fourth release
///   segments, respectively.
/// * Bytes 2, 1 and 0 represent *one* of the following:
///   `min, .devN, aN, bN, rcN, <no suffix>, .postN, max`.
///   Its representation is thus:
///   * The most significant 3 bits of Byte 2 corresponds to a value in
///     the range 0-6 inclusive, corresponding to min, dev, pre-a, pre-b, pre-rc,
///     no-suffix or post releases, respectively. `min` is a special version that
///     does not exist in PEP 440, but is used here to represent the smallest
///     possible version, preceding any `dev`, `pre`, `post` or releases. `max` is
///     an analogous concept for the largest possible version, following any `post`
///     or local releases.
///   * The low 5 bits combined with the bits in bytes 1 and 0 correspond
///     to the release number of the suffix, if one exists. If there is no
///     suffix, then these bits are always 0.
///
/// The order of the encoding above is significant. For example, suffixes are
/// encoded at a less significant location than the release numbers, so that
/// `1.2.3 < 1.2.3.post4`.
///
/// In a previous representation, we tried to encode the suffixes in different
/// locations so that, in theory, you could represent `1.2.3.dev2.post3` in the
/// packed form. But getting the ordering right for this is difficult (perhaps
/// impossible without extra space?). So we limited to only storing one suffix.
/// But even then, we wound up with a bug where `1.0dev1 > 1.0a1`, when of
/// course, all dev releases should compare less than pre releases. This was
/// because the encoding recorded the pre-release as "absent", and this in turn
/// screwed up the order comparisons.
///
/// Thankfully, such versions are incredibly rare. Virtually all versions have
/// zero or one pre, dev or post release components.
#[derive(Clone, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
struct VersionSmall {
    /// The representation discussed above.
    repr: u64,
    /// The `u64` numbers in the release component.
    ///
    /// These are *only* used to implement the public API `Version::release`
    /// method. This is necessary in order to provide a `&[u64]` to the caller.
    /// If we didn't need the public API, or could re-work it, then we could
    /// get rid of this extra storage. (Which is indeed duplicative of what is
    /// stored in `repr`.) Note that this uses `u64` not because it can store
    /// bigger numbers than what's in `repr` (it can't), but so that it permits
    /// us to return a `&[u64]`.
    ///
    /// I believe there is only one way to get rid of this extra storage:
    /// change the public API so that it doesn't return a `&[u64]`. Instead,
    /// we'd return a new type that conceptually represents a `&[u64]`, but may
    /// use a different representation based on what kind of `Version` it came
    /// from. The downside of this approach is that one loses the flexibility
    /// of a simple `&[u64]`. (Which, at time of writing, is taken advantage of
    /// in several places via slice patterns.) But, if we needed to change it,
    /// we could do it without losing expressivity, but losing convenience.
    release: [u64; 4],
    /// The number of segments in the release component.
    ///
    /// Strictly speaking, this isn't necessary since `1.2` is considered
    /// equivalent to `1.2.0.0`. But in practice it's nice to be able
    /// to truncate the zero components. And always filling out to 4
    /// places somewhat exposes internal details, since the "full" version
    /// representation would not do that.
    len: u8,
}

impl VersionSmall {
    const SUFFIX_MIN: u64 = 0;
    const SUFFIX_DEV: u64 = 1;
    const SUFFIX_PRE_ALPHA: u64 = 2;
    const SUFFIX_PRE_BETA: u64 = 3;
    const SUFFIX_PRE_RC: u64 = 4;
    const SUFFIX_NONE: u64 = 5;
    const SUFFIX_POST: u64 = 6;
    const SUFFIX_MAX: u64 = 7;
    const SUFFIX_MAX_VERSION: u64 = 0x001F_FFFF;

    #[inline]
    fn new() -> Self {
        Self {
            repr: 0x0000_0000_00A0_0000,
            release: [0, 0, 0, 0],
            len: 0,
        }
    }

    #[inline]
    #[allow(clippy::unused_self)]
    fn epoch(&self) -> u64 {
        0
    }

    #[inline]
    #[allow(clippy::unused_self)]
    fn set_epoch(&mut self, value: u64) -> bool {
        if value != 0 {
            return false;
        }
        true
    }

    #[inline]
    fn release(&self) -> &[u64] {
        &self.release[..usize::from(self.len)]
    }

    #[inline]
    fn clear_release(&mut self) {
        self.repr &= !0xFFFF_FFFF_FF00_0000;
        self.release = [0, 0, 0, 0];
        self.len = 0;
    }

    #[inline]
    fn push_release(&mut self, n: u64) -> bool {
        if self.len == 0 {
            if n > u64::from(u16::MAX) {
                return false;
            }
            self.repr |= n << 48;
            self.release[0] = n;
            self.len = 1;
            true
        } else {
            if n > u64::from(u8::MAX) {
                return false;
            }
            if self.len >= 4 {
                return false;
            }
            let shift = 48 - (usize::from(self.len) * 8);
            self.repr |= n << shift;
            self.release[usize::from(self.len)] = n;
            self.len += 1;
            true
        }
    }

    #[inline]
    fn post(&self) -> Option<u64> {
        if self.suffix_kind() == Self::SUFFIX_POST {
            Some(self.suffix_version())
        } else {
            None
        }
    }

    #[inline]
    fn set_post(&mut self, value: Option<u64>) -> bool {
        if self.min().is_some()
            || self.pre().is_some()
            || self.dev().is_some()
            || self.max().is_some()
        {
            return value.is_none();
        }
        match value {
            None => {
                self.set_suffix_kind(Self::SUFFIX_NONE);
            }
            Some(number) => {
                if number > Self::SUFFIX_MAX_VERSION {
                    return false;
                }
                self.set_suffix_kind(Self::SUFFIX_POST);
                self.set_suffix_version(number);
            }
        }
        true
    }

    #[inline]
    fn pre(&self) -> Option<Prerelease> {
        let (kind, number) = (self.suffix_kind(), self.suffix_version());
        if kind == Self::SUFFIX_PRE_ALPHA {
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number,
            })
        } else if kind == Self::SUFFIX_PRE_BETA {
            Some(Prerelease {
                kind: PrereleaseKind::Beta,
                number,
            })
        } else if kind == Self::SUFFIX_PRE_RC {
            Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number,
            })
        } else {
            None
        }
    }

    #[inline]
    fn set_pre(&mut self, value: Option<Prerelease>) -> bool {
        if self.min().is_some()
            || self.dev().is_some()
            || self.post().is_some()
            || self.max().is_some()
        {
            return value.is_none();
        }
        match value {
            None => {
                self.set_suffix_kind(Self::SUFFIX_NONE);
            }
            Some(Prerelease { kind, number }) => {
                if number > Self::SUFFIX_MAX_VERSION {
                    return false;
                }
                match kind {
                    PrereleaseKind::Alpha => {
                        self.set_suffix_kind(Self::SUFFIX_PRE_ALPHA);
                    }
                    PrereleaseKind::Beta => {
                        self.set_suffix_kind(Self::SUFFIX_PRE_BETA);
                    }
                    PrereleaseKind::Rc => {
                        self.set_suffix_kind(Self::SUFFIX_PRE_RC);
                    }
                }
                self.set_suffix_version(number);
            }
        }
        true
    }

    #[inline]
    fn dev(&self) -> Option<u64> {
        if self.suffix_kind() == Self::SUFFIX_DEV {
            Some(self.suffix_version())
        } else {
            None
        }
    }

    #[inline]
    fn set_dev(&mut self, value: Option<u64>) -> bool {
        if self.min().is_some()
            || self.pre().is_some()
            || self.post().is_some()
            || self.max().is_some()
        {
            return value.is_none();
        }
        match value {
            None => {
                self.set_suffix_kind(Self::SUFFIX_NONE);
            }
            Some(number) => {
                if number > Self::SUFFIX_MAX_VERSION {
                    return false;
                }
                self.set_suffix_kind(Self::SUFFIX_DEV);
                self.set_suffix_version(number);
            }
        }
        true
    }

    #[inline]
    fn min(&self) -> Option<u64> {
        if self.suffix_kind() == Self::SUFFIX_MIN {
            Some(self.suffix_version())
        } else {
            None
        }
    }

    #[inline]
    fn set_min(&mut self, value: Option<u64>) -> bool {
        if self.dev().is_some()
            || self.pre().is_some()
            || self.post().is_some()
            || self.max().is_some()
        {
            return value.is_none();
        }
        match value {
            None => {
                self.set_suffix_kind(Self::SUFFIX_NONE);
            }
            Some(number) => {
                if number > Self::SUFFIX_MAX_VERSION {
                    return false;
                }
                self.set_suffix_kind(Self::SUFFIX_MIN);
                self.set_suffix_version(number);
            }
        }
        true
    }

    #[inline]
    fn max(&self) -> Option<u64> {
        if self.suffix_kind() == Self::SUFFIX_MAX {
            Some(self.suffix_version())
        } else {
            None
        }
    }

    #[inline]
    fn set_max(&mut self, value: Option<u64>) -> bool {
        if self.dev().is_some()
            || self.pre().is_some()
            || self.post().is_some()
            || self.min().is_some()
        {
            return value.is_none();
        }
        match value {
            None => {
                self.set_suffix_kind(Self::SUFFIX_NONE);
            }
            Some(number) => {
                if number > Self::SUFFIX_MAX_VERSION {
                    return false;
                }
                self.set_suffix_kind(Self::SUFFIX_MAX);
                self.set_suffix_version(number);
            }
        }
        true
    }

    #[inline]
    #[allow(clippy::unused_self)]
    fn local(&self) -> &[LocalSegment] {
        // A "small" version is never used if the version has a non-zero number
        // of local segments.
        &[]
    }

    #[inline]
    fn suffix_kind(&self) -> u64 {
        let kind = (self.repr >> 21) & 0b111;
        debug_assert!(kind <= Self::SUFFIX_MAX);
        kind
    }

    #[inline]
    fn set_suffix_kind(&mut self, kind: u64) {
        debug_assert!(kind <= Self::SUFFIX_MAX);
        self.repr &= !0x00E0_0000;
        self.repr |= kind << 21;
        if kind == Self::SUFFIX_NONE {
            self.set_suffix_version(0);
        }
    }

    #[inline]
    fn suffix_version(&self) -> u64 {
        self.repr & 0x001F_FFFF
    }

    #[inline]
    fn set_suffix_version(&mut self, value: u64) {
        debug_assert!(value <= 0x001F_FFFF);
        self.repr &= !0x001F_FFFF;
        self.repr |= value;
    }
}

/// The "full" representation of a version.
///
/// This can represent all possible versions, but is a bit beefier because of
/// it. It also uses some indirection for variable length data such as the
/// release numbers and the local segments.
///
/// In general, the "full" representation is rarely used in practice since most
/// versions will fit into the "small" representation.
#[derive(Clone, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
struct VersionFull {
    /// The [versioning
    /// epoch](https://peps.python.org/pep-0440/#version-epochs). Normally
    /// just 0, but you can increment it if you switched the versioning
    /// scheme.
    epoch: u64,
    /// The normal number part of the version (["final
    /// release"](https://peps.python.org/pep-0440/#final-releases)), such
    /// a `1.2.3` in `4!1.2.3-a8.post9.dev1`
    ///
    /// Note that we drop the * placeholder by moving it to `Operator`
    release: Vec<u64>,
    /// The [prerelease](https://peps.python.org/pep-0440/#pre-releases),
    /// i.e. alpha, beta or rc plus a number
    ///
    /// Note that whether this is Some influences the version range
    /// matching since normally we exclude all pre-release versions
    pre: Option<Prerelease>,
    /// The [Post release
    /// version](https://peps.python.org/pep-0440/#post-releases), higher
    /// post version are preferred over lower post or none-post versions
    post: Option<u64>,
    /// The [developmental
    /// release](https://peps.python.org/pep-0440/#developmental-releases),
    /// if any
    dev: Option<u64>,
    /// A [local version
    /// identifier](https://peps.python.org/pep-0440/#local-version-identifiers)
    /// such as `+deadbeef` in `1.2.3+deadbeef`
    ///
    /// > They consist of a normal public version identifier (as defined
    /// > in the previous section), along with an arbitrary “local version
    /// > label”, separated from the public version identifier by a plus.
    /// > Local version labels have no specific semantics assigned, but
    /// > some syntactic restrictions are imposed.
    local: Vec<LocalSegment>,
    /// An internal-only segment that does not exist in PEP 440, used to
    /// represent the smallest possible version of a release, preceding any
    /// `dev`, `pre`, `post` or releases.
    min: Option<u64>,
    /// An internal-only segment that does not exist in PEP 440, used to
    /// represent the largest possible version of a release, following any
    /// `post` or local releases.
    max: Option<u64>,
}

/// A version number pattern.
///
/// A version pattern appears in a
/// [`VersionSpecifier`](crate::VersionSpecifier). It is just like a version,
/// except that it permits a trailing `*` (wildcard) at the end of the version
/// number. The wildcard indicates that any version with the same prefix should
/// match.
///
/// A `VersionPattern` cannot do any matching itself. Instead,
/// it needs to be paired with an [`Operator`] to create a
/// [`VersionSpecifier`](crate::VersionSpecifier).
///
/// Here are some valid and invalid examples:
///
/// * `1.2.3` -> verbatim pattern
/// * `1.2.3.*` -> wildcard pattern
/// * `1.2.*.4` -> invalid
/// * `1.0-dev1.*` -> invalid
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VersionPattern {
    version: Version,
    wildcard: bool,
}

impl VersionPattern {
    /// Creates a new verbatim version pattern that matches the given
    /// version exactly.
    #[inline]
    pub fn verbatim(version: Version) -> Self {
        Self {
            version,
            wildcard: false,
        }
    }

    /// Creates a new wildcard version pattern that matches any version with
    /// the given version as a prefix.
    #[inline]
    pub fn wildcard(version: Version) -> Self {
        Self {
            version,
            wildcard: true,
        }
    }

    /// Returns the underlying version.
    #[inline]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Consumes this pattern and returns ownership of the underlying version.
    #[inline]
    pub fn into_version(self) -> Version {
        self.version
    }

    /// Returns true if and only if this pattern contains a wildcard.
    #[inline]
    pub fn is_wildcard(&self) -> bool {
        self.wildcard
    }
}

impl FromStr for VersionPattern {
    type Err = VersionPatternParseError;

    fn from_str(version: &str) -> Result<Self, VersionPatternParseError> {
        Parser::new(version.as_bytes()).parse_pattern()
    }
}

/// An optional pre-release modifier and number applied to a version.
#[derive(
    PartialEq,
    Eq,
    Debug,
    Hash,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct Prerelease {
    /// The kind of pre-release.
    pub kind: PrereleaseKind,
    /// The number associated with the pre-release.
    pub number: u64,
}

/// Optional pre-release modifier (alpha, beta or release candidate) appended to version
///
/// <https://peps.python.org/pep-0440/#pre-releases>
#[derive(
    PartialEq,
    Eq,
    Debug,
    Hash,
    Clone,
    Copy,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
#[cfg_attr(feature = "pyo3", pyclass)]
pub enum PrereleaseKind {
    /// alpha pre-release
    Alpha,
    /// beta pre-release
    Beta,
    /// release candidate pre-release
    Rc,
}

impl std::fmt::Display for PrereleaseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alpha => write!(f, "a"),
            Self::Beta => write!(f, "b"),
            Self::Rc => write!(f, "rc"),
        }
    }
}

impl std::fmt::Display for Prerelease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.kind, self.number)
    }
}

/// A part of the [local version identifier](<https://peps.python.org/pep-0440/#local-version-identifiers>)
///
/// Local versions are a mess:
///
/// > Comparison and ordering of local versions considers each segment of the local version
/// > (divided by a .) separately. If a segment consists entirely of ASCII digits then that section
/// > should be considered an integer for comparison purposes and if a segment contains any ASCII
/// > letters then that segment is compared lexicographically with case insensitivity. When
/// > comparing a numeric and lexicographic segment, the numeric section always compares as greater
/// > than the lexicographic segment. Additionally a local version with a great number of segments
/// > will always compare as greater than a local version with fewer segments, as long as the
/// > shorter local version’s segments match the beginning of the longer local version’s segments
/// > exactly.
///
/// Luckily the default `Ord` implementation for `Vec<LocalSegment>` matches the PEP 440 rules.
#[derive(Eq, PartialEq, Debug, Clone, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug, Eq, PartialEq, PartialOrd, Ord))]
pub enum LocalSegment {
    /// Not-parseable as integer segment of local version
    String(String),
    /// Inferred integer segment of local version
    Number(u64),
}

impl std::fmt::Display for LocalSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(string) => write!(f, "{string}"),
            Self::Number(number) => write!(f, "{number}"),
        }
    }
}

impl PartialOrd for LocalSegment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LocalSegment {
    fn cmp(&self, other: &Self) -> Ordering {
        // <https://peps.python.org/pep-0440/#local-version-identifiers>
        match (self, other) {
            (Self::Number(n1), Self::Number(n2)) => n1.cmp(n2),
            (Self::String(s1), Self::String(s2)) => s1.cmp(s2),
            (Self::Number(_), Self::String(_)) => Ordering::Greater,
            (Self::String(_), Self::Number(_)) => Ordering::Less,
        }
    }
}

/// The state used for [parsing a version][pep440].
///
/// This parses the most "flexible" format of a version as described in the
/// "normalization" section of PEP 440.
///
/// This can also parse a version "pattern," which essentially is just like
/// parsing a version, but permits a trailing wildcard. e.g., `1.2.*`.
///
/// [pep440]: https://packaging.python.org/en/latest/specifications/version-specifiers/
#[derive(Debug)]
struct Parser<'a> {
    /// The version string we are parsing.
    v: &'a [u8],
    /// The current position of the parser.
    i: usize,
    /// The epoch extracted from the version.
    epoch: u64,
    /// The release numbers extracted from the version.
    release: ReleaseNumbers,
    /// The pre-release version, if any.
    pre: Option<Prerelease>,
    /// The post-release version, if any.
    post: Option<u64>,
    /// The dev release, if any.
    dev: Option<u64>,
    /// The local segments, if any.
    local: Vec<LocalSegment>,
    /// Whether a wildcard at the end of the version was found or not.
    ///
    /// This is only valid when a version pattern is being parsed.
    wildcard: bool,
}

impl<'a> Parser<'a> {
    /// The "separators" that are allowed in several different parts of a
    /// version.
    #[allow(clippy::byte_char_slices)]
    const SEPARATOR: ByteSet = ByteSet::new(&[b'.', b'_', b'-']);

    /// Create a new `Parser` for parsing the version in the given byte string.
    fn new(version: &'a [u8]) -> Parser<'a> {
        Parser {
            v: version,
            i: 0,
            epoch: 0,
            release: ReleaseNumbers::new(),
            pre: None,
            post: None,
            dev: None,
            local: vec![],
            wildcard: false,
        }
    }

    /// Parse a verbatim version.
    ///
    /// If a version pattern is found, then an error is returned.
    fn parse(self) -> Result<Version, VersionParseError> {
        match self.parse_pattern() {
            Ok(vpat) => {
                if vpat.is_wildcard() {
                    Err(ErrorKind::Wildcard.into())
                } else {
                    Ok(vpat.into_version())
                }
            }
            // If we get an error when parsing a version pattern, then
            // usually it will actually just be a VersionParseError.
            // But if it's specific to version patterns, and since
            // we are expecting a verbatim version here, we can just
            // return a generic "wildcards not allowed" error in that
            // case.
            Err(err) => match *err.kind {
                PatternErrorKind::Version(err) => Err(err),
                PatternErrorKind::WildcardNotTrailing => Err(ErrorKind::Wildcard.into()),
            },
        }
    }

    /// Parse a version pattern, which may be a verbatim version.
    fn parse_pattern(mut self) -> Result<VersionPattern, VersionPatternParseError> {
        if let Some(vpat) = self.parse_fast() {
            return Ok(vpat);
        }
        self.bump_while(|byte| byte.is_ascii_whitespace());
        self.bump_if("v");
        self.parse_epoch_and_initial_release()?;
        self.parse_rest_of_release()?;
        if self.parse_wildcard()? {
            return Ok(self.into_pattern());
        }
        self.parse_pre()?;
        self.parse_post()?;
        self.parse_dev()?;
        self.parse_local()?;
        self.bump_while(|byte| byte.is_ascii_whitespace());
        if !self.is_done() {
            let version = String::from_utf8_lossy(&self.v[..self.i]).into_owned();
            let remaining = String::from_utf8_lossy(&self.v[self.i..]).into_owned();
            return Err(ErrorKind::UnexpectedEnd { version, remaining }.into());
        }
        Ok(self.into_pattern())
    }

    /// Attempts to do a "fast parse" of a version.
    ///
    /// This looks for versions of the form `w[.x[.y[.z]]]` while
    /// simultaneously parsing numbers. This format corresponds to the
    /// overwhelming majority of all version strings and can avoid most of the
    /// work done in the more general parser.
    ///
    /// If the version string is not in the format of `w[.x[.y[.z]]]`, then
    /// this returns `None`.
    fn parse_fast(&self) -> Option<VersionPattern> {
        let (mut prev_digit, mut cur, mut release, mut len) = (false, 0u8, [0u8; 4], 0u8);
        for &byte in self.v {
            if byte == b'.' {
                if !prev_digit {
                    return None;
                }
                prev_digit = false;
                *release.get_mut(usize::from(len))? = cur;
                len += 1;
                cur = 0;
            } else {
                let digit = byte.checked_sub(b'0')?;
                if digit > 9 {
                    return None;
                }
                prev_digit = true;
                cur = cur.checked_mul(10)?.checked_add(digit)?;
            }
        }
        if !prev_digit {
            return None;
        }
        *release.get_mut(usize::from(len))? = cur;
        len += 1;
        let small = VersionSmall {
            // Clippy warns about no-ops like `(0x00 << 16)`, but I
            // think it makes the bit logic much clearer, and makes it
            // explicit that nothing was forgotten.
            #[allow(clippy::identity_op)]
            repr: (u64::from(release[0]) << 48)
                | (u64::from(release[1]) << 40)
                | (u64::from(release[2]) << 32)
                | (u64::from(release[3]) << 24)
                | (0xA0 << 16)
                | (0x00 << 8)
                | (0x00 << 0),
            release: [
                u64::from(release[0]),
                u64::from(release[1]),
                u64::from(release[2]),
                u64::from(release[3]),
            ],
            len,
        };
        let inner = Arc::new(VersionInner::Small { small });
        let version = Version { inner };
        Some(VersionPattern {
            version,
            wildcard: false,
        })
    }

    /// Parses an optional initial epoch number and the first component of the
    /// release part of a version number. In all cases, the first part of a
    /// version must be a single number, and if one isn't found, an error is
    /// returned.
    ///
    /// Upon success, the epoch is possibly set and the release has exactly one
    /// number in it. The parser will be positioned at the beginning of the
    /// next component, which is usually a `.`, indicating the start of the
    /// second number in the release component. It could however point to the
    /// end of input, in which case, a valid version should be returned.
    fn parse_epoch_and_initial_release(&mut self) -> Result<(), VersionPatternParseError> {
        let first_number = self.parse_number()?.ok_or(ErrorKind::NoLeadingNumber)?;
        let first_release_number = if self.bump_if("!") {
            self.epoch = first_number;
            self.parse_number()?
                .ok_or(ErrorKind::NoLeadingReleaseNumber)?
        } else {
            first_number
        };
        self.release.push(first_release_number);
        Ok(())
    }

    /// This parses the rest of the numbers in the release component of
    /// the version. Upon success, the release part of this parser will be
    /// completely finished, and the parser will be positioned at the first
    /// character after the last number in the release component. This position
    /// may point to a `.`, for example, the second dot in `1.2.*` or `1.2.a5`
    /// or `1.2.dev5`. It may also point to the end of the input, in which
    /// case, the caller should return the current version.
    ///
    /// Callers should use this after the initial optional epoch and the first
    /// release number have been parsed.
    fn parse_rest_of_release(&mut self) -> Result<(), VersionPatternParseError> {
        while self.bump_if(".") {
            let Some(n) = self.parse_number()? else {
                self.unbump();
                break;
            };
            self.release.push(n);
        }
        Ok(())
    }

    /// Attempts to parse a trailing wildcard after the numbers in the release
    /// component. Upon success, this returns `true` and positions the parser
    /// immediately after the `.*` (which must necessarily be the end of
    /// input), or leaves it unchanged if no wildcard was found. It is an error
    /// if a `.*` is found and there is still more input after the `.*`.
    ///
    /// Callers should use this immediately after parsing all of the numbers in
    /// the release component of the version.
    fn parse_wildcard(&mut self) -> Result<bool, VersionPatternParseError> {
        if !self.bump_if(".*") {
            return Ok(false);
        }
        if !self.is_done() {
            return Err(PatternErrorKind::WildcardNotTrailing.into());
        }
        self.wildcard = true;
        Ok(true)
    }

    /// Parses the pre-release component of a version.
    ///
    /// If this version has no pre-release component, then this is a no-op.
    /// Otherwise, it sets `self.pre` and positions the parser to the first
    /// byte immediately following the pre-release.
    fn parse_pre(&mut self) -> Result<(), VersionPatternParseError> {
        // SPELLINGS and MAP are in correspondence. SPELLINGS is used to look
        // for what spelling is used in the version string (if any), and
        // the index of the element found is used to lookup which type of
        // pre-release it is.
        //
        // Note also that the order of the strings themselves matters. If 'pre'
        // were before 'preview' for example, then 'preview' would never match
        // since the strings are matched in order.
        const SPELLINGS: StringSet =
            StringSet::new(&["alpha", "beta", "preview", "pre", "rc", "a", "b", "c"]);
        const MAP: &[PrereleaseKind] = &[
            PrereleaseKind::Alpha,
            PrereleaseKind::Beta,
            PrereleaseKind::Rc,
            PrereleaseKind::Rc,
            PrereleaseKind::Rc,
            PrereleaseKind::Alpha,
            PrereleaseKind::Beta,
            PrereleaseKind::Rc,
        ];

        let oldpos = self.i;
        self.bump_if_byte_set(&Parser::SEPARATOR);
        let Some(spelling) = self.bump_if_string_set(&SPELLINGS) else {
            // We might see a separator (or not) and then something
            // that isn't a pre-release. At this stage, we can't tell
            // whether it's invalid or not. So we back-up and let the
            // caller try something else.
            self.reset(oldpos);
            return Ok(());
        };
        let kind = MAP[spelling];
        self.bump_if_byte_set(&Parser::SEPARATOR);
        // Under the normalization rules, a pre-release without an
        // explicit number defaults to `0`.
        let number = self.parse_number()?.unwrap_or(0);
        self.pre = Some(Prerelease { kind, number });
        Ok(())
    }

    /// Parses the post-release component of a version.
    ///
    /// If this version has no post-release component, then this is a no-op.
    /// Otherwise, it sets `self.post` and positions the parser to the first
    /// byte immediately following the post-release.
    fn parse_post(&mut self) -> Result<(), VersionPatternParseError> {
        const SPELLINGS: StringSet = StringSet::new(&["post", "rev", "r"]);

        let oldpos = self.i;
        if self.bump_if("-") {
            if let Some(n) = self.parse_number()? {
                self.post = Some(n);
                return Ok(());
            }
            self.reset(oldpos);
        }
        self.bump_if_byte_set(&Parser::SEPARATOR);
        if self.bump_if_string_set(&SPELLINGS).is_none() {
            // As with pre-releases, if we don't see post|rev|r here, we can't
            // yet determine whether the version as a whole is invalid since
            // post-releases are optional.
            self.reset(oldpos);
            return Ok(());
        }
        self.bump_if_byte_set(&Parser::SEPARATOR);
        // Under the normalization rules, a post-release without an
        // explicit number defaults to `0`.
        self.post = Some(self.parse_number()?.unwrap_or(0));
        Ok(())
    }

    /// Parses the dev-release component of a version.
    ///
    /// If this version has no dev-release component, then this is a no-op.
    /// Otherwise, it sets `self.dev` and positions the parser to the first
    /// byte immediately following the post-release.
    fn parse_dev(&mut self) -> Result<(), VersionPatternParseError> {
        let oldpos = self.i;
        self.bump_if_byte_set(&Parser::SEPARATOR);
        if !self.bump_if("dev") {
            // As with pre-releases, if we don't see dev here, we can't
            // yet determine whether the version as a whole is invalid
            // since dev-releases are optional.
            self.reset(oldpos);
            return Ok(());
        }
        self.bump_if_byte_set(&Parser::SEPARATOR);
        // Under the normalization rules, a post-release without an
        // explicit number defaults to `0`.
        self.dev = Some(self.parse_number()?.unwrap_or(0));
        Ok(())
    }

    /// Parses the local component of a version.
    ///
    /// If this version has no local component, then this is a no-op.
    /// Otherwise, it adds to `self.local` and positions the parser to the
    /// first byte immediately following the local component. (Which ought to
    /// be the end of the version since the local component is the last thing
    /// that can appear in a version.)
    fn parse_local(&mut self) -> Result<(), VersionPatternParseError> {
        if !self.bump_if("+") {
            return Ok(());
        }
        let mut precursor = '+';
        loop {
            let first = self.bump_while(|byte| byte.is_ascii_alphanumeric());
            if first.is_empty() {
                return Err(ErrorKind::LocalEmpty { precursor }.into());
            }
            self.local.push(if let Ok(number) = parse_u64(first) {
                LocalSegment::Number(number)
            } else {
                let string = String::from_utf8(first.to_ascii_lowercase())
                    .expect("ASCII alphanumerics are always valid UTF-8");
                LocalSegment::String(string)
            });
            let Some(byte) = self.bump_if_byte_set(&Parser::SEPARATOR) else {
                break;
            };
            precursor = char::from(byte);
        }
        Ok(())
    }

    /// Consumes input from the current position while the characters are ASCII
    /// digits, and then attempts to parse what was consumed as a decimal
    /// number.
    ///
    /// If nothing was consumed, then `Ok(None)` is returned. Otherwise, if the
    /// digits consumed do not form a valid decimal number that fits into a
    /// `u64`, then an error is returned.
    fn parse_number(&mut self) -> Result<Option<u64>, VersionPatternParseError> {
        let digits = self.bump_while(|ch| ch.is_ascii_digit());
        if digits.is_empty() {
            return Ok(None);
        }
        Ok(Some(parse_u64(digits)?))
    }

    /// Turns whatever state has been gathered into a `VersionPattern`.
    ///
    /// # Panics
    ///
    /// When `self.release` is empty. Callers must ensure at least one part
    /// of the release component has been successfully parsed. Otherwise, the
    /// version itself is invalid.
    fn into_pattern(self) -> VersionPattern {
        assert!(
            self.release.len() > 0,
            "version with no release numbers is invalid"
        );
        let version = Version::new(self.release.as_slice())
            .with_epoch(self.epoch)
            .with_pre(self.pre)
            .with_post(self.post)
            .with_dev(self.dev)
            .with_local(self.local);
        VersionPattern {
            version,
            wildcard: self.wildcard,
        }
    }

    /// Consumes input from this parser while the given predicate returns true.
    /// The resulting input (which may be empty) is returned.
    ///
    /// Once returned, the parser is positioned at the first position where the
    /// predicate returns `false`. (This may be the position at the end of the
    /// input such that [`Parser::is_done`] returns `true`.)
    fn bump_while(&mut self, mut predicate: impl FnMut(u8) -> bool) -> &'a [u8] {
        let start = self.i;
        while !self.is_done() && predicate(self.byte()) {
            self.i = self.i.saturating_add(1);
        }
        &self.v[start..self.i]
    }

    /// Consumes `bytes.len()` bytes from the current position of the parser if
    /// and only if `bytes` is a prefix of the input starting at the current
    /// position. Otherwise, this is a no-op. Returns true when consumption was
    /// successful.
    fn bump_if(&mut self, string: &str) -> bool {
        if self.is_done() {
            return false;
        }
        if starts_with_ignore_ascii_case(string.as_bytes(), &self.v[self.i..]) {
            self.i = self
                .i
                .checked_add(string.len())
                .expect("valid offset because of prefix");
            true
        } else {
            false
        }
    }

    /// Like [`Parser::bump_if`], but attempts each string in the ordered set
    /// given. If one is successfully consumed from the start of the current
    /// position in the input, then it is returned.
    fn bump_if_string_set(&mut self, set: &StringSet) -> Option<usize> {
        let index = set.starts_with(&self.v[self.i..])?;
        let found = &set.strings[index];
        self.i = self
            .i
            .checked_add(found.len())
            .expect("valid offset because of prefix");
        Some(index)
    }

    /// Like [`Parser::bump_if`], but attempts each byte in the set
    /// given. If one is successfully consumed from the start of the
    /// current position in the input.
    fn bump_if_byte_set(&mut self, set: &ByteSet) -> Option<u8> {
        let found = set.starts_with(&self.v[self.i..])?;
        self.i = self
            .i
            .checked_add(1)
            .expect("valid offset because of prefix");
        Some(found)
    }

    /// Moves the parser back one byte. i.e., ungetch.
    ///
    /// This is useful when one has bumped the parser "too far" and wants to
    /// back-up. This tends to help with composition among parser routines.
    ///
    /// # Panics
    ///
    /// When the parser is already positioned at the beginning.
    fn unbump(&mut self) {
        self.i = self.i.checked_sub(1).expect("not at beginning of input");
    }

    /// Resets the parser to the given position.
    ///
    /// # Panics
    ///
    /// When `offset` is greater than `self.v.len()`.
    fn reset(&mut self, offset: usize) {
        assert!(offset <= self.v.len());
        self.i = offset;
    }

    /// Returns the byte at the current position of the parser.
    ///
    /// # Panics
    ///
    /// When `Parser::is_done` returns `true`.
    fn byte(&self) -> u8 {
        self.v[self.i]
    }

    /// Returns true if and only if there is no more input to consume.
    fn is_done(&self) -> bool {
        self.i >= self.v.len()
    }
}

/// Stores the numbers found in the release portion of a version.
///
/// We use this in the version parser to avoid allocating in the 90+% case.
#[derive(Debug)]
enum ReleaseNumbers {
    Inline { numbers: [u64; 4], len: usize },
    Vec(Vec<u64>),
}

impl ReleaseNumbers {
    /// Create a new empty set of release numbers.
    fn new() -> Self {
        Self::Inline {
            numbers: [0; 4],
            len: 0,
        }
    }

    /// Push a new release number. This automatically switches over to the heap
    /// when the lengths grow too big.
    fn push(&mut self, n: u64) {
        match *self {
            Self::Inline {
                ref mut numbers,
                ref mut len,
            } => {
                assert!(*len <= 4);
                if *len == 4 {
                    let mut numbers = numbers.to_vec();
                    numbers.push(n);
                    *self = Self::Vec(numbers.clone());
                } else {
                    numbers[*len] = n;
                    *len += 1;
                }
            }
            Self::Vec(ref mut numbers) => {
                numbers.push(n);
            }
        }
    }

    /// Returns the number of components in this release component.
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Returns the release components as a slice.
    fn as_slice(&self) -> &[u64] {
        match *self {
            Self::Inline { ref numbers, len } => &numbers[..len],
            Self::Vec(ref vec) => vec,
        }
    }
}

/// Represents a set of strings for prefix searching.
///
/// This can be built as a constant and is useful for quickly looking for one
/// of a number of matching literal strings while ignoring ASCII case.
struct StringSet {
    /// A set of the first bytes of each string in this set. We use this to
    /// quickly bail out of searching if the first byte of our haystack doesn't
    /// match any element in this set.
    first_byte: ByteSet,
    /// The strings in this set. They are matched in order.
    strings: &'static [&'static str],
}

impl StringSet {
    /// Create a new string set for prefix searching from the given strings.
    ///
    /// # Panics
    ///
    /// When the number of strings is too big.
    const fn new(strings: &'static [&'static str]) -> Self {
        assert!(
            strings.len() <= 20,
            "only a small number of strings are supported"
        );
        let (mut firsts, mut firsts_len) = ([0u8; 20], 0);
        let mut i = 0;
        while i < strings.len() {
            assert!(
                !strings[i].is_empty(),
                "every string in set should be non-empty",
            );
            firsts[firsts_len] = strings[i].as_bytes()[0];
            firsts_len += 1;
            i += 1;
        }
        let first_byte = ByteSet::new(&firsts);
        Self {
            first_byte,
            strings,
        }
    }

    /// Returns the index of the first string in this set that is a prefix of
    /// the given haystack, or `None` if no elements are a prefix.
    fn starts_with(&self, haystack: &[u8]) -> Option<usize> {
        let first_byte = self.first_byte.starts_with(haystack)?;
        for (i, &string) in self.strings.iter().enumerate() {
            let bytes = string.as_bytes();
            if bytes[0].eq_ignore_ascii_case(&first_byte)
                && starts_with_ignore_ascii_case(bytes, haystack)
            {
                return Some(i);
            }
        }
        None
    }
}

/// A set of bytes for searching case insensitively (ASCII only).
struct ByteSet {
    set: [bool; 256],
}

impl ByteSet {
    /// Create a new byte set for searching from the given bytes.
    const fn new(bytes: &[u8]) -> Self {
        let mut set = [false; 256];
        let mut i = 0;
        while i < bytes.len() {
            set[bytes[i].to_ascii_uppercase() as usize] = true;
            set[bytes[i].to_ascii_lowercase() as usize] = true;
            i += 1;
        }
        Self { set }
    }

    /// Returns the first byte in the haystack if and only if that byte is in
    /// this set (ignoring ASCII case).
    fn starts_with(&self, haystack: &[u8]) -> Option<u8> {
        let byte = *haystack.first()?;
        if self.contains(byte) {
            Some(byte)
        } else {
            None
        }
    }

    /// Returns true if and only if the given byte is in this set.
    fn contains(&self, byte: u8) -> bool {
        self.set[usize::from(byte)]
    }
}

impl std::fmt::Debug for ByteSet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut set = f.debug_set();
        for byte in 0..=255 {
            if self.contains(byte) {
                set.entry(&char::from(byte));
            }
        }
        set.finish()
    }
}

/// An error that occurs when parsing a [`Version`] string fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionParseError {
    kind: Box<ErrorKind>,
}

impl std::error::Error for VersionParseError {}

impl std::fmt::Display for VersionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self.kind {
            ErrorKind::Wildcard => write!(f, "wildcards are not allowed in a version"),
            ErrorKind::InvalidDigit { got } if got.is_ascii() => {
                write!(f, "expected ASCII digit, but found {:?}", char::from(got))
            }
            ErrorKind::InvalidDigit { got } => {
                write!(
                    f,
                    "expected ASCII digit, but found non-ASCII byte \\x{got:02X}"
                )
            }
            ErrorKind::NumberTooBig { ref bytes } => {
                let string = match std::str::from_utf8(bytes) {
                    Ok(v) => v,
                    Err(err) => {
                        std::str::from_utf8(&bytes[..err.valid_up_to()]).expect("valid UTF-8")
                    }
                };
                write!(
                    f,
                    "expected number less than or equal to {}, \
                     but number found in {string:?} exceeds it",
                    u64::MAX,
                )
            }
            ErrorKind::NoLeadingNumber => {
                write!(
                    f,
                    "expected version to start with a number, \
                     but no leading ASCII digits were found"
                )
            }
            ErrorKind::NoLeadingReleaseNumber => {
                write!(
                    f,
                    "expected version to have a non-empty release component after an epoch, \
                     but no ASCII digits after the epoch were found"
                )
            }
            ErrorKind::LocalEmpty { precursor } => {
                write!(
                    f,
                    "found a `{precursor}` indicating the start of a local \
                     component in a version, but did not find any alphanumeric \
                     ASCII segment following the `{precursor}`",
                )
            }
            ErrorKind::UnexpectedEnd {
                ref version,
                ref remaining,
            } => {
                write!(
                    f,
                    "after parsing `{version}`, found `{remaining}`, \
                     which is not part of a valid version",
                )
            }
        }
    }
}

/// The kind of error that occurs when parsing a `Version`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ErrorKind {
    /// Occurs when a version pattern is found but a normal verbatim version is
    /// expected.
    Wildcard,
    /// Occurs when an ASCII digit was expected, but something else was found.
    InvalidDigit {
        /// The (possibly non-ASCII) byte that was seen instead of [0-9].
        got: u8,
    },
    /// Occurs when a number was found that exceeds what can fit into a u64.
    NumberTooBig {
        /// The bytes that were being parsed as a number. These may contain
        /// invalid digits or even invalid UTF-8.
        bytes: Vec<u8>,
    },
    /// Occurs when a version does not start with a leading number.
    NoLeadingNumber,
    /// Occurs when an epoch version does not have a number after the `!`.
    NoLeadingReleaseNumber,
    /// Occurs when a `+` (or a `.` after the first local segment) is seen
    /// (indicating a local component of a version), but no alphanumeric ASCII
    /// string is found following it.
    LocalEmpty {
        /// Either a `+` or a `[-_.]` indicating what was found that demands a
        /// non-empty local segment following it.
        precursor: char,
    },
    /// Occurs when a version has been parsed but there is some unexpected
    /// trailing data in the string.
    UnexpectedEnd {
        /// The version that has been parsed so far.
        version: String,
        /// The bytes that were remaining and not parsed.
        remaining: String,
    },
}

impl From<ErrorKind> for VersionParseError {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }
}

/// An error that occurs when parsing a [`VersionPattern`] string fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionPatternParseError {
    kind: Box<PatternErrorKind>,
}

impl std::error::Error for VersionPatternParseError {}

impl std::fmt::Display for VersionPatternParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self.kind {
            PatternErrorKind::Version(ref err) => err.fmt(f),
            PatternErrorKind::WildcardNotTrailing => {
                write!(f, "wildcards in versions must be at the end")
            }
        }
    }
}

/// The kind of error that occurs when parsing a `VersionPattern`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PatternErrorKind {
    Version(VersionParseError),
    WildcardNotTrailing,
}

impl From<PatternErrorKind> for VersionPatternParseError {
    fn from(kind: PatternErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
        }
    }
}

impl From<ErrorKind> for VersionPatternParseError {
    fn from(kind: ErrorKind) -> Self {
        Self::from(VersionParseError::from(kind))
    }
}

impl From<VersionParseError> for VersionPatternParseError {
    fn from(err: VersionParseError) -> Self {
        Self {
            kind: Box::new(PatternErrorKind::Version(err)),
        }
    }
}

/// Workaround for <https://github.com/PyO3/pyo3/pull/2786>
#[cfg(feature = "pyo3")]
#[derive(Clone, Debug)]
#[pyclass(name = "Version")]
pub struct PyVersion(pub Version);

#[cfg(feature = "pyo3")]
#[pymethods]
impl PyVersion {
    /// The [versioning epoch](https://peps.python.org/pep-0440/#version-epochs). Normally just 0,
    /// but you can increment it if you switched the versioning scheme.
    #[getter]
    pub fn epoch(&self) -> u64 {
        self.0.epoch()
    }
    /// The normal number part of the version
    /// (["final release"](https://peps.python.org/pep-0440/#final-releases)),
    /// such a `1.2.3` in `4!1.2.3-a8.post9.dev1`
    ///
    /// Note that we drop the * placeholder by moving it to `Operator`
    #[getter]
    pub fn release(&self) -> Vec<u64> {
        self.0.release().to_vec()
    }
    /// The [pre-release](https://peps.python.org/pep-0440/#pre-releases), i.e. alpha, beta or rc
    /// plus a number
    ///
    /// Note that whether this is Some influences the version
    /// range matching since normally we exclude all pre-release versions
    #[getter]
    pub fn pre(&self) -> Option<Prerelease> {
        self.0.pre()
    }
    /// The [Post release version](https://peps.python.org/pep-0440/#post-releases),
    /// higher post version are preferred over lower post or none-post versions
    #[getter]
    pub fn post(&self) -> Option<u64> {
        self.0.post()
    }
    /// The [developmental release](https://peps.python.org/pep-0440/#developmental-releases),
    /// if any
    #[getter]
    pub fn dev(&self) -> Option<u64> {
        self.0.dev()
    }
    /// The first item of release or 0 if unavailable.
    #[getter]
    #[allow(clippy::get_first)]
    pub fn major(&self) -> u64 {
        self.0.release().get(0).copied().unwrap_or_default()
    }
    /// The second item of release or 0 if unavailable.
    #[getter]
    pub fn minor(&self) -> u64 {
        self.0.release().get(1).copied().unwrap_or_default()
    }
    /// The third item of release or 0 if unavailable.
    #[getter]
    pub fn micro(&self) -> u64 {
        self.0.release().get(2).copied().unwrap_or_default()
    }

    /// Parses a PEP 440 version string
    #[cfg(feature = "pyo3")]
    #[new]
    pub fn parse(version: &str) -> PyResult<Self> {
        Ok(Self(
            Version::from_str(version).map_err(|e| PyValueError::new_err(e.to_string()))?,
        ))
    }

    // Maps the error type
    /// Parse a PEP 440 version optionally ending with `.*`
    #[cfg(feature = "pyo3")]
    #[staticmethod]
    pub fn parse_star(version_specifier: &str) -> PyResult<(Self, bool)> {
        version_specifier
            .parse::<VersionPattern>()
            .map_err(|e| PyValueError::new_err(e.to_string()))
            .map(|VersionPattern { version, wildcard }| (Self(version), wildcard))
    }

    /// Returns the normalized representation
    #[cfg(feature = "pyo3")]
    pub fn __str__(&self) -> String {
        self.0.to_string()
    }

    /// Returns the normalized representation
    #[cfg(feature = "pyo3")]
    pub fn __repr__(&self) -> String {
        format!(r#"<Version("{}")>"#, self.0)
    }

    /// Returns the normalized representation
    #[cfg(feature = "pyo3")]
    pub fn __hash__(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    #[cfg(feature = "pyo3")]
    fn __richcmp__(&self, other: &Self, op: CompareOp) -> bool {
        op.matches(self.0.cmp(&other.0))
    }

    fn any_prerelease(&self) -> bool {
        self.0.any_prerelease()
    }
}

#[cfg(feature = "pyo3")]
impl IntoPy<PyObject> for Version {
    fn into_py(self, py: Python<'_>) -> PyObject {
        PyVersion(self).into_py(py)
    }
}

#[cfg(feature = "pyo3")]
impl<'source> FromPyObject<'source> for Version {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        Ok(ob.extract::<PyVersion>()?.0)
    }
}

/// Compare the release parts of two versions, e.g. `4.3.1` > `4.2`, `1.1.0` ==
/// `1.1` and `1.16` < `1.19`
pub(crate) fn compare_release(this: &[u64], other: &[u64]) -> Ordering {
    if this.len() == other.len() {
        return this.cmp(other);
    }
    // "When comparing release segments with different numbers of components, the shorter segment
    // is padded out with additional zeros as necessary"
    for (this, other) in this.iter().chain(std::iter::repeat(&0)).zip(
        other
            .iter()
            .chain(std::iter::repeat(&0))
            .take(this.len().max(other.len())),
    ) {
        match this.cmp(other) {
            Ordering::Less => {
                return Ordering::Less;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                return Ordering::Greater;
            }
        }
    }
    Ordering::Equal
}

/// Compare the parts attached after the release, given equal release
///
/// According to [a summary of permitted suffixes and relative
/// ordering][pep440-suffix-ordering] the order of pre/post-releases is: .devN,
/// aN, bN, rcN, <no suffix (final)>, .postN but also, you can have dev/post
/// releases on beta releases, so we make a three stage ordering: ({min: 0,
/// dev: 1, a: 2, b: 3, rc: 4, (): 5, post: 6}, <preN>, <postN or None as
/// smallest>, <devN or Max as largest>, <local>)
///
/// For post, any number is better than none (so None defaults to None<0),
/// but for dev, no number is better (so None default to the maximum). For
/// local the Option<Vec<T>> luckily already has the correct default Ord
/// implementation
///
/// [pep440-suffix-ordering]: https://peps.python.org/pep-0440/#summary-of-permitted-suffixes-and-relative-ordering
fn sortable_tuple(version: &Version) -> (u64, u64, Option<u64>, u64, &[LocalSegment]) {
    // If the version is a "max" version, use a post version larger than any possible post version.
    let post = if version.max().is_some() {
        Some(u64::MAX)
    } else {
        version.post()
    };
    match (version.pre(), post, version.dev(), version.min()) {
        // min release
        (_pre, post, _dev, Some(n)) => (0, 0, post, n, version.local()),
        // dev release
        (None, None, Some(n), None) => (1, 0, None, n, version.local()),
        // alpha release
        (
            Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: n,
            }),
            post,
            dev,
            None,
        ) => (2, n, post, dev.unwrap_or(u64::MAX), version.local()),
        // beta release
        (
            Some(Prerelease {
                kind: PrereleaseKind::Beta,
                number: n,
            }),
            post,
            dev,
            None,
        ) => (3, n, post, dev.unwrap_or(u64::MAX), version.local()),
        // alpha release
        (
            Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: n,
            }),
            post,
            dev,
            None,
        ) => (4, n, post, dev.unwrap_or(u64::MAX), version.local()),
        // final release
        (None, None, None, None) => (5, 0, None, 0, version.local()),
        // post release
        (None, Some(post), dev, None) => {
            (6, 0, Some(post), dev.unwrap_or(u64::MAX), version.local())
        }
    }
}

/// Returns true only when, ignoring ASCII case, `needle` is a prefix of
/// `haystack`.
fn starts_with_ignore_ascii_case(needle: &[u8], haystack: &[u8]) -> bool {
    needle.len() <= haystack.len()
        && std::iter::zip(needle, haystack).all(|(b1, b2)| b1.eq_ignore_ascii_case(b2))
}

/// Parses a u64 number from the given slice of ASCII digit characters.
///
/// If any byte in the given slice is not [0-9], then this returns an error.
/// Similarly, if the number parsed does not fit into a `u64`, then this
/// returns an error.
///
/// # Motivation
///
/// We hand-write this for a couple reasons. Firstly, the standard library's
/// `FromStr` impl for parsing integers requires UTF-8 validation first. We
/// don't need that for version parsing since we stay in the realm of ASCII.
/// Secondly, std's version is a little more flexible because it supports
/// signed integers. So for example, it permits a leading `+` before the actual
/// integer. We don't need that for version parsing.
fn parse_u64(bytes: &[u8]) -> Result<u64, VersionParseError> {
    let mut n: u64 = 0;
    for &byte in bytes {
        let digit = match byte.checked_sub(b'0') {
            None => return Err(ErrorKind::InvalidDigit { got: byte }.into()),
            Some(digit) if digit > 9 => return Err(ErrorKind::InvalidDigit { got: byte }.into()),
            Some(digit) => {
                debug_assert!((0..=9).contains(&digit));
                u64::from(digit)
            }
        };
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add(digit))
            .ok_or_else(|| ErrorKind::NumberTooBig {
                bytes: bytes.to_vec(),
            })?;
    }
    Ok(n)
}

/// The minimum version that can be represented by a [`Version`]: `0a0.dev0`.
pub static MIN_VERSION: LazyLock<Version> =
    LazyLock::new(|| Version::from_str("0a0.dev0").unwrap());

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    #[cfg(feature = "pyo3")]
    use pyo3::pyfunction;

    use crate::VersionSpecifier;

    use super::*;

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L24-L81>
    #[test]
    fn test_packaging_versions() {
        let versions = [
            // Implicit epoch of 0
            ("1.0.dev456", Version::new([1, 0]).with_dev(Some(456))),
            (
                "1.0a1",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 1,
                })),
            ),
            (
                "1.0a2.dev456",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 2,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1.0a12.dev456",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 12,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1.0a12",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 12,
                })),
            ),
            (
                "1.0b1.dev456",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 1,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1.0b2",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                })),
            ),
            (
                "1.0b2.post345.dev456",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_dev(Some(456))
                    .with_post(Some(345)),
            ),
            (
                "1.0b2.post345",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_post(Some(345)),
            ),
            (
                "1.0b2-346",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_post(Some(346)),
            ),
            (
                "1.0c1.dev456",
                Version::new([1, 0])
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Rc,
                        number: 1,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1.0c1",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1,
                })),
            ),
            (
                "1.0rc2",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 2,
                })),
            ),
            (
                "1.0c3",
                Version::new([1, 0]).with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 3,
                })),
            ),
            ("1.0", Version::new([1, 0])),
            (
                "1.0.post456.dev34",
                Version::new([1, 0]).with_post(Some(456)).with_dev(Some(34)),
            ),
            ("1.0.post456", Version::new([1, 0]).with_post(Some(456))),
            ("1.1.dev1", Version::new([1, 1]).with_dev(Some(1))),
            (
                "1.2+123abc",
                Version::new([1, 2]).with_local(vec![LocalSegment::String("123abc".to_string())]),
            ),
            (
                "1.2+123abc456",
                Version::new([1, 2])
                    .with_local(vec![LocalSegment::String("123abc456".to_string())]),
            ),
            (
                "1.2+abc",
                Version::new([1, 2]).with_local(vec![LocalSegment::String("abc".to_string())]),
            ),
            (
                "1.2+abc123",
                Version::new([1, 2]).with_local(vec![LocalSegment::String("abc123".to_string())]),
            ),
            (
                "1.2+abc123def",
                Version::new([1, 2])
                    .with_local(vec![LocalSegment::String("abc123def".to_string())]),
            ),
            (
                "1.2+1234.abc",
                Version::new([1, 2]).with_local(vec![
                    LocalSegment::Number(1234),
                    LocalSegment::String("abc".to_string()),
                ]),
            ),
            (
                "1.2+123456",
                Version::new([1, 2]).with_local(vec![LocalSegment::Number(123_456)]),
            ),
            (
                "1.2.r32+123456",
                Version::new([1, 2])
                    .with_post(Some(32))
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
            (
                "1.2.rev33+123456",
                Version::new([1, 2])
                    .with_post(Some(33))
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
            // Explicit epoch of 1
            (
                "1!1.0.dev456",
                Version::new([1, 0]).with_epoch(1).with_dev(Some(456)),
            ),
            (
                "1!1.0a1",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 1,
                    })),
            ),
            (
                "1!1.0a2.dev456",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 2,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1!1.0a12.dev456",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 12,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1!1.0a12",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Alpha,
                        number: 12,
                    })),
            ),
            (
                "1!1.0b1.dev456",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 1,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1!1.0b2",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    })),
            ),
            (
                "1!1.0b2.post345.dev456",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_post(Some(345))
                    .with_dev(Some(456)),
            ),
            (
                "1!1.0b2.post345",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_post(Some(345)),
            ),
            (
                "1!1.0b2-346",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Beta,
                        number: 2,
                    }))
                    .with_post(Some(346)),
            ),
            (
                "1!1.0c1.dev456",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Rc,
                        number: 1,
                    }))
                    .with_dev(Some(456)),
            ),
            (
                "1!1.0c1",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Rc,
                        number: 1,
                    })),
            ),
            (
                "1!1.0rc2",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Rc,
                        number: 2,
                    })),
            ),
            (
                "1!1.0c3",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_pre(Some(Prerelease {
                        kind: PrereleaseKind::Rc,
                        number: 3,
                    })),
            ),
            ("1!1.0", Version::new([1, 0]).with_epoch(1)),
            (
                "1!1.0.post456.dev34",
                Version::new([1, 0])
                    .with_epoch(1)
                    .with_post(Some(456))
                    .with_dev(Some(34)),
            ),
            (
                "1!1.0.post456",
                Version::new([1, 0]).with_epoch(1).with_post(Some(456)),
            ),
            (
                "1!1.1.dev1",
                Version::new([1, 1]).with_epoch(1).with_dev(Some(1)),
            ),
            (
                "1!1.2+123abc",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::String("123abc".to_string())]),
            ),
            (
                "1!1.2+123abc456",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::String("123abc456".to_string())]),
            ),
            (
                "1!1.2+abc",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::String("abc".to_string())]),
            ),
            (
                "1!1.2+abc123",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::String("abc123".to_string())]),
            ),
            (
                "1!1.2+abc123def",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::String("abc123def".to_string())]),
            ),
            (
                "1!1.2+1234.abc",
                Version::new([1, 2]).with_epoch(1).with_local(vec![
                    LocalSegment::Number(1234),
                    LocalSegment::String("abc".to_string()),
                ]),
            ),
            (
                "1!1.2+123456",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
            (
                "1!1.2.r32+123456",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_post(Some(32))
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
            (
                "1!1.2.rev33+123456",
                Version::new([1, 2])
                    .with_epoch(1)
                    .with_post(Some(33))
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
            (
                "98765!1.2.rev33+123456",
                Version::new([1, 2])
                    .with_epoch(98765)
                    .with_post(Some(33))
                    .with_local(vec![LocalSegment::Number(123_456)]),
            ),
        ];
        for (string, structured) in versions {
            match Version::from_str(string) {
                Err(err) => {
                    unreachable!(
                        "expected {string:?} to parse as {structured:?}, but got {err:?}",
                        structured = structured.as_bloated_debug(),
                    )
                }
                Ok(v) => assert!(
                    v == structured,
                    "for {string:?}, expected {structured:?} but got {v:?}",
                    structured = structured.as_bloated_debug(),
                    v = v.as_bloated_debug(),
                ),
            }
            let spec = format!("=={string}");
            match VersionSpecifier::from_str(&spec) {
                Err(err) => {
                    unreachable!(
                        "expected version in {spec:?} to parse as {structured:?}, but got {err:?}",
                        structured = structured.as_bloated_debug(),
                    )
                }
                Ok(v) => assert!(
                    v.version() == &structured,
                    "for {string:?}, expected {structured:?} but got {v:?}",
                    structured = structured.as_bloated_debug(),
                    v = v.version.as_bloated_debug(),
                ),
            }
        }
    }

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L91-L100>
    #[test]
    fn test_packaging_failures() {
        let versions = [
            // Versions with invalid local versions
            "1.0+a+",
            "1.0++",
            "1.0+_foobar",
            "1.0+foo&asd",
            "1.0+1+1",
            // Nonsensical versions should also be invalid
            "french toast",
            "==french toast",
        ];
        for version in versions {
            assert!(Version::from_str(version).is_err());
            assert!(VersionSpecifier::from_str(&format!("=={version}")).is_err());
        }
    }

    #[test]
    fn test_equality_and_normalization() {
        let versions = [
            // Various development release incarnations
            ("1.0dev", "1.0.dev0"),
            ("1.0.dev", "1.0.dev0"),
            ("1.0dev1", "1.0.dev1"),
            ("1.0dev", "1.0.dev0"),
            ("1.0-dev", "1.0.dev0"),
            ("1.0-dev1", "1.0.dev1"),
            ("1.0DEV", "1.0.dev0"),
            ("1.0.DEV", "1.0.dev0"),
            ("1.0DEV1", "1.0.dev1"),
            ("1.0DEV", "1.0.dev0"),
            ("1.0.DEV1", "1.0.dev1"),
            ("1.0-DEV", "1.0.dev0"),
            ("1.0-DEV1", "1.0.dev1"),
            // Various alpha incarnations
            ("1.0a", "1.0a0"),
            ("1.0.a", "1.0a0"),
            ("1.0.a1", "1.0a1"),
            ("1.0-a", "1.0a0"),
            ("1.0-a1", "1.0a1"),
            ("1.0alpha", "1.0a0"),
            ("1.0.alpha", "1.0a0"),
            ("1.0.alpha1", "1.0a1"),
            ("1.0-alpha", "1.0a0"),
            ("1.0-alpha1", "1.0a1"),
            ("1.0A", "1.0a0"),
            ("1.0.A", "1.0a0"),
            ("1.0.A1", "1.0a1"),
            ("1.0-A", "1.0a0"),
            ("1.0-A1", "1.0a1"),
            ("1.0ALPHA", "1.0a0"),
            ("1.0.ALPHA", "1.0a0"),
            ("1.0.ALPHA1", "1.0a1"),
            ("1.0-ALPHA", "1.0a0"),
            ("1.0-ALPHA1", "1.0a1"),
            // Various beta incarnations
            ("1.0b", "1.0b0"),
            ("1.0.b", "1.0b0"),
            ("1.0.b1", "1.0b1"),
            ("1.0-b", "1.0b0"),
            ("1.0-b1", "1.0b1"),
            ("1.0beta", "1.0b0"),
            ("1.0.beta", "1.0b0"),
            ("1.0.beta1", "1.0b1"),
            ("1.0-beta", "1.0b0"),
            ("1.0-beta1", "1.0b1"),
            ("1.0B", "1.0b0"),
            ("1.0.B", "1.0b0"),
            ("1.0.B1", "1.0b1"),
            ("1.0-B", "1.0b0"),
            ("1.0-B1", "1.0b1"),
            ("1.0BETA", "1.0b0"),
            ("1.0.BETA", "1.0b0"),
            ("1.0.BETA1", "1.0b1"),
            ("1.0-BETA", "1.0b0"),
            ("1.0-BETA1", "1.0b1"),
            // Various release candidate incarnations
            ("1.0c", "1.0rc0"),
            ("1.0.c", "1.0rc0"),
            ("1.0.c1", "1.0rc1"),
            ("1.0-c", "1.0rc0"),
            ("1.0-c1", "1.0rc1"),
            ("1.0rc", "1.0rc0"),
            ("1.0.rc", "1.0rc0"),
            ("1.0.rc1", "1.0rc1"),
            ("1.0-rc", "1.0rc0"),
            ("1.0-rc1", "1.0rc1"),
            ("1.0C", "1.0rc0"),
            ("1.0.C", "1.0rc0"),
            ("1.0.C1", "1.0rc1"),
            ("1.0-C", "1.0rc0"),
            ("1.0-C1", "1.0rc1"),
            ("1.0RC", "1.0rc0"),
            ("1.0.RC", "1.0rc0"),
            ("1.0.RC1", "1.0rc1"),
            ("1.0-RC", "1.0rc0"),
            ("1.0-RC1", "1.0rc1"),
            // Various post release incarnations
            ("1.0post", "1.0.post0"),
            ("1.0.post", "1.0.post0"),
            ("1.0post1", "1.0.post1"),
            ("1.0post", "1.0.post0"),
            ("1.0-post", "1.0.post0"),
            ("1.0-post1", "1.0.post1"),
            ("1.0POST", "1.0.post0"),
            ("1.0.POST", "1.0.post0"),
            ("1.0POST1", "1.0.post1"),
            ("1.0POST", "1.0.post0"),
            ("1.0r", "1.0.post0"),
            ("1.0rev", "1.0.post0"),
            ("1.0.POST1", "1.0.post1"),
            ("1.0.r1", "1.0.post1"),
            ("1.0.rev1", "1.0.post1"),
            ("1.0-POST", "1.0.post0"),
            ("1.0-POST1", "1.0.post1"),
            ("1.0-5", "1.0.post5"),
            ("1.0-r5", "1.0.post5"),
            ("1.0-rev5", "1.0.post5"),
            // Local version case insensitivity
            ("1.0+AbC", "1.0+abc"),
            // Integer Normalization
            ("1.01", "1.1"),
            ("1.0a05", "1.0a5"),
            ("1.0b07", "1.0b7"),
            ("1.0c056", "1.0rc56"),
            ("1.0rc09", "1.0rc9"),
            ("1.0.post000", "1.0.post0"),
            ("1.1.dev09000", "1.1.dev9000"),
            ("00!1.2", "1.2"),
            ("0100!0.0", "100!0.0"),
            // Various other normalizations
            ("v1.0", "1.0"),
            ("   v1.0\t\n", "1.0"),
        ];
        for (version_str, normalized_str) in versions {
            let version = Version::from_str(version_str).unwrap();
            let normalized = Version::from_str(normalized_str).unwrap();
            // Just test version parsing again
            assert_eq!(version, normalized, "{version_str} {normalized_str}");
            // Test version normalization
            assert_eq!(
                version.to_string(),
                normalized.to_string(),
                "{version_str} {normalized_str}"
            );
        }
    }

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L229-L277>
    #[test]
    fn test_equality_and_normalization2() {
        let versions = [
            ("1.0.dev456", "1.0.dev456"),
            ("1.0a1", "1.0a1"),
            ("1.0a2.dev456", "1.0a2.dev456"),
            ("1.0a12.dev456", "1.0a12.dev456"),
            ("1.0a12", "1.0a12"),
            ("1.0b1.dev456", "1.0b1.dev456"),
            ("1.0b2", "1.0b2"),
            ("1.0b2.post345.dev456", "1.0b2.post345.dev456"),
            ("1.0b2.post345", "1.0b2.post345"),
            ("1.0rc1.dev456", "1.0rc1.dev456"),
            ("1.0rc1", "1.0rc1"),
            ("1.0", "1.0"),
            ("1.0.post456.dev34", "1.0.post456.dev34"),
            ("1.0.post456", "1.0.post456"),
            ("1.0.1", "1.0.1"),
            ("0!1.0.2", "1.0.2"),
            ("1.0.3+7", "1.0.3+7"),
            ("0!1.0.4+8.0", "1.0.4+8.0"),
            ("1.0.5+9.5", "1.0.5+9.5"),
            ("1.2+1234.abc", "1.2+1234.abc"),
            ("1.2+123456", "1.2+123456"),
            ("1.2+123abc", "1.2+123abc"),
            ("1.2+123abc456", "1.2+123abc456"),
            ("1.2+abc", "1.2+abc"),
            ("1.2+abc123", "1.2+abc123"),
            ("1.2+abc123def", "1.2+abc123def"),
            ("1.1.dev1", "1.1.dev1"),
            ("7!1.0.dev456", "7!1.0.dev456"),
            ("7!1.0a1", "7!1.0a1"),
            ("7!1.0a2.dev456", "7!1.0a2.dev456"),
            ("7!1.0a12.dev456", "7!1.0a12.dev456"),
            ("7!1.0a12", "7!1.0a12"),
            ("7!1.0b1.dev456", "7!1.0b1.dev456"),
            ("7!1.0b2", "7!1.0b2"),
            ("7!1.0b2.post345.dev456", "7!1.0b2.post345.dev456"),
            ("7!1.0b2.post345", "7!1.0b2.post345"),
            ("7!1.0rc1.dev456", "7!1.0rc1.dev456"),
            ("7!1.0rc1", "7!1.0rc1"),
            ("7!1.0", "7!1.0"),
            ("7!1.0.post456.dev34", "7!1.0.post456.dev34"),
            ("7!1.0.post456", "7!1.0.post456"),
            ("7!1.0.1", "7!1.0.1"),
            ("7!1.0.2", "7!1.0.2"),
            ("7!1.0.3+7", "7!1.0.3+7"),
            ("7!1.0.4+8.0", "7!1.0.4+8.0"),
            ("7!1.0.5+9.5", "7!1.0.5+9.5"),
            ("7!1.1.dev1", "7!1.1.dev1"),
        ];
        for (version_str, normalized_str) in versions {
            let version = Version::from_str(version_str).unwrap();
            let normalized = Version::from_str(normalized_str).unwrap();
            assert_eq!(version, normalized, "{version_str} {normalized_str}");
            // Test version normalization
            assert_eq!(
                version.to_string(),
                normalized_str,
                "{version_str} {normalized_str}"
            );
            // Since we're already at it
            assert_eq!(
                version.to_string(),
                normalized.to_string(),
                "{version_str} {normalized_str}"
            );
        }
    }

    #[test]
    fn test_star_fixed_version() {
        let result = Version::from_str("0.9.1.*");
        assert_eq!(result.unwrap_err(), ErrorKind::Wildcard.into());
    }

    #[test]
    fn test_invalid_word() {
        let result = Version::from_str("blergh");
        assert_eq!(result.unwrap_err(), ErrorKind::NoLeadingNumber.into());
    }

    #[test]
    fn test_from_version_star() {
        let p = |s: &str| -> Result<VersionPattern, _> { s.parse() };
        assert!(!p("1.2.3").unwrap().is_wildcard());
        assert!(p("1.2.3.*").unwrap().is_wildcard());
        assert_eq!(
            p("1.2.*.4.*").unwrap_err(),
            PatternErrorKind::WildcardNotTrailing.into(),
        );
        assert_eq!(
            p("1.0-dev1.*").unwrap_err(),
            ErrorKind::UnexpectedEnd {
                version: "1.0-dev1".to_string(),
                remaining: ".*".to_string()
            }
            .into(),
        );
        assert_eq!(
            p("1.0a1.*").unwrap_err(),
            ErrorKind::UnexpectedEnd {
                version: "1.0a1".to_string(),
                remaining: ".*".to_string()
            }
            .into(),
        );
        assert_eq!(
            p("1.0.post1.*").unwrap_err(),
            ErrorKind::UnexpectedEnd {
                version: "1.0.post1".to_string(),
                remaining: ".*".to_string()
            }
            .into(),
        );
        assert_eq!(
            p("1.0+lolwat.*").unwrap_err(),
            ErrorKind::LocalEmpty { precursor: '.' }.into(),
        );
    }

    // Tests the valid cases of our version parser. These were written
    // in tandem with the parser.
    //
    // They are meant to be additional (but in some cases likely redundant)
    // with some of the above tests.
    #[test]
    fn parse_version_valid() {
        let p = |s: &str| match Parser::new(s.as_bytes()).parse() {
            Ok(v) => v,
            Err(err) => unreachable!("expected valid version, but got error: {err:?}"),
        };

        // release-only tests
        assert_eq!(p("5"), Version::new([5]));
        assert_eq!(p("5.6"), Version::new([5, 6]));
        assert_eq!(p("5.6.7"), Version::new([5, 6, 7]));
        assert_eq!(p("512.623.734"), Version::new([512, 623, 734]));
        assert_eq!(p("1.2.3.4"), Version::new([1, 2, 3, 4]));
        assert_eq!(p("1.2.3.4.5"), Version::new([1, 2, 3, 4, 5]));

        // epoch tests
        assert_eq!(p("4!5"), Version::new([5]).with_epoch(4));
        assert_eq!(p("4!5.6"), Version::new([5, 6]).with_epoch(4));

        // pre-release tests
        assert_eq!(
            p("5a1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1
            }))
        );
        assert_eq!(
            p("5alpha1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1
            }))
        );
        assert_eq!(
            p("5b1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Beta,
                number: 1
            }))
        );
        assert_eq!(
            p("5beta1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Beta,
                number: 1
            }))
        );
        assert_eq!(
            p("5rc1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1
            }))
        );
        assert_eq!(
            p("5c1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1
            }))
        );
        assert_eq!(
            p("5preview1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1
            }))
        );
        assert_eq!(
            p("5pre1"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1
            }))
        );
        assert_eq!(
            p("5.6.7pre1"),
            Version::new([5, 6, 7]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1
            }))
        );
        assert_eq!(
            p("5alpha789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5.alpha789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5-alpha789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5_alpha789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5alpha.789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5alpha-789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5alpha_789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5ALPHA789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5aLpHa789"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 789
            }))
        );
        assert_eq!(
            p("5alpha"),
            Version::new([5]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 0
            }))
        );

        // post-release tests
        assert_eq!(p("5post2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5rev2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5r2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5.post2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5-post2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5_post2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5.post.2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5.post-2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5.post_2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(
            p("5.6.7.post_2"),
            Version::new([5, 6, 7]).with_post(Some(2))
        );
        assert_eq!(p("5-2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5.6.7-2"), Version::new([5, 6, 7]).with_post(Some(2)));
        assert_eq!(p("5POST2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5PoSt2"), Version::new([5]).with_post(Some(2)));
        assert_eq!(p("5post"), Version::new([5]).with_post(Some(0)));

        // dev-release tests
        assert_eq!(p("5dev2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5.dev2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5-dev2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5_dev2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5.dev.2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5.dev-2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5.dev_2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5.6.7.dev_2"), Version::new([5, 6, 7]).with_dev(Some(2)));
        assert_eq!(p("5DEV2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5dEv2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5DeV2"), Version::new([5]).with_dev(Some(2)));
        assert_eq!(p("5dev"), Version::new([5]).with_dev(Some(0)));

        // local tests
        assert_eq!(
            p("5+2"),
            Version::new([5]).with_local(vec![LocalSegment::Number(2)])
        );
        assert_eq!(
            p("5+a"),
            Version::new([5]).with_local(vec![LocalSegment::String("a".to_string())])
        );
        assert_eq!(
            p("5+abc.123"),
            Version::new([5]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::Number(123),
            ])
        );
        assert_eq!(
            p("5+123.abc"),
            Version::new([5]).with_local(vec![
                LocalSegment::Number(123),
                LocalSegment::String("abc".to_string()),
            ])
        );
        assert_eq!(
            p("5+18446744073709551615.abc"),
            Version::new([5]).with_local(vec![
                LocalSegment::Number(18_446_744_073_709_551_615),
                LocalSegment::String("abc".to_string()),
            ])
        );
        assert_eq!(
            p("5+18446744073709551616.abc"),
            Version::new([5]).with_local(vec![
                LocalSegment::String("18446744073709551616".to_string()),
                LocalSegment::String("abc".to_string()),
            ])
        );
        assert_eq!(
            p("5+ABC.123"),
            Version::new([5]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::Number(123),
            ])
        );
        assert_eq!(
            p("5+ABC-123.4_5_xyz-MNO"),
            Version::new([5]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::Number(123),
                LocalSegment::Number(4),
                LocalSegment::Number(5),
                LocalSegment::String("xyz".to_string()),
                LocalSegment::String("mno".to_string()),
            ])
        );
        assert_eq!(
            p("5.6.7+abc-00123"),
            Version::new([5, 6, 7]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::Number(123),
            ])
        );
        assert_eq!(
            p("5.6.7+abc-foo00123"),
            Version::new([5, 6, 7]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::String("foo00123".to_string()),
            ])
        );
        assert_eq!(
            p("5.6.7+abc-00123a"),
            Version::new([5, 6, 7]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::String("00123a".to_string()),
            ])
        );

        // {pre-release, post-release} tests
        assert_eq!(
            p("5a2post3"),
            Version::new([5])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 2
                }))
                .with_post(Some(3))
        );
        assert_eq!(
            p("5.a-2_post-3"),
            Version::new([5])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 2
                }))
                .with_post(Some(3))
        );
        assert_eq!(
            p("5a2-3"),
            Version::new([5])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 2
                }))
                .with_post(Some(3))
        );

        // Ignoring a no-op 'v' prefix.
        assert_eq!(p("v5"), Version::new([5]));
        assert_eq!(p("V5"), Version::new([5]));
        assert_eq!(p("v5.6.7"), Version::new([5, 6, 7]));

        // Ignoring leading and trailing whitespace.
        assert_eq!(p("  v5  "), Version::new([5]));
        assert_eq!(p("  5  "), Version::new([5]));
        assert_eq!(
            p("  5.6.7+abc.123.xyz  "),
            Version::new([5, 6, 7]).with_local(vec![
                LocalSegment::String("abc".to_string()),
                LocalSegment::Number(123),
                LocalSegment::String("xyz".to_string())
            ])
        );
        assert_eq!(p("  \n5\n \t"), Version::new([5]));

        // min tests
        assert!(Parser::new("1.min0".as_bytes()).parse().is_err());
    }

    // Tests the error cases of our version parser.
    //
    // I wrote these with the intent to cover every possible error
    // case.
    //
    // They are meant to be additional (but in some cases likely redundant)
    // with some of the above tests.
    #[test]
    fn parse_version_invalid() {
        let p = |s: &str| match Parser::new(s.as_bytes()).parse() {
            Err(err) => err,
            Ok(v) => unreachable!(
                "expected version parser error, but got: {v:?}",
                v = v.as_bloated_debug()
            ),
        };

        assert_eq!(p(""), ErrorKind::NoLeadingNumber.into());
        assert_eq!(p("a"), ErrorKind::NoLeadingNumber.into());
        assert_eq!(p("v 5"), ErrorKind::NoLeadingNumber.into());
        assert_eq!(p("V 5"), ErrorKind::NoLeadingNumber.into());
        assert_eq!(p("x 5"), ErrorKind::NoLeadingNumber.into());
        assert_eq!(
            p("18446744073709551616"),
            ErrorKind::NumberTooBig {
                bytes: b"18446744073709551616".to_vec()
            }
            .into()
        );
        assert_eq!(p("5!"), ErrorKind::NoLeadingReleaseNumber.into());
        assert_eq!(
            p("5.6./"),
            ErrorKind::UnexpectedEnd {
                version: "5.6".to_string(),
                remaining: "./".to_string()
            }
            .into()
        );
        assert_eq!(
            p("5.6.-alpha2"),
            ErrorKind::UnexpectedEnd {
                version: "5.6".to_string(),
                remaining: ".-alpha2".to_string()
            }
            .into()
        );
        assert_eq!(
            p("1.2.3a18446744073709551616"),
            ErrorKind::NumberTooBig {
                bytes: b"18446744073709551616".to_vec()
            }
            .into()
        );
        assert_eq!(p("5+"), ErrorKind::LocalEmpty { precursor: '+' }.into());
        assert_eq!(p("5+ "), ErrorKind::LocalEmpty { precursor: '+' }.into());
        assert_eq!(p("5+abc."), ErrorKind::LocalEmpty { precursor: '.' }.into());
        assert_eq!(p("5+abc-"), ErrorKind::LocalEmpty { precursor: '-' }.into());
        assert_eq!(p("5+abc_"), ErrorKind::LocalEmpty { precursor: '_' }.into());
        assert_eq!(
            p("5+abc. "),
            ErrorKind::LocalEmpty { precursor: '.' }.into()
        );
        assert_eq!(
            p("5.6-"),
            ErrorKind::UnexpectedEnd {
                version: "5.6".to_string(),
                remaining: "-".to_string()
            }
            .into()
        );
    }

    #[test]
    fn parse_version_pattern_valid() {
        let p = |s: &str| match Parser::new(s.as_bytes()).parse_pattern() {
            Ok(v) => v,
            Err(err) => unreachable!("expected valid version, but got error: {err:?}"),
        };

        assert_eq!(p("5.*"), VersionPattern::wildcard(Version::new([5])));
        assert_eq!(p("5.6.*"), VersionPattern::wildcard(Version::new([5, 6])));
        assert_eq!(
            p("2!5.6.*"),
            VersionPattern::wildcard(Version::new([5, 6]).with_epoch(2))
        );
    }

    #[test]
    fn parse_version_pattern_invalid() {
        let p = |s: &str| match Parser::new(s.as_bytes()).parse_pattern() {
            Err(err) => err,
            Ok(vpat) => unreachable!("expected version pattern parser error, but got: {vpat:?}"),
        };

        assert_eq!(p("*"), ErrorKind::NoLeadingNumber.into());
        assert_eq!(p("2!*"), ErrorKind::NoLeadingReleaseNumber.into());
    }

    // Tests that the ordering between versions is correct.
    //
    // The ordering example used here was taken from PEP 440:
    // https://packaging.python.org/en/latest/specifications/version-specifiers/#summary-of-permitted-suffixes-and-relative-ordering
    #[test]
    fn ordering() {
        let versions = &[
            "1.dev0",
            "1.0.dev456",
            "1.0a1",
            "1.0a2.dev456",
            "1.0a12.dev456",
            "1.0a12",
            "1.0b1.dev456",
            "1.0b2",
            "1.0b2.post345.dev456",
            "1.0b2.post345",
            "1.0rc1.dev456",
            "1.0rc1",
            "1.0",
            "1.0+abc.5",
            "1.0+abc.7",
            "1.0+5",
            "1.0.post456.dev34",
            "1.0.post456",
            "1.0.15",
            "1.1.dev1",
        ];
        for (i, v1) in versions.iter().enumerate() {
            for v2 in &versions[i + 1..] {
                let less = v1.parse::<Version>().unwrap();
                let greater = v2.parse::<Version>().unwrap();
                assert_eq!(
                    less.cmp(&greater),
                    Ordering::Less,
                    "less: {:?}\ngreater: {:?}",
                    less.as_bloated_debug(),
                    greater.as_bloated_debug()
                );
            }
        }
    }

    #[test]
    fn min_version() {
        // Ensure that the `.min` suffix precedes all other suffixes.
        let less = Version::new([1, 0]).with_min(Some(0));

        let versions = &[
            "1.dev0",
            "1.0.dev456",
            "1.0a1",
            "1.0a2.dev456",
            "1.0a12.dev456",
            "1.0a12",
            "1.0b1.dev456",
            "1.0b2",
            "1.0b2.post345.dev456",
            "1.0b2.post345",
            "1.0rc1.dev456",
            "1.0rc1",
            "1.0",
            "1.0+abc.5",
            "1.0+abc.7",
            "1.0+5",
            "1.0.post456.dev34",
            "1.0.post456",
            "1.0.15",
            "1.1.dev1",
        ];

        for greater in versions {
            let greater = greater.parse::<Version>().unwrap();
            assert_eq!(
                less.cmp(&greater),
                Ordering::Less,
                "less: {:?}\ngreater: {:?}",
                less.as_bloated_debug(),
                greater.as_bloated_debug()
            );
        }
    }

    #[test]
    fn max_version() {
        // Ensure that the `.max` suffix succeeds all other suffixes.
        let greater = Version::new([1, 0]).with_max(Some(0));

        let versions = &[
            "1.dev0",
            "1.0.dev456",
            "1.0a1",
            "1.0a2.dev456",
            "1.0a12.dev456",
            "1.0a12",
            "1.0b1.dev456",
            "1.0b2",
            "1.0b2.post345.dev456",
            "1.0b2.post345",
            "1.0rc1.dev456",
            "1.0rc1",
            "1.0",
            "1.0+abc.5",
            "1.0+abc.7",
            "1.0+5",
            "1.0.post456.dev34",
            "1.0.post456",
            "1.0",
        ];

        for less in versions {
            let less = less.parse::<Version>().unwrap();
            assert_eq!(
                less.cmp(&greater),
                Ordering::Less,
                "less: {:?}\ngreater: {:?}",
                less.as_bloated_debug(),
                greater.as_bloated_debug()
            );
        }

        // Ensure that the `.max` suffix plays nicely with pre-release versions.
        let greater = Version::new([1, 0])
            .with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            }))
            .with_max(Some(0));

        let versions = &["1.0a1", "1.0a1+local", "1.0a1.post1"];

        for less in versions {
            let less = less.parse::<Version>().unwrap();
            assert_eq!(
                less.cmp(&greater),
                Ordering::Less,
                "less: {:?}\ngreater: {:?}",
                less.as_bloated_debug(),
                greater.as_bloated_debug()
            );
        }

        // Ensure that the `.max` suffix plays nicely with pre-release versions.
        let less = Version::new([1, 0])
            .with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            }))
            .with_max(Some(0));

        let versions = &["1.0b1", "1.0b1+local", "1.0b1.post1", "1.0"];

        for greater in versions {
            let greater = greater.parse::<Version>().unwrap();
            assert_eq!(
                less.cmp(&greater),
                Ordering::Less,
                "less: {:?}\ngreater: {:?}",
                less.as_bloated_debug(),
                greater.as_bloated_debug()
            );
        }
    }

    // Tests our bespoke u64 decimal integer parser.
    #[test]
    fn parse_number_u64() {
        let p = |s: &str| parse_u64(s.as_bytes());
        assert_eq!(p("0"), Ok(0));
        assert_eq!(p("00"), Ok(0));
        assert_eq!(p("1"), Ok(1));
        assert_eq!(p("01"), Ok(1));
        assert_eq!(p("9"), Ok(9));
        assert_eq!(p("10"), Ok(10));
        assert_eq!(p("18446744073709551615"), Ok(18_446_744_073_709_551_615));
        assert_eq!(p("018446744073709551615"), Ok(18_446_744_073_709_551_615));
        assert_eq!(
            p("000000018446744073709551615"),
            Ok(18_446_744_073_709_551_615)
        );

        assert_eq!(p("10a"), Err(ErrorKind::InvalidDigit { got: b'a' }.into()));
        assert_eq!(p("10["), Err(ErrorKind::InvalidDigit { got: b'[' }.into()));
        assert_eq!(p("10/"), Err(ErrorKind::InvalidDigit { got: b'/' }.into()));
        assert_eq!(
            p("18446744073709551616"),
            Err(ErrorKind::NumberTooBig {
                bytes: b"18446744073709551616".to_vec()
            }
            .into())
        );
        assert_eq!(
            p("18446744073799551615abc"),
            Err(ErrorKind::NumberTooBig {
                bytes: b"18446744073799551615abc".to_vec()
            }
            .into())
        );
        assert_eq!(
            parse_u64(b"18446744073799551615\xFF"),
            Err(ErrorKind::NumberTooBig {
                bytes: b"18446744073799551615\xFF".to_vec()
            }
            .into())
        );
    }

    #[cfg(feature = "pyo3")]
    #[pyfunction]
    fn _convert_in_and_out(version: Version) -> Version {
        version
    }

    /// Wraps a `Version` and provides a more "bloated" debug but standard
    /// representation.
    ///
    /// We don't do this by default because it takes up a ton of space, and
    /// just printing out the display version of the version is quite a bit
    /// simpler.
    ///
    /// Nevertheless, when *testing* version parsing, you really want to
    /// be able to peek at all of its constituent parts. So we use this in
    /// assertion failure messages.
    struct VersionBloatedDebug<'a>(&'a Version);

    impl<'a> std::fmt::Debug for VersionBloatedDebug<'a> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Version")
                .field("epoch", &self.0.epoch())
                .field("release", &self.0.release())
                .field("pre", &self.0.pre())
                .field("post", &self.0.post())
                .field("dev", &self.0.dev())
                .field("local", &self.0.local())
                .field("min", &self.0.min())
                .field("max", &self.0.max())
                .finish()
        }
    }

    impl Version {
        pub(crate) fn as_bloated_debug(&self) -> impl std::fmt::Debug + '_ {
            VersionBloatedDebug(self)
        }
    }
}
