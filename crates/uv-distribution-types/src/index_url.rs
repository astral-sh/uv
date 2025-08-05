use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, RwLock};

use itertools::Either;
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;
use url::{ParseError, Url};

use uv_pep508::{Scheme, VerbatimUrl, VerbatimUrlError, split_scheme};
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user;

use crate::{Index, IndexStatusCodeStrategy, Verbatim};

static PYPI_URL: LazyLock<DisplaySafeUrl> =
    LazyLock::new(|| DisplaySafeUrl::parse("https://pypi.org/simple").unwrap());

static DEFAULT_INDEX: LazyLock<Index> = LazyLock::new(|| {
    Index::from_index_url(IndexUrl::Pypi(Arc::new(VerbatimUrl::from_url(
        PYPI_URL.clone(),
    ))))
});

/// The URL of an index to use for fetching packages (e.g., PyPI).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum IndexUrl {
    Pypi(Arc<VerbatimUrl>),
    Url(Arc<VerbatimUrl>),
    Path(Arc<VerbatimUrl>),
}

impl IndexUrl {
    /// Parse an [`IndexUrl`] from a string, relative to an optional root directory.
    ///
    /// If no root directory is provided, relative paths are resolved against the current working
    /// directory.
    pub fn parse(path: &str, root_dir: Option<&Path>) -> Result<Self, IndexUrlError> {
        let url = VerbatimUrl::from_url_or_path(path, root_dir)?;
        Ok(Self::from(url))
    }

    /// Return the root [`Url`] of the index, if applicable.
    ///
    /// For indexes with a `/simple` endpoint, this is simply the URL with the final segment
    /// removed. This is useful, e.g., for credential propagation to other endpoints on the index.
    pub fn root(&self) -> Option<DisplaySafeUrl> {
        let mut segments = self.url().path_segments()?;
        let last = match segments.next_back()? {
            // If the last segment is empty due to a trailing `/`, skip it (as in `pop_if_empty`)
            "" => segments.next_back()?,
            segment => segment,
        };

        // We also handle `/+simple` as it's used in devpi
        if !(last.eq_ignore_ascii_case("simple") || last.eq_ignore_ascii_case("+simple")) {
            return None;
        }

        let mut url = self.url().clone();
        url.path_segments_mut().ok()?.pop_if_empty().pop();
        Some(url)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for IndexUrl {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("IndexUrl")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "The URL of an index to use for fetching packages (e.g., `https://pypi.org/simple`), or a local path."
        })
    }
}

impl IndexUrl {
    /// Return the raw URL for the index.
    pub fn url(&self) -> &DisplaySafeUrl {
        match self {
            Self::Pypi(url) => url.raw(),
            Self::Url(url) => url.raw(),
            Self::Path(url) => url.raw(),
        }
    }

    /// Convert the index URL into a [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        match self {
            Self::Pypi(url) => url.to_url(),
            Self::Url(url) => url.to_url(),
            Self::Path(url) => url.to_url(),
        }
    }

    /// Return the redacted URL for the index, omitting any sensitive credentials.
    pub fn without_credentials(&self) -> Cow<'_, DisplaySafeUrl> {
        let url = self.url();
        if url.username().is_empty() && url.password().is_none() {
            Cow::Borrowed(url)
        } else {
            let mut url = url.clone();
            let _ = url.set_username("");
            let _ = url.set_password(None);
            Cow::Owned(url)
        }
    }

    /// Warn user if the given URL was provided as an ambiguous relative path.
    ///
    /// This is a temporary warning. Ambiguous values will not be
    /// accepted in the future.
    pub fn warn_on_disambiguated_relative_path(&self) {
        let Self::Path(verbatim_url) = &self else {
            return;
        };

        if let Some(path) = verbatim_url.given() {
            if !is_disambiguated_path(path) {
                if cfg!(windows) {
                    warn_user!(
                        "Relative paths passed to `--index` or `--default-index` should be disambiguated from index names (use `.\\{path}` or `./{path}`). Support for ambiguous values will be removed in the future"
                    );
                } else {
                    warn_user!(
                        "Relative paths passed to `--index` or `--default-index` should be disambiguated from index names (use `./{path}`). Support for ambiguous values will be removed in the future"
                    );
                }
            }
        }
    }
}

impl Display for IndexUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pypi(url) => Display::fmt(url, f),
            Self::Url(url) => Display::fmt(url, f),
            Self::Path(url) => Display::fmt(url, f),
        }
    }
}

impl Verbatim for IndexUrl {
    fn verbatim(&self) -> Cow<'_, str> {
        match self {
            Self::Pypi(url) => url.verbatim(),
            Self::Url(url) => url.verbatim(),
            Self::Path(url) => url.verbatim(),
        }
    }
}

/// Checks if a path is disambiguated.
///
/// Disambiguated paths are absolute paths, paths with valid schemes,
/// and paths starting with "./" or "../" on Unix or ".\\", "..\\",
/// "./", or "../" on Windows.
fn is_disambiguated_path(path: &str) -> bool {
    if cfg!(windows) {
        if path.starts_with(".\\") || path.starts_with("..\\") || path.starts_with('/') {
            return true;
        }
    }
    if path.starts_with("./") || path.starts_with("../") || Path::new(path).is_absolute() {
        return true;
    }
    // Check if the path has a scheme (like `file://`)
    if let Some((scheme, _)) = split_scheme(path) {
        return Scheme::parse(scheme).is_some();
    }
    // This is an ambiguous relative path
    false
}

/// An error that can occur when parsing an [`IndexUrl`].
#[derive(Error, Debug)]
pub enum IndexUrlError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Url(#[from] ParseError),
    #[error(transparent)]
    VerbatimUrl(#[from] VerbatimUrlError),
}

impl FromStr for IndexUrl {
    type Err = IndexUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s, None)
    }
}

impl serde::ser::Serialize for IndexUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            Self::Pypi(url) => url.without_credentials().serialize(serializer),
            Self::Url(url) => url.without_credentials().serialize(serializer),
            Self::Path(url) => url.without_credentials().serialize(serializer),
        }
    }
}

impl<'de> serde::de::Deserialize<'de> for IndexUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = IndexUrl;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                IndexUrl::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl From<VerbatimUrl> for IndexUrl {
    fn from(url: VerbatimUrl) -> Self {
        if url.scheme() == "file" {
            Self::Path(Arc::new(url))
        } else if *url.raw() == *PYPI_URL {
            Self::Pypi(Arc::new(url))
        } else {
            Self::Url(Arc::new(url))
        }
    }
}

impl From<IndexUrl> for DisplaySafeUrl {
    fn from(index: IndexUrl) -> Self {
        match index {
            IndexUrl::Pypi(url) => url.to_url(),
            IndexUrl::Url(url) => url.to_url(),
            IndexUrl::Path(url) => url.to_url(),
        }
    }
}

impl Deref for IndexUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Pypi(url) => url,
            Self::Url(url) => url,
            Self::Path(url) => url,
        }
    }
}

/// The index locations to use for fetching packages. By default, uses the PyPI index.
///
/// This type merges the legacy `--index-url`, `--extra-index-url`, and `--find-links` options,
/// along with the uv-specific `--index` and `--default-index`.
#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IndexLocations {
    indexes: Vec<Index>,
    flat_index: Vec<Index>,
    no_index: bool,
}

impl IndexLocations {
    /// Determine the index URLs to use for fetching packages.
    pub fn new(indexes: Vec<Index>, flat_index: Vec<Index>, no_index: bool) -> Self {
        Self {
            indexes,
            flat_index,
            no_index,
        }
    }

    /// Combine a set of index locations.
    ///
    /// If either the current or the other index locations have `no_index` set, the result will
    /// have `no_index` set.
    ///
    /// If the current index location has an `index` set, it will be preserved.
    #[must_use]
    pub fn combine(self, indexes: Vec<Index>, flat_index: Vec<Index>, no_index: bool) -> Self {
        Self {
            indexes: self.indexes.into_iter().chain(indexes).collect(),
            flat_index: self.flat_index.into_iter().chain(flat_index).collect(),
            no_index: self.no_index || no_index,
        }
    }

    /// Returns `true` if no index configuration is set, i.e., the [`IndexLocations`] matches the
    /// default configuration.
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

impl<'a> IndexLocations {
    /// Return the default [`Index`] entry.
    ///
    /// If `--no-index` is set, return `None`.
    ///
    /// If no index is provided, use the `PyPI` index.
    pub fn default_index(&'a self) -> Option<&'a Index> {
        if self.no_index {
            None
        } else {
            let mut seen = FxHashSet::default();
            self.indexes
                .iter()
                .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
                .find(|index| index.default)
                .or_else(|| Some(&DEFAULT_INDEX))
        }
    }

    /// Return an iterator over the implicit [`Index`] entries.
    ///
    /// Default and explicit indexes are excluded.
    pub fn implicit_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            let mut seen = FxHashSet::default();
            Either::Right(
                self.indexes
                    .iter()
                    .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
                    .filter(|index| !index.default && !index.explicit),
            )
        }
    }

    /// Return an iterator over all [`Index`] entries in order.
    ///
    /// Explicit indexes are excluded.
    ///
    /// Prioritizes the extra indexes over the default index.
    ///
    /// If `no_index` was enabled, then this always returns an empty
    /// iterator.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        self.implicit_indexes()
            .chain(self.default_index())
            .filter(|index| !index.explicit)
    }

    /// Return an iterator over all simple [`Index`] entries in order.
    ///
    /// If `no_index` was enabled, then this always returns an empty iterator.
    pub fn simple_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            let mut seen = FxHashSet::default();
            Either::Right(
                self.indexes
                    .iter()
                    .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name))),
            )
        }
    }

    /// Return an iterator over the [`FlatIndexLocation`] entries.
    pub fn flat_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        self.flat_index.iter()
    }

    /// Return the `--no-index` flag.
    pub fn no_index(&self) -> bool {
        self.no_index
    }

    /// Clone the index locations into a [`IndexUrls`] instance.
    pub fn index_urls(&'a self) -> IndexUrls {
        IndexUrls {
            indexes: self.indexes.clone(),
            no_index: self.no_index,
        }
    }

    /// Return a vector containing all allowed [`Index`] entries.
    ///
    /// This includes explicit indexes, implicit indexes, flat indexes, and the default index.
    ///
    /// The indexes will be returned in the reverse of the order in which they were defined, such
    /// that the last-defined index is the first item in the vector.
    pub fn allowed_indexes(&'a self) -> Vec<&'a Index> {
        if self.no_index {
            self.flat_index.iter().rev().collect()
        } else {
            let mut indexes = vec![];

            let mut seen = FxHashSet::default();
            let mut default = false;
            for index in {
                self.indexes
                    .iter()
                    .chain(self.flat_index.iter())
                    .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
            } {
                if index.default {
                    if default {
                        continue;
                    }
                    default = true;
                }
                indexes.push(index);
            }
            if !default {
                indexes.push(&*DEFAULT_INDEX);
            }

            indexes.reverse();
            indexes
        }
    }

    /// Return a vector containing all known [`Index`] entries.
    ///
    /// This includes explicit indexes, implicit indexes, flat indexes, and default indexes;
    /// in short, it includes all defined indexes, even if they're overridden by some other index
    /// definition.
    ///
    /// The indexes will be returned in the reverse of the order in which they were defined, such
    /// that the last-defined index is the first item in the vector.
    pub fn known_indexes(&'a self) -> impl Iterator<Item = &'a Index> {
        if self.no_index {
            Either::Left(self.flat_index.iter().rev())
        } else {
            Either::Right(
                std::iter::once(&*DEFAULT_INDEX)
                    .chain(self.flat_index.iter().rev())
                    .chain(self.indexes.iter().rev()),
            )
        }
    }

    /// Add all authenticated sources to the cache.
    pub fn cache_index_credentials(&self) {
        for index in self.known_indexes() {
            if let Some(credentials) = index.credentials() {
                let credentials = Arc::new(credentials);
                uv_auth::store_credentials(index.raw_url(), credentials.clone());
                if let Some(root_url) = index.root_url() {
                    uv_auth::store_credentials(&root_url, credentials.clone());
                }
            }
        }
    }

    /// Return the Simple API cache control header for an [`IndexUrl`], if configured.
    pub fn simple_api_cache_control_for(&self, url: &IndexUrl) -> Option<&str> {
        for index in &self.indexes {
            if index.url() == url {
                return index.cache_control.as_ref()?.api.as_deref();
            }
        }
        None
    }

    /// Return the artifact cache control header for an [`IndexUrl`], if configured.
    pub fn artifact_cache_control_for(&self, url: &IndexUrl) -> Option<&str> {
        for index in &self.indexes {
            if index.url() == url {
                return index.cache_control.as_ref()?.files.as_deref();
            }
        }
        None
    }
}

impl From<&IndexLocations> for uv_auth::Indexes {
    fn from(index_locations: &IndexLocations) -> Self {
        Self::from_indexes(index_locations.allowed_indexes().into_iter().map(|index| {
            let mut url = index.url().url().clone();
            url.set_username("").ok();
            url.set_password(None).ok();
            let mut root_url = index.url().root().unwrap_or_else(|| url.clone());
            root_url.set_username("").ok();
            root_url.set_password(None).ok();
            uv_auth::Index {
                url,
                root_url,
                auth_policy: index.authenticate,
            }
        }))
    }
}

/// The index URLs to use for fetching packages.
///
/// This type merges the legacy `--index-url` and `--extra-index-url` options, along with the
/// uv-specific `--index` and `--default-index`.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct IndexUrls {
    indexes: Vec<Index>,
    no_index: bool,
}

impl<'a> IndexUrls {
    pub fn from_indexes(indexes: Vec<Index>) -> Self {
        Self {
            indexes,
            no_index: false,
        }
    }

    /// Return the default [`Index`] entry.
    ///
    /// If `--no-index` is set, return `None`.
    ///
    /// If no index is provided, use the `PyPI` index.
    fn default_index(&'a self) -> Option<&'a Index> {
        if self.no_index {
            None
        } else {
            let mut seen = FxHashSet::default();
            self.indexes
                .iter()
                .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
                .find(|index| index.default)
                .or_else(|| Some(&DEFAULT_INDEX))
        }
    }

    /// Return an iterator over the implicit [`Index`] entries.
    ///
    /// Default and explicit indexes are excluded.
    fn implicit_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            let mut seen = FxHashSet::default();
            Either::Right(
                self.indexes
                    .iter()
                    .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
                    .filter(|index| !index.default && !index.explicit),
            )
        }
    }

    /// Return an iterator over all [`IndexUrl`] entries in order.
    ///
    /// Prioritizes the `[tool.uv.index]` definitions over the `--extra-index-url` definitions
    /// over the `--index-url` definition.
    ///
    /// If `no_index` was enabled, then this always returns an empty
    /// iterator.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        let mut seen = FxHashSet::default();
        self.implicit_indexes()
            .chain(self.default_index())
            .filter(|index| !index.explicit)
            .filter(move |index| seen.insert(index.raw_url())) // Filter out redundant raw URLs
    }

    /// Return an iterator over all user-defined [`Index`] entries in order.
    ///
    /// Prioritizes the `[tool.uv.index]` definitions over the `--extra-index-url` definitions
    /// over the `--index-url` definition.
    ///
    /// Unlike [`IndexUrl::indexes`], this includes explicit indexes and does _not_ insert PyPI
    /// as a fallback default.
    ///
    /// If `no_index` was enabled, then this always returns an empty
    /// iterator.
    pub fn defined_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            return Either::Left(std::iter::empty());
        }

        let mut seen = FxHashSet::default();
        let (non_default, default) = self
            .indexes
            .iter()
            .filter(move |index| {
                if let Some(name) = &index.name {
                    seen.insert(name)
                } else {
                    true
                }
            })
            .partition::<Vec<_>, _>(|index| !index.default);

        Either::Right(non_default.into_iter().chain(default))
    }

    /// Return the `--no-index` flag.
    pub fn no_index(&self) -> bool {
        self.no_index
    }

    /// Return the [`IndexStatusCodeStrategy`] for an [`IndexUrl`].
    pub fn status_code_strategy_for(&self, url: &IndexUrl) -> IndexStatusCodeStrategy {
        for index in &self.indexes {
            if index.url() == url {
                return index.status_code_strategy();
            }
        }
        IndexStatusCodeStrategy::Default
    }

    /// Return the Simple API cache control header for an [`IndexUrl`], if configured.
    pub fn simple_api_cache_control_for(&self, url: &IndexUrl) -> Option<&str> {
        for index in &self.indexes {
            if index.url() == url {
                return index.cache_control.as_ref()?.api.as_deref();
            }
        }
        None
    }

    /// Return the artifact cache control header for an [`IndexUrl`], if configured.
    pub fn artifact_cache_control_for(&self, url: &IndexUrl) -> Option<&str> {
        for index in &self.indexes {
            if index.url() == url {
                return index.cache_control.as_ref()?.files.as_deref();
            }
        }
        None
    }
}

bitflags::bitflags! {
    #[derive(Debug, Copy, Clone)]
    struct Flags: u8 {
        /// Whether the index supports range requests.
        const NO_RANGE_REQUESTS = 1;
        /// Whether the index returned a `401 Unauthorized` status code.
        const UNAUTHORIZED      = 1 << 2;
        /// Whether the index returned a `403 Forbidden` status code.
        const FORBIDDEN         = 1 << 1;
    }
}

/// A map of [`IndexUrl`]s to their capabilities.
///
/// We only store indexes that lack capabilities (i.e., don't support range requests, aren't
/// authorized). The benefit is that the map is almost always empty, so validating capabilities is
/// extremely cheap.
#[derive(Debug, Default, Clone)]
pub struct IndexCapabilities(Arc<RwLock<FxHashMap<IndexUrl, Flags>>>);

impl IndexCapabilities {
    /// Returns `true` if the given [`IndexUrl`] supports range requests.
    pub fn supports_range_requests(&self, index_url: &IndexUrl) -> bool {
        !self
            .0
            .read()
            .unwrap()
            .get(index_url)
            .is_some_and(|flags| flags.intersects(Flags::NO_RANGE_REQUESTS))
    }

    /// Mark an [`IndexUrl`] as not supporting range requests.
    pub fn set_no_range_requests(&self, index_url: IndexUrl) {
        self.0
            .write()
            .unwrap()
            .entry(index_url)
            .or_insert(Flags::empty())
            .insert(Flags::NO_RANGE_REQUESTS);
    }

    /// Returns `true` if the given [`IndexUrl`] returns a `401 Unauthorized` status code.
    pub fn unauthorized(&self, index_url: &IndexUrl) -> bool {
        self.0
            .read()
            .unwrap()
            .get(index_url)
            .is_some_and(|flags| flags.intersects(Flags::UNAUTHORIZED))
    }

    /// Mark an [`IndexUrl`] as returning a `401 Unauthorized` status code.
    pub fn set_unauthorized(&self, index_url: IndexUrl) {
        self.0
            .write()
            .unwrap()
            .entry(index_url)
            .or_insert(Flags::empty())
            .insert(Flags::UNAUTHORIZED);
    }

    /// Returns `true` if the given [`IndexUrl`] returns a `403 Forbidden` status code.
    pub fn forbidden(&self, index_url: &IndexUrl) -> bool {
        self.0
            .read()
            .unwrap()
            .get(index_url)
            .is_some_and(|flags| flags.intersects(Flags::FORBIDDEN))
    }

    /// Mark an [`IndexUrl`] as returning a `403 Forbidden` status code.
    pub fn set_forbidden(&self, index_url: IndexUrl) {
        self.0
            .write()
            .unwrap()
            .entry(index_url)
            .or_insert(Flags::empty())
            .insert(Flags::FORBIDDEN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_url_parse_valid_paths() {
        // Absolute path
        assert!(is_disambiguated_path("/absolute/path"));
        // Relative path
        assert!(is_disambiguated_path("./relative/path"));
        assert!(is_disambiguated_path("../../relative/path"));
        if cfg!(windows) {
            // Windows absolute path
            assert!(is_disambiguated_path("C:/absolute/path"));
            // Windows relative path
            assert!(is_disambiguated_path(".\\relative\\path"));
            assert!(is_disambiguated_path("..\\..\\relative\\path"));
        }
    }

    #[test]
    fn test_index_url_parse_ambiguous_paths() {
        // Test single-segment ambiguous path
        assert!(!is_disambiguated_path("index"));
        // Test multi-segment ambiguous path
        assert!(!is_disambiguated_path("relative/path"));
    }

    #[test]
    fn test_index_url_parse_with_schemes() {
        assert!(is_disambiguated_path("file:///absolute/path"));
        assert!(is_disambiguated_path("https://registry.com/simple/"));
        assert!(is_disambiguated_path(
            "git+https://github.com/example/repo.git"
        ));
    }

    #[test]
    fn test_cache_control_lookup() {
        use std::str::FromStr;

        use uv_small_str::SmallString;

        use crate::IndexFormat;
        use crate::index_name::IndexName;

        let indexes = vec![
            Index {
                name: Some(IndexName::from_str("index1").unwrap()),
                url: IndexUrl::from_str("https://index1.example.com/simple").unwrap(),
                cache_control: Some(crate::IndexCacheControl {
                    api: Some(SmallString::from("max-age=300")),
                    files: Some(SmallString::from("max-age=1800")),
                }),
                explicit: false,
                default: false,
                origin: None,
                format: IndexFormat::Simple,
                publish_url: None,
                authenticate: uv_auth::AuthPolicy::default(),
                ignore_error_codes: None,
            },
            Index {
                name: Some(IndexName::from_str("index2").unwrap()),
                url: IndexUrl::from_str("https://index2.example.com/simple").unwrap(),
                cache_control: None,
                explicit: false,
                default: false,
                origin: None,
                format: IndexFormat::Simple,
                publish_url: None,
                authenticate: uv_auth::AuthPolicy::default(),
                ignore_error_codes: None,
            },
        ];

        let index_urls = IndexUrls::from_indexes(indexes);

        let url1 = IndexUrl::from_str("https://index1.example.com/simple").unwrap();
        assert_eq!(
            index_urls.simple_api_cache_control_for(&url1),
            Some("max-age=300")
        );
        assert_eq!(
            index_urls.artifact_cache_control_for(&url1),
            Some("max-age=1800")
        );

        let url2 = IndexUrl::from_str("https://index2.example.com/simple").unwrap();
        assert_eq!(index_urls.simple_api_cache_control_for(&url2), None);
        assert_eq!(index_urls.artifact_cache_control_for(&url2), None);

        let url3 = IndexUrl::from_str("https://index3.example.com/simple").unwrap();
        assert_eq!(index_urls.simple_api_cache_control_for(&url3), None);
        assert_eq!(index_urls.artifact_cache_control_for(&url3), None);
    }
}
