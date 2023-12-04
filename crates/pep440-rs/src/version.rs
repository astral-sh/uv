use once_cell::sync::Lazy;
#[cfg(feature = "pyo3")]
use pyo3::{
    basic::CompareOp, exceptions::PyValueError, pyclass, pymethods, FromPyObject, IntoPy, PyAny,
    PyObject, PyResult, Python,
};
use regex::Captures;
use regex::Regex;
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::{max, Ordering};
#[cfg(feature = "pyo3")]
use std::collections::hash_map::DefaultHasher;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::iter;
use std::str::FromStr;

#[cfg(feature = "tracing")]
use tracing::warn;

/// A regex copied from <https://peps.python.org/pep-0440/#appendix-b-parsing-version-strings-with-regular-expressions>,
/// updated to support stars for version ranges
pub(crate) const VERSION_RE_INNER: &str = r"
(?:
    (?:v?)                                            # <https://peps.python.org/pep-0440/#preceding-v-character>
    (?:(?P<epoch>[0-9]+)!)?                           # epoch
    (?P<release>[0-9*]+(?:\.[0-9]+)*)                 # release segment, this now allows for * versions which are more lenient than necessary so we can put better error messages in the code
    (?P<pre_field>                                    # pre-release
        [-_\.]?
        (?P<pre_name>(a|b|c|rc|alpha|beta|pre|preview))
        [-_\.]?
        (?P<pre>[0-9]+)?
    )?
    (?P<post_field>                                   # post release
        (?:-(?P<post_old>[0-9]+))
        |
        (?:
            [-_\.]?
            (?P<post_l>post|rev|r)
            [-_\.]?
            (?P<post_new>[0-9]+)?
        )
    )?
    (?P<dev_field>                                    # dev release
        [-_\.]?
        (?P<dev_l>dev)
        [-_\.]?
        (?P<dev>[0-9]+)?
    )?
)
(?:\+(?P<local>[a-z0-9]+(?:[-_\.][a-z0-9]+)*))?       # local version
(?P<trailing_dot_star>\.\*)?                          # allow for version matching `.*`
";

/// Matches a python version, such as `1.19.a1`. Based on the PEP 440 regex
static VERSION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r#"(?xi)^(?:\s*){VERSION_RE_INNER}(?:\s*)$"#)).unwrap());

/// One of `~=` `==` `!=` `<=` `>=` `<` `>` `===`
#[derive(Eq, PartialEq, Debug, Hash, Clone)]
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

impl FromStr for Operator {
    type Err = String;

    /// Notably, this does not know about star versions, it just assumes the base operator
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let operator = match s {
            "==" => Self::Equal,
            "===" => {
                #[cfg(feature = "tracing")]
                {
                    warn!("Using arbitrary equality (`===`) is discouraged");
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
            // Should be forbidden by the regex if called from normal parsing
            other => {
                return Err(format!(
                    "No such comparison operator '{other}', must be one of ~= == != <= >= < > ===",
                ));
            }
        };
        Ok(operator)
    }
}

impl Display for Operator {
    /// Note the `EqualStar` is also `==`.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let operator = match self {
            Operator::Equal => "==",
            // Beware, this doesn't print the star
            Operator::EqualStar => "==",
            #[allow(deprecated)]
            Operator::ExactEqual => "===",
            Operator::NotEqual => "!=",
            Operator::NotEqualStar => "!=",
            Operator::TildeEqual => "~=",
            Operator::LessThan => "<",
            Operator::LessThanEqual => "<=",
            Operator::GreaterThan => ">",
            Operator::GreaterThanEqual => ">=",
        };

        write!(f, "{operator}")
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl Operator {
    fn __str__(&self) -> String {
        self.to_string()
    }

    fn __repr__(&self) -> String {
        self.to_string()
    }
}

/// Optional prerelease modifier (alpha, beta or release candidate) appended to version
///
/// <https://peps.python.org/pep-0440/#pre-releases>
#[derive(PartialEq, Eq, Debug, Hash, Clone, Ord, PartialOrd)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub enum PreRelease {
    /// alpha prerelease
    Alpha,
    /// beta prerelease
    Beta,
    /// release candidate prerelease
    Rc,
}

impl FromStr for PreRelease {
    type Err = String;

    fn from_str(prerelease: &str) -> Result<Self, Self::Err> {
        match prerelease.to_lowercase().as_str() {
            "a" | "alpha" => Ok(Self::Alpha),
            "b" | "beta" => Ok(Self::Beta),
            "c" | "rc" | "pre" | "preview" => Ok(Self::Rc),
            _ => Err(format!(
                "'{prerelease}' isn't recognized as alpha, beta or release candidate",
            )),
        }
    }
}

impl Display for PreRelease {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alpha => write!(f, "a"),
            Self::Beta => write!(f, "b"),
            Self::Rc => write!(f, "rc"),
        }
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
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum LocalSegment {
    /// Not-parseable as integer segment of local version
    String(String),
    /// Inferred integer segment of local version
    Number(u64),
}

impl Display for LocalSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

impl FromStr for LocalSegment {
    /// This can be a never type when stabilized
    type Err = ();

    fn from_str(segment: &str) -> Result<Self, Self::Err> {
        Ok(if let Ok(number) = segment.parse::<u64>() {
            Self::Number(number)
        } else {
            // "and if a segment contains any ASCII letters then that segment is compared lexicographically with case insensitivity"
            Self::String(segment.to_lowercase())
        })
    }
}

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
#[derive(Debug, Clone)]
pub struct Version {
    /// The [versioning epoch](https://peps.python.org/pep-0440/#version-epochs). Normally just 0,
    /// but you can increment it if you switched the versioning scheme.
    pub epoch: u64,
    /// The normal number part of the version
    /// (["final release"](https://peps.python.org/pep-0440/#final-releases)),
    /// such a `1.2.3` in `4!1.2.3-a8.post9.dev1`
    ///
    /// Note that we drop the * placeholder by moving it to `Operator`
    pub release: Vec<u64>,
    /// The [prerelease](https://peps.python.org/pep-0440/#pre-releases), i.e. alpha, beta or rc
    /// plus a number
    ///
    /// Note that whether this is Some influences the version
    /// range matching since normally we exclude all prerelease versions
    pub pre: Option<(PreRelease, u64)>,
    /// The [Post release version](https://peps.python.org/pep-0440/#post-releases),
    /// higher post version are preferred over lower post or none-post versions
    pub post: Option<u64>,
    /// The [developmental release](https://peps.python.org/pep-0440/#developmental-releases),
    /// if any
    pub dev: Option<u64>,
    /// A [local version identifier](https://peps.python.org/pep-0440/#local-version-identifiers)
    /// such as `+deadbeef` in `1.2.3+deadbeef`
    ///
    /// > They consist of a normal public version identifier (as defined in the previous section),
    /// > along with an arbitrary “local version label”, separated from the public version
    /// > identifier by a plus. Local version labels have no specific semantics assigned, but some
    /// > syntactic restrictions are imposed.
    pub local: Option<Vec<LocalSegment>>,
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
        self.0.epoch
    }
    /// The normal number part of the version
    /// (["final release"](https://peps.python.org/pep-0440/#final-releases)),
    /// such a `1.2.3` in `4!1.2.3-a8.post9.dev1`
    ///
    /// Note that we drop the * placeholder by moving it to `Operator`
    #[getter]
    pub fn release(&self) -> Vec<u64> {
        self.0.release.clone()
    }
    /// The [prerelease](https://peps.python.org/pep-0440/#pre-releases), i.e. alpha, beta or rc
    /// plus a number
    ///
    /// Note that whether this is Some influences the version
    /// range matching since normally we exclude all prerelease versions
    #[getter]
    pub fn pre(&self) -> Option<(PreRelease, u64)> {
        self.0.pre.clone()
    }
    /// The [Post release version](https://peps.python.org/pep-0440/#post-releases),
    /// higher post version are preferred over lower post or none-post versions
    #[getter]
    pub fn post(&self) -> Option<u64> {
        self.0.post
    }
    /// The [developmental release](https://peps.python.org/pep-0440/#developmental-releases),
    /// if any
    #[getter]
    pub fn dev(&self) -> Option<u64> {
        self.0.dev
    }
    /// The first item of release or 0 if unavailable.
    #[getter]
    #[allow(clippy::get_first)]
    pub fn major(&self) -> u64 {
        self.0.release.get(0).copied().unwrap_or_default()
    }
    /// The second item of release or 0 if unavailable.
    #[getter]
    pub fn minor(&self) -> u64 {
        self.0.release.get(1).copied().unwrap_or_default()
    }
    /// The third item of release or 0 if unavailable.
    #[getter]
    pub fn micro(&self) -> u64 {
        self.0.release.get(2).copied().unwrap_or_default()
    }

    /// Parses a PEP 440 version string
    #[cfg(feature = "pyo3")]
    #[new]
    pub fn parse(version: &str) -> PyResult<Self> {
        Ok(Self(
            Version::from_str(version).map_err(PyValueError::new_err)?,
        ))
    }

    // Maps the error type
    /// Parse a PEP 440 version optionally ending with `.*`
    #[cfg(feature = "pyo3")]
    #[staticmethod]
    pub fn parse_star(version_specifier: &str) -> PyResult<(Self, bool)> {
        Version::from_str_star(version_specifier)
            .map_err(PyValueError::new_err)
            .map(|(version, star)| (Self(version), star))
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
        let mut hasher = DefaultHasher::new();
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

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
#[cfg(feature = "serde")]
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
#[cfg(feature = "serde")]
impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Version {
    /// Constructor for a version that is just a release such as `3.8`
    pub fn from_release(release: Vec<u64>) -> Self {
        Self {
            epoch: 0,
            release,
            pre: None,
            post: None,
            dev: None,
            local: None,
        }
    }

    /// For PEP 440 specifier matching: "Except where specifically noted below, local version
    /// identifiers MUST NOT be permitted in version specifiers, and local version labels MUST be
    /// ignored entirely when checking if candidate versions match a given version specifier."
    pub(crate) fn without_local(&self) -> Self {
        Self {
            local: None,
            ..self.clone()
        }
    }
}

impl Version {
    /// Whether this is an alpha/beta/rc or dev version
    pub fn any_prerelease(&self) -> bool {
        self.is_pre() || self.is_dev()
    }

    /// Whether this is an alpha/beta/rc version
    pub fn is_pre(&self) -> bool {
        self.pre.is_some()
    }

    /// Whether this is a dev version
    pub fn is_dev(&self) -> bool {
        self.dev.is_some()
    }

    /// Whether this is a post version
    pub fn is_post(&self) -> bool {
        self.post.is_some()
    }

    /// Whether this is a local version (e.g. `1.2.3+localsuffixesareweird`)
    pub fn is_local(&self) -> bool {
        self.local.is_some()
    }
}

/// Shows normalized version
impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let epoch = if self.epoch == 0 {
            String::new()
        } else {
            format!("{}!", self.epoch)
        };
        let release = self
            .release
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(".");
        let pre = self
            .pre
            .as_ref()
            .map(|(pre_kind, pre_version)| format!("{pre_kind}{pre_version}"))
            .unwrap_or_default();
        let post = self
            .post
            .map(|post| format!(".post{post}"))
            .unwrap_or_default();
        let dev = self.dev.map(|dev| format!(".dev{dev}")).unwrap_or_default();
        let local = self
            .local
            .as_ref()
            .map(|segments| {
                format!(
                    "+{}",
                    segments
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(".")
                )
            })
            .unwrap_or_default();
        write!(f, "{epoch}{release}{pre}{post}{dev}{local}")
    }
}

/// Compare the release parts of two versions, e.g. `4.3.1` > `4.2`, `1.1.0` == `1.1` and
/// `1.16` < `1.19`
pub(crate) fn compare_release(this: &[u64], other: &[u64]) -> Ordering {
    // "When comparing release segments with different numbers of components, the shorter segment
    // is padded out with additional zeros as necessary"
    for (this, other) in this.iter().chain(iter::repeat(&0)).zip(
        other
            .iter()
            .chain(iter::repeat(&0))
            .take(max(this.len(), other.len())),
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
/// According to <https://peps.python.org/pep-0440/#summary-of-permitted-suffixes-and-relative-ordering>
/// the order of pre/post-releases is:
/// .devN, aN, bN, rcN, <no suffix (final)>, .postN
/// but also, you can have dev/post releases on beta releases, so we make a three stage ordering:
/// ({dev: 0, a: 1, b: 2, rc: 3, (): 4, post: 5}, <preN>, <postN or None as smallest>, <devN or Max as largest>, <local>)
///
/// For post, any number is better than none (so None defaults to None<0), but for dev, no number
/// is better (so None default to the maximum). For local the Option<Vec<T>> luckily already has the
/// correct default Ord implementation
fn sortable_tuple(version: &Version) -> (u64, u64, Option<u64>, u64, Option<&[LocalSegment]>) {
    match (&version.pre, &version.post, &version.dev) {
        // dev release
        (None, None, Some(n)) => (0, 0, None, *n, version.local.as_deref()),
        // alpha release
        (Some((PreRelease::Alpha, n)), post, dev) => (
            1,
            *n,
            *post,
            dev.unwrap_or(u64::MAX),
            version.local.as_deref(),
        ),
        // beta release
        (Some((PreRelease::Beta, n)), post, dev) => (
            2,
            *n,
            *post,
            dev.unwrap_or(u64::MAX),
            version.local.as_deref(),
        ),
        // alpha release
        (Some((PreRelease::Rc, n)), post, dev) => (
            3,
            *n,
            *post,
            dev.unwrap_or(u64::MAX),
            version.local.as_deref(),
        ),
        // final release
        (None, None, None) => (4, 0, None, 0, version.local.as_deref()),
        // post release
        (None, Some(post), dev) => (
            5,
            0,
            Some(*post),
            dev.unwrap_or(u64::MAX),
            version.local.as_deref(),
        ),
    }
}

impl PartialEq<Self> for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Hash for Version {
    /// Custom implementation to ignoring trailing zero because `PartialEq` zero pads
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        // Skip trailing zeros
        for i in self.release.iter().rev().skip_while(|x| **x == 0) {
            i.hash(state);
        }
        self.pre.hash(state);
        self.dev.hash(state);
        self.post.hash(state);
        self.local.hash(state);
    }
}

impl PartialOrd<Self> for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// 1.0.dev456 < 1.0a1 < 1.0a2.dev456 < 1.0a12.dev456 < 1.0a12 < 1.0b1.dev456 < 1.0b2
    /// < 1.0b2.post345.dev456 < 1.0b2.post345 < 1.0b2-346 < 1.0c1.dev456 < 1.0c1 < 1.0rc2 < 1.0c3
    /// < 1.0 < 1.0.post456.dev34 < 1.0.post456
    fn cmp(&self, other: &Self) -> Ordering {
        match self.epoch.cmp(&other.epoch) {
            Ordering::Less => {
                return Ordering::Less;
            }
            Ordering::Equal => {}
            Ordering::Greater => {
                return Ordering::Greater;
            }
        }

        match compare_release(&self.release, &other.release) {
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

impl FromStr for Version {
    type Err = String;

    /// Parses a version such as `1.19`, `1.0a1`,`1.0+abc.5` or `1!2012.2`
    ///
    /// Note that this variant doesn't allow the version to end with a star, see
    /// [`Self::from_str_star`] if you want to parse versions for specifiers
    fn from_str(version: &str) -> Result<Self, Self::Err> {
        let captures = VERSION_RE
            .captures(version)
            .ok_or_else(|| format!("Version `{version}` doesn't match PEP 440 rules"))?;
        let (version, star) = Version::parse_impl(&captures)?;
        if star {
            return Err("A star (`*`) must not be used in a fixed version (use `Version::from_string_star` otherwise)".to_string());
        }
        Ok(version)
    }
}

impl Version {
    /// Like [`Self::from_str`], but also allows the version to end with a star and returns whether it
    /// did. This variant is for use in specifiers.
    ///  * `1.2.3` -> false
    ///  * `1.2.3.*` -> true
    ///  * `1.2.*.4` -> err
    ///  * `1.0-dev1.*` -> err
    pub fn from_str_star(version: &str) -> Result<(Self, bool), String> {
        let captures = VERSION_RE
            .captures(version)
            .ok_or_else(|| format!("Version `{version}` doesn't match PEP 440 rules"))?;
        let (version, star) = Version::parse_impl(&captures)?;
        Ok((version, star))
    }

    /// Extracted for reusability around star/non-star
    pub(crate) fn parse_impl(captures: &Captures) -> Result<(Version, bool), String> {
        let number_field = |field_name| {
            if let Some(field_str) = captures.name(field_name) {
                match field_str.as_str().parse::<u64>() {
                    Ok(number) => Ok(Some(number)),
                    // Should be already forbidden by the regex
                    Err(err) => Err(format!(
                        "Couldn't parse '{}' as number from {}: {}",
                        field_str.as_str(),
                        field_name,
                        err
                    )),
                }
            } else {
                Ok(None)
            }
        };
        let epoch = number_field("epoch")?
            // "If no explicit epoch is given, the implicit epoch is 0"
            .unwrap_or_default();
        let pre = {
            let pre_type = captures
                .name("pre_name")
                .map(|pre| PreRelease::from_str(pre.as_str()))
                // Shouldn't fail due to the regex
                .transpose()?;
            let pre_number = number_field("pre")?
                // <https://peps.python.org/pep-0440/#implicit-pre-release-number>
                .unwrap_or_default();
            pre_type.map(|pre_type| (pre_type, pre_number))
        };
        let post = if captures.name("post_field").is_some() {
            // While PEP 440 says .post is "followed by a non-negative integer value",
            // packaging has tests that ensure that it defaults to 0
            // https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L187-L202
            Some(
                number_field("post_new")?
                    .or(number_field("post_old")?)
                    .unwrap_or_default(),
            )
        } else {
            None
        };
        let dev = if captures.name("dev_field").is_some() {
            // <https://peps.python.org/pep-0440/#implicit-development-release-number>
            Some(number_field("dev")?.unwrap_or_default())
        } else {
            None
        };
        let local = captures.name("local").map(|local| {
            local
                .as_str()
                .split(&['-', '_', '.'][..])
                .map(|segment| {
                    if let Ok(number) = segment.parse::<u64>() {
                        LocalSegment::Number(number)
                    } else {
                        // "and if a segment contains any ASCII letters then that segment is compared lexicographically with case insensitivity"
                        LocalSegment::String(segment.to_lowercase())
                    }
                })
                .collect()
        });
        let release = captures
            .name("release")
            // Should be forbidden by the regex
            .ok_or_else(|| "No release in version".to_string())?
            .as_str()
            .split('.')
            .map(|segment| segment.parse::<u64>().map_err(|err| err.to_string()))
            .collect::<Result<Vec<u64>, String>>()?;

        let star = captures.name("trailing_dot_star").is_some();
        if star {
            if pre.is_some() {
                return Err(
                    "You can't have both a trailing `.*` and a prerelease version".to_string(),
                );
            }
            if post.is_some() {
                return Err("You can't have both a trailing `.*` and a post version".to_string());
            }
            if dev.is_some() {
                return Err("You can't have both a trailing `.*` and a dev version".to_string());
            }
            if local.is_some() {
                return Err("You can't have both a trailing `.*` and a local version".to_string());
            }
        }

        let version = Version {
            epoch,
            release,
            pre,
            post,
            dev,
            local,
        };
        Ok((version, star))
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "pyo3")]
    use pyo3::pyfunction;
    use std::str::FromStr;

    use crate::{Version, VersionSpecifier};

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L24-L81>
    #[test]
    fn test_packaging_versions() {
        let versions = [
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
        for version in versions {
            Version::from_str(version).unwrap();
            VersionSpecifier::from_str(&format!("=={version}")).unwrap();
        }
    }

    /// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L91-L100>
    #[test]
    fn test_packaging_failures() {
        let versions = [
            // Nonsensical versions should be invalid
            "french toast",
            // Versions with invalid local versions
            "1.0+a+",
            "1.0++",
            "1.0+_foobar",
            "1.0+foo&asd",
            "1.0+1+1",
        ];
        for version in versions {
            assert_eq!(
                Version::from_str(version).unwrap_err(),
                format!("Version `{version}` doesn't match PEP 440 rules")
            );
            assert_eq!(
                VersionSpecifier::from_str(&format!("=={version}")).unwrap_err(),
                format!("Version specifier `=={version}` doesn't match PEP 440 rules")
            );
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
        assert_eq!(
            result.unwrap_err(),
            "A star (`*`) must not be used in a fixed version (use `Version::from_string_star` otherwise)"
        );
    }

    #[test]
    fn test_regex_mismatch() {
        let result = Version::from_str("blergh");
        assert_eq!(
            result.unwrap_err(),
            "Version `blergh` doesn't match PEP 440 rules"
        );
    }

    #[test]
    fn test_from_version_star() {
        assert!(!Version::from_str_star("1.2.3").unwrap().1);
        assert!(Version::from_str_star("1.2.3.*").unwrap().1);
        assert_eq!(
            Version::from_str_star("1.2.*.4.*").unwrap_err(),
            "Version `1.2.*.4.*` doesn't match PEP 440 rules"
        );
        assert_eq!(
            Version::from_str_star("1.0-dev1.*").unwrap_err(),
            "You can't have both a trailing `.*` and a dev version"
        );
        assert_eq!(
            Version::from_str_star("1.0a1.*").unwrap_err(),
            "You can't have both a trailing `.*` and a prerelease version"
        );
        assert_eq!(
            Version::from_str_star("1.0.post1.*").unwrap_err(),
            "You can't have both a trailing `.*` and a post version"
        );
        assert_eq!(
            Version::from_str_star("1.0+lolwat.*").unwrap_err(),
            "You can't have both a trailing `.*` and a local version"
        );
    }

    #[cfg(feature = "pyo3")]
    #[pyfunction]
    fn _convert_in_and_out(version: Version) -> Version {
        version
    }
}
