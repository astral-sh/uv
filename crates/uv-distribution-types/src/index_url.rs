use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, RwLock};

use http::StatusCode;
use itertools::Either;
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;
use url::{ParseError, Url};
use uv_auth::RealmRef;
use uv_cache_key::CanonicalUrl;
use uv_pep508::{Scheme, VerbatimUrl, VerbatimUrlError, split_scheme};
use uv_pypi_types::HashAlgorithm;
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user;

use crate::{
    ArtifactUrlMapError, ExcludeNewerOverride, FileLocation, Index, IndexFormat, IndexReference,
    IndexStatusCodeStrategy, ProxyIndex, ValidatedArtifactUrlMap, Verbatim,
};

pub static PYPI_URL: LazyLock<DisplaySafeUrl> =
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
    #[inline]
    fn inner(&self) -> &VerbatimUrl {
        match self {
            Self::Pypi(url) | Self::Url(url) | Self::Path(url) => url,
        }
    }

    /// Return the raw URL for the index.
    pub fn url(&self) -> &DisplaySafeUrl {
        self.inner().raw()
    }

    /// Convert the index URL into a [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        match self {
            Self::Pypi(url) | Self::Url(url) | Self::Path(url) => {
                Arc::unwrap_or_clone(url).into_url()
            }
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

        if let Some(path) = verbatim_url.given()
            && !is_disambiguated_path(path)
        {
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

impl Display for IndexUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.inner(), f)
    }
}

impl Verbatim for IndexUrl {
    fn verbatim(&self) -> Cow<'_, str> {
        self.inner().verbatim()
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
        self.inner().without_credentials().serialize(serializer)
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
        index.into_url()
    }
}

impl Deref for IndexUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

/// The index locations to use for fetching packages. By default, uses the PyPI index.
///
/// This type merges the legacy `--index-url`, `--extra-index-url`, and `--find-links` options,
/// along with the uv-specific `--index` and `--default-index`.
#[derive(Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IndexLocations {
    indexes: Vec<Index>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    proxy_indexes: Vec<ProxyIndex>,
    flat_index: Vec<Index>,
    no_index: bool,
}

/// Preserve the existing output when no proxy indexes are configured.
impl std::fmt::Debug for IndexLocations {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = formatter.debug_struct("IndexLocations");
        debug.field("indexes", &self.indexes);
        if !self.proxy_indexes.is_empty() {
            debug.field("proxy_indexes", &self.proxy_indexes);
        }
        debug
            .field("flat_index", &self.flat_index)
            .field("no_index", &self.no_index)
            .finish()
    }
}

impl IndexLocations {
    /// Determine the index URLs to use for fetching packages.
    pub fn new(indexes: Vec<Index>, flat_index: Vec<Index>, no_index: bool) -> Self {
        Self {
            indexes,
            proxy_indexes: Vec::new(),
            flat_index,
            no_index,
        }
    }

    /// Configure proxy indexes for canonical package indexes.
    #[must_use]
    pub fn with_proxy_indexes(mut self, proxy_indexes: Vec<ProxyIndex>) -> Self {
        self.proxy_indexes = proxy_indexes;
        self
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
            proxy_indexes: self.proxy_indexes,
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

/// Returns `true` if two [`IndexUrl`]s refer to the same index.
fn is_same_index(a: &IndexUrl, b: &IndexUrl) -> bool {
    RealmRef::from(&**b.url()) == RealmRef::from(&**a.url())
        && CanonicalUrl::new(a.url().clone()) == CanonicalUrl::new(b.url().clone())
}

fn diagnostic_safe_index_url(index: &IndexUrl) -> DisplaySafeUrl {
    let mut url = index.url().clone();
    url.remove_credentials();
    url.set_query(None);
    url.set_fragment(None);
    url
}

/// Return user-defined indexes in priority order, excluding shadowed names.
fn prioritized_defined_indexes(indexes: &[Index]) -> impl Iterator<Item = &Index> {
    let mut seen = FxHashSet::default();
    let (non_default, default) = indexes
        .iter()
        .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
        .partition::<Vec<_>, _>(|index| !index.default);

    non_default.into_iter().chain(default)
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

    /// Return an iterator over the explicit [`Index`] entries.
    ///
    /// Explicit indexes are only used when pinned via `tool.uv.sources`.
    pub fn explicit_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            let mut seen = FxHashSet::default();
            Either::Right(
                self.indexes
                    .iter()
                    .filter(move |index| index.name.as_ref().is_none_or(|name| seen.insert(name)))
                    .filter(|index| index.explicit),
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

    /// Return an iterator over all [`Index`] entries to fetch in order.
    ///
    /// Unlike [`IndexLocations::indexes`], indexes with duplicate raw URLs are excluded.
    pub fn fetch_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        let mut seen = FxHashSet::default();
        self.indexes()
            .filter(move |index| seen.insert(index.raw_url()))
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

    /// Return the configured proxy indexes.
    pub fn proxy_indexes(&self) -> impl Iterator<Item = &ProxyIndex> {
        self.proxy_indexes.iter()
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

    /// Return an iterator over all user-defined [`Index`] entries in order.
    ///
    /// Prioritizes the `[tool.uv.index]` definitions over the `--extra-index-url` definitions
    /// over the `--index-url` definition.
    ///
    /// Unlike [`IndexLocations::indexes`], this includes explicit indexes and does _not_ insert
    /// PyPI as a fallback default.
    ///
    /// If `no_index` was enabled, then this always returns an empty iterator.
    pub fn defined_indexes(&'a self) -> impl Iterator<Item = &'a Index> + 'a {
        if self.no_index {
            return Either::Left(std::iter::empty());
        }

        Either::Right(prioritized_defined_indexes(&self.indexes))
    }

    /// Return the configured index matching the given URL.
    fn index_for_url(&self, url: &IndexUrl) -> Option<&Index> {
        self.indexes
            .iter()
            .find(|index| is_same_index(index.url(), url))
    }

    /// Return the [`IndexStatusCodeStrategy`] for an [`IndexUrl`].
    pub fn status_code_strategy_for(&self, url: &IndexUrl) -> IndexStatusCodeStrategy {
        self.index_for_url(url).map_or(
            IndexStatusCodeStrategy::Default,
            Index::status_code_strategy,
        )
    }

    /// Return whether the given status code is explicitly ignored for an [`IndexUrl`].
    pub fn ignores_error_code_for(&self, url: &IndexUrl, status_code: StatusCode) -> bool {
        self.index_for_url(url)
            .is_some_and(|index| index.ignores_error_code(status_code))
    }

    /// Return the Simple API cache control header for an [`IndexUrl`], if configured.
    pub fn simple_api_cache_control_for(&self, url: &IndexUrl) -> Option<http::HeaderValue> {
        self.index_for_url(url)
            .and_then(Index::simple_api_cache_control)
    }

    /// Return the artifact cache control header for an [`IndexUrl`], if configured.
    pub fn artifact_cache_control_for(&self, url: &IndexUrl) -> Option<http::HeaderValue> {
        self.index_for_url(url)
            .and_then(Index::artifact_cache_control)
    }

    /// Return the hash algorithm required for distributions resolved from a given index.
    pub fn hash_algorithm_for(&self, url: &IndexUrl) -> Option<HashAlgorithm> {
        self.index_for_url(url)
            .and_then(|index| index.hash_algorithm.map(HashAlgorithm::from))
    }

    /// Return the `exclude-newer` setting for a given index, if the index is configured.
    pub fn exclude_newer_for(&self, url: &IndexUrl) -> Option<&ExcludeNewerOverride> {
        self.index_for_url(url).and_then(Index::exclude_newer)
    }
}

/// Convert an index URL into an authentication scope without retaining credentials.
fn authentication_index(index: &IndexUrl, auth_policy: uv_auth::AuthPolicy) -> uv_auth::Index {
    let mut url = index.url().clone();
    url.set_username("").ok();
    url.set_password(None).ok();
    let mut root_url = index.root().unwrap_or_else(|| url.clone());
    root_url.set_username("").ok();
    root_url.set_password(None).ok();
    uv_auth::Index {
        url,
        root_url,
        auth_policy,
    }
}

impl From<&IndexLocations> for uv_auth::Indexes {
    fn from(index_locations: &IndexLocations) -> Self {
        let configured_indexes = index_locations.allowed_indexes();
        let indexes = configured_indexes
            .iter()
            .map(|index| authentication_index(index.url(), index.authenticate))
            .chain(
                index_locations
                    .proxy_indexes()
                    .filter(|proxy_index| {
                        !configured_indexes
                            .iter()
                            .any(|index| is_same_index(index.url(), &proxy_index.url))
                    })
                    .map(|proxy_index| {
                        authentication_index(&proxy_index.url, uv_auth::AuthPolicy::Auto)
                    }),
            );
        Self::from_indexes(indexes)
    }
}

/// Validated routes from canonical indexes to physical metadata endpoints.
#[derive(Debug, Clone, Default)]
pub struct IndexRoutes {
    routes: Vec<(IndexUrl, IndexUrl)>,
}

impl IndexRoutes {
    /// Return the logical and physical endpoints for a canonical index.
    pub fn route_for<'index>(&'index self, canonical: &'index IndexUrl) -> IndexRoute<'index> {
        let physical = self
            .routes
            .iter()
            .find(|(route_canonical, _)| is_same_index(route_canonical, canonical))
            .map_or(canonical, |(_, physical)| physical);
        IndexRoute {
            canonical,
            physical,
        }
    }

    /// Return the configured proxy routes.
    pub fn proxy_routes(&self) -> impl Iterator<Item = IndexRoute<'_>> {
        self.routes.iter().map(|(canonical, physical)| IndexRoute {
            canonical,
            physical,
        })
    }
}

impl TryFrom<&IndexLocations> for IndexRoutes {
    type Error = ProxyIndexError;

    fn try_from(index_locations: &IndexLocations) -> Result<Self, Self::Error> {
        let effective_indexes =
            prioritized_defined_indexes(&index_locations.indexes).collect::<Vec<_>>();
        let mut routes =
            Vec::<(IndexUrl, IndexUrl)>::with_capacity(index_locations.proxy_indexes.len());
        for proxy_index in &index_locations.proxy_indexes {
            let canonical = match &proxy_index.index {
                IndexReference::Name(name) => effective_indexes
                    .iter()
                    .copied()
                    .find(|index| index.name.as_ref() == Some(name))
                    .map(|index| index.url().clone())
                    .ok_or_else(|| ProxyIndexError::UnknownIndex { name: name.clone() })?,
                IndexReference::Url(url) => url.clone(),
            };
            if effective_indexes
                .iter()
                .copied()
                .chain(&index_locations.flat_index)
                .any(|index| {
                    index.format == IndexFormat::Flat && is_same_index(index.url(), &canonical)
                })
            {
                return Err(ProxyIndexError::FlatIndex { index: canonical });
            }
            if matches!(canonical, IndexUrl::Path(_)) {
                return Err(ProxyIndexError::PathIndex { index: canonical });
            }
            if matches!(proxy_index.url, IndexUrl::Path(_)) {
                return Err(ProxyIndexError::PathIndex {
                    index: proxy_index.url.clone(),
                });
            }
            if is_same_index(&canonical, &proxy_index.url) {
                return Err(ProxyIndexError::SelfProxy { index: canonical });
            }
            if routes
                .iter()
                .any(|(route_canonical, _)| is_same_index(route_canonical, &canonical))
            {
                return Err(ProxyIndexError::Duplicate { index: canonical });
            }
            routes.push((canonical, proxy_index.url.clone()));
        }
        Ok(Self { routes })
    }
}

/// The logical and physical endpoints for an index request.
#[derive(Debug, Clone, Copy)]
pub struct IndexRoute<'index> {
    pub canonical: &'index IndexUrl,
    pub physical: &'index IndexUrl,
}

impl IndexRoute<'_> {
    /// Return `true` if the index request is routed through a proxy.
    pub fn is_proxy(self) -> bool {
        !is_same_index(self.canonical, self.physical)
    }
}

/// Validated output-only routes for canonicalizing proxy artifacts.
#[derive(Debug, Clone)]
pub struct ProxyArtifactRoutes {
    routes: Vec<ProxyArtifactRouteData>,
}

impl ProxyArtifactRoutes {
    /// Return the output route for a canonical proxy index.
    pub fn route_for(&self, canonical: &IndexUrl) -> Option<ProxyArtifactRoute<'_>> {
        self.routes
            .iter()
            .find(|route| is_same_index(&route.canonical, canonical))
            .map(|route| ProxyArtifactRoute {
                physical: &route.physical,
                artifact_url_map: &route.artifact_url_map,
            })
    }
}

impl TryFrom<&IndexLocations> for ProxyArtifactRoutes {
    type Error = ProxyIndexError;

    fn try_from(index_locations: &IndexLocations) -> Result<Self, Self::Error> {
        let index_routes = IndexRoutes::try_from(index_locations)?;
        let routes = index_routes
            .proxy_routes()
            .zip(index_locations.proxy_indexes())
            .map(|(route, proxy_index)| {
                let artifact_url_map =
                    proxy_index.artifact_url_map.validate().map_err(|source| {
                        ProxyIndexError::ArtifactUrlMap {
                            index: diagnostic_safe_index_url(&proxy_index.url),
                            source: Box::new(source),
                        }
                    })?;
                Ok(ProxyArtifactRouteData {
                    canonical: route.canonical.clone(),
                    physical: route.physical.clone(),
                    artifact_url_map,
                })
            })
            .collect::<Result<Vec<_>, ProxyIndexError>>()?;
        Ok(Self { routes })
    }
}

#[derive(Debug, Clone)]
struct ProxyArtifactRouteData {
    canonical: IndexUrl,
    physical: IndexUrl,
    artifact_url_map: ValidatedArtifactUrlMap,
}

/// A validated output-only route for canonicalizing proxy artifacts.
#[derive(Debug, Clone, Copy)]
pub struct ProxyArtifactRoute<'route> {
    physical: &'route IndexUrl,
    artifact_url_map: &'route ValidatedArtifactUrlMap,
}

impl ProxyArtifactRoute<'_> {
    /// Return `true` if the physical index identifies this proxy route.
    pub fn matches_physical(self, physical: &IndexUrl) -> bool {
        is_same_index(self.physical, physical)
    }

    /// Map a physical proxy artifact URL to its canonical lock representation.
    pub fn canonical_artifact_url(
        self,
        physical: &FileLocation,
        filename: &str,
    ) -> Result<FileLocation, ArtifactUrlMapError> {
        self.artifact_url_map
            .canonical_artifact_url(physical, filename)
    }
}

/// An invalid proxy-index configuration.
#[derive(Debug, Clone, Eq, PartialEq, Error)]
pub enum ProxyIndexError {
    #[error("Proxy index references unknown index `{name}`")]
    UnknownIndex { name: crate::IndexName },
    #[error("Multiple proxy indexes are configured for `{index}`")]
    Duplicate { index: IndexUrl },
    #[error("Proxy index for `{index}` points to the canonical index itself")]
    SelfProxy { index: IndexUrl },
    #[error("Proxy index for `{index}` references a flat index")]
    FlatIndex { index: IndexUrl },
    #[error("Proxy index mappings do not support path-backed index `{index}`")]
    PathIndex { index: IndexUrl },
    #[error("Invalid artifact URL map for proxy index `{index}`")]
    ArtifactUrlMap {
        index: DisplaySafeUrl,
        #[source]
        source: Box<ArtifactUrlMapError>,
    },
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
    pub(crate) fn set_unauthorized(&self, index_url: IndexUrl) {
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
    pub(crate) fn set_forbidden(&self, index_url: IndexUrl) {
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
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        ArtifactUrlMap, IndexCacheControl, IndexFormat, IndexName, IndexReference, ProxyIndex,
    };
    use http::HeaderValue;

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
    fn fetch_indexes_deduplicates_raw_urls() {
        let url = IndexUrl::from_str("https://index.example.com/simple").unwrap();
        let mut first = Index::from(url.clone());
        first.name = Some(IndexName::from_str("first").unwrap());
        let mut second = Index::from(url);
        second.name = Some(IndexName::from_str("second").unwrap());
        second.default = true;
        let locations = IndexLocations::new(vec![first, second], Vec::new(), false);

        assert_eq!(locations.indexes().count(), 2);
        assert_eq!(locations.fetch_indexes().count(), 1);
    }

    #[test]
    fn proxy_index_resolves_name() -> Result<(), Box<dyn std::error::Error>> {
        let canonical = IndexUrl::from_str("https://pypi.org/simple")?;
        let proxy = IndexUrl::from_str("https://packages.example.com/simple")?;
        let mut index = Index::from(canonical.clone());
        index.name = Some(IndexName::from_str("pypi")?);
        let locations =
            IndexLocations::new(vec![index], Vec::new(), false).with_proxy_indexes(vec![
                ProxyIndex {
                    index: IndexReference::Name(IndexName::from_str("pypi")?),
                    url: proxy.clone(),
                    artifact_url_map: artifact_url_map()?,
                },
            ]);

        let routes = IndexRoutes::try_from(&locations)?;
        let route = routes.route_for(&canonical);
        assert_eq!(route.canonical, &canonical);
        assert_eq!(route.physical, &proxy);
        assert!(route.is_proxy());
        Ok(())
    }

    #[test]
    fn proxy_index_matches_url_without_credentials() -> Result<(), Box<dyn std::error::Error>> {
        let authenticated = IndexUrl::from_str("https://user:secret@pypi.org/simple")?;
        let canonical = IndexUrl::from_str("https://pypi.org/simple")?;
        let proxy = IndexUrl::from_str("https://packages.example.com/simple")?;
        let locations = IndexLocations::default().with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Url(authenticated),
            url: proxy.clone(),
            artifact_url_map: artifact_url_map()?,
        }]);

        let routes = IndexRoutes::try_from(&locations)?;
        assert_eq!(routes.route_for(&canonical).physical, &proxy);
        Ok(())
    }

    #[test]
    fn proxy_index_preserves_configured_never_authentication_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let canonical = IndexUrl::from_str("https://pypi.org/simple")?;
        let proxy =
            IndexUrl::from_str("https://proxy-user:proxy-secret@packages.example.com/simple")?;
        let mut configured_proxy = Index::from(proxy.clone());
        configured_proxy.explicit = true;
        configured_proxy.authenticate = uv_auth::AuthPolicy::Never;
        assert!(configured_proxy.credentials()?.is_some());

        let locations = IndexLocations::new(
            vec![Index::from(canonical.clone()), configured_proxy],
            Vec::new(),
            false,
        )
        .with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Url(canonical),
            url: proxy,
            artifact_url_map: artifact_url_map()?,
        }]);
        let configured_indexes = locations.allowed_indexes();
        let expected = uv_auth::Indexes::from_indexes(
            configured_indexes
                .iter()
                .map(|index| authentication_index(index.url(), index.authenticate)),
        );

        assert_eq!(uv_auth::Indexes::from(&locations), expected);
        Ok(())
    }

    #[test]
    fn proxy_index_rejects_duplicate_and_self_mappings() -> Result<(), Box<dyn std::error::Error>> {
        let canonical = IndexUrl::from_str("https://pypi.org/simple")?;
        let duplicate = IndexLocations::default().with_proxy_indexes(vec![
            ProxyIndex {
                index: IndexReference::Url(canonical.clone()),
                url: IndexUrl::from_str("https://one.example.com/simple")?,
                artifact_url_map: artifact_url_map()?,
            },
            ProxyIndex {
                index: IndexReference::Url(IndexUrl::from_str("https://pypi.org/simple/")?),
                url: IndexUrl::from_str("https://two.example.com/simple")?,
                artifact_url_map: artifact_url_map()?,
            },
        ]);
        assert!(matches!(
            IndexRoutes::try_from(&duplicate),
            Err(ProxyIndexError::Duplicate { .. })
        ));

        let self_proxy = IndexLocations::default().with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Url(canonical),
            url: IndexUrl::from_str("https://pypi.org/simple/")?,
            artifact_url_map: artifact_url_map()?,
        }]);
        assert!(matches!(
            IndexRoutes::try_from(&self_proxy),
            Err(ProxyIndexError::SelfProxy { .. })
        ));
        Ok(())
    }

    #[test]
    fn proxy_index_rejects_unknown_and_flat_indexes() -> Result<(), Box<dyn std::error::Error>> {
        let unknown = IndexLocations::default().with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Name(IndexName::from_str("unknown")?),
            url: IndexUrl::from_str("https://proxy.example.com/simple")?,
            artifact_url_map: artifact_url_map()?,
        }]);
        assert!(matches!(
            IndexRoutes::try_from(&unknown),
            Err(ProxyIndexError::UnknownIndex { .. })
        ));

        let canonical = IndexUrl::from_str("https://canonical.example.com/packages")?;
        let mut index = Index::from(canonical);
        index.name = Some(IndexName::from_str("canonical")?);
        index.format = IndexFormat::Flat;
        let flat = IndexLocations::new(vec![index], Vec::new(), false).with_proxy_indexes(vec![
            ProxyIndex {
                index: IndexReference::Name(IndexName::from_str("canonical")?),
                url: IndexUrl::from_str("https://proxy.example.com/packages")?,
                artifact_url_map: artifact_url_map()?,
            },
        ]);
        assert!(matches!(
            IndexRoutes::try_from(&flat),
            Err(ProxyIndexError::FlatIndex { .. })
        ));
        Ok(())
    }

    #[test]
    fn proxy_index_rejects_path_indexes() -> Result<(), Box<dyn std::error::Error>> {
        let locations = IndexLocations::default().with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Url(IndexUrl::from_str("https://pypi.org/simple")?),
            url: IndexUrl::from_str("./proxy")?,
            artifact_url_map: artifact_url_map()?,
        }]);
        assert!(matches!(
            IndexRoutes::try_from(&locations),
            Err(ProxyIndexError::PathIndex { .. })
        ));
        Ok(())
    }

    #[test]
    fn proxy_index_requires_non_empty_artifact_url_map() -> Result<(), Box<dyn std::error::Error>> {
        let missing = toml::from_str::<ProxyIndex>(
            r#"
index = "https://pypi.org/simple"
url = "https://proxy.example/simple"
"#,
        );
        assert!(missing.is_err());

        let empty = toml::from_str::<ProxyIndex>(
            r#"
index = "https://pypi.org/simple"
url = "https://proxy.example/simple"
artifact-url-map = {}
"#,
        )?;
        let locations = IndexLocations::default().with_proxy_indexes(vec![empty]);
        assert!(IndexRoutes::try_from(&locations).is_ok());
        assert!(matches!(
            ProxyArtifactRoutes::try_from(&locations),
            Err(ProxyIndexError::ArtifactUrlMap {
                source,
                ..
            }) if matches!(source.as_ref(), ArtifactUrlMapError::Empty)
        ));
        Ok(())
    }

    #[test]
    fn proxy_artifact_route_errors_redact_index_secrets() -> Result<(), Box<dyn std::error::Error>>
    {
        let locations = IndexLocations::default().with_proxy_indexes(vec![ProxyIndex {
            index: IndexReference::Url(IndexUrl::from_str("https://pypi.org/simple")?),
            url: IndexUrl::from_str(
                "https://username-token:password-token@proxy.example/simple?query-token=secret#fragment-token=secret",
            )?,
            artifact_url_map: ArtifactUrlMap::new(BTreeMap::default()),
        }]);

        let error = ProxyArtifactRoutes::try_from(&locations)
            .expect_err("an empty artifact URL map should be rejected");
        let ProxyIndexError::ArtifactUrlMap { index, .. } = &error else {
            return Err("expected artifact URL map error".into());
        };
        assert_eq!(index.as_str(), "https://proxy.example/simple");
        assert!(error.to_string().contains("https://proxy.example/simple"));
        for diagnostic in [error.to_string(), format!("{error:?}")] {
            assert!(!diagnostic.contains("username-token"));
            assert!(!diagnostic.contains("password-token"));
            assert!(!diagnostic.contains("query-token"));
            assert!(!diagnostic.contains("fragment-token"));
        }
        Ok(())
    }

    #[test]
    fn proxy_artifact_routes_preserve_declaration_map_pairing()
    -> Result<(), Box<dyn std::error::Error>> {
        let first_canonical = IndexUrl::from_str("https://one.example/simple")?;
        let second_canonical = IndexUrl::from_str("https://two.example/simple")?;
        let first_physical = IndexUrl::from_str("https://proxy.example/one/simple")?;
        let second_physical = IndexUrl::from_str("https://proxy.example/two/simple")?;
        let locations = IndexLocations::default().with_proxy_indexes(vec![
            ProxyIndex {
                index: IndexReference::Url(first_canonical.clone()),
                url: first_physical,
                artifact_url_map: ArtifactUrlMap::single(
                    DisplaySafeUrl::parse("https://files.proxy.example/one")?,
                    DisplaySafeUrl::parse("https://files.example/one")?,
                ),
            },
            ProxyIndex {
                index: IndexReference::Url(second_canonical.clone()),
                url: second_physical,
                artifact_url_map: ArtifactUrlMap::single(
                    DisplaySafeUrl::parse("https://files.proxy.example/two")?,
                    DisplaySafeUrl::parse("https://files.example/two")?,
                ),
            },
        ]);

        let routes = ProxyArtifactRoutes::try_from(&locations)?;
        let first = routes
            .route_for(&first_canonical)
            .expect("first output route should exist");
        let second = routes
            .route_for(&second_canonical)
            .expect("second output route should exist");

        let first_artifact = FileLocation::new(
            "https://files.proxy.example/one/example.whl".into(),
            &"".into(),
        );
        assert_eq!(
            first
                .canonical_artifact_url(&first_artifact, "example.whl")?
                .to_url()?
                .as_str(),
            "https://files.example/one/example.whl"
        );

        let second_artifact = FileLocation::new(
            "https://files.proxy.example/two/example.tar.gz".into(),
            &"".into(),
        );
        assert_eq!(
            second
                .canonical_artifact_url(&second_artifact, "example.tar.gz")?
                .to_url()?
                .as_str(),
            "https://files.example/two/example.tar.gz"
        );
        Ok(())
    }

    fn artifact_url_map() -> Result<ArtifactUrlMap, uv_redacted::DisplaySafeUrlError> {
        Ok(ArtifactUrlMap::single(
            DisplaySafeUrl::parse("https://proxy.example/files")?,
            DisplaySafeUrl::parse("https://canonical.example/files")?,
        ))
    }

    #[test]
    fn test_cache_control_lookup() {
        use std::str::FromStr;

        use crate::IndexFormat;
        use crate::index_name::IndexName;

        let indexes = vec![
            Index {
                name: Some(IndexName::from_str("index1").unwrap()),
                url: IndexUrl::from_str("https://index1.example.com/simple").unwrap(),
                cache_control: Some(crate::IndexCacheControl {
                    api: Some(HeaderValue::from_static("max-age=300")),
                    files: Some(HeaderValue::from_static("max-age=1800")),
                }),
                explicit: false,
                default: false,
                origin: None,
                format: IndexFormat::Simple,
                publish_url: None,
                authenticate: uv_auth::AuthPolicy::default(),
                ignore_error_codes: None,
                hash_algorithm: None,
                exclude_newer: None,
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
                hash_algorithm: None,
                exclude_newer: None,
            },
        ];

        let index_locations = IndexLocations::new(indexes, Vec::new(), false);

        let url1 = IndexUrl::from_str("https://index1.example.com/simple").unwrap();
        assert_eq!(
            index_locations.simple_api_cache_control_for(&url1),
            Some(HeaderValue::from_static("max-age=300"))
        );
        assert_eq!(
            index_locations.artifact_cache_control_for(&url1),
            Some(HeaderValue::from_static("max-age=1800"))
        );

        let url2 = IndexUrl::from_str("https://index2.example.com/simple").unwrap();
        assert_eq!(index_locations.simple_api_cache_control_for(&url2), None);
        assert_eq!(index_locations.artifact_cache_control_for(&url2), None);

        let url3 = IndexUrl::from_str("https://index3.example.com/simple").unwrap();
        assert_eq!(index_locations.simple_api_cache_control_for(&url3), None);
        assert_eq!(index_locations.artifact_cache_control_for(&url3), None);
    }

    #[test]
    fn test_pytorch_default_cache_control() {
        // Test that PyTorch indexes get default cache control from the getter methods
        let indexes = vec![Index {
            name: Some(IndexName::from_str("pytorch").unwrap()),
            url: IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap(),
            cache_control: None, // No explicit cache control
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: uv_auth::AuthPolicy::default(),
            ignore_error_codes: None,
            hash_algorithm: None,
            exclude_newer: None,
        }];

        let index_locations = IndexLocations::new(indexes, Vec::new(), false);

        let pytorch_url = IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap();

        assert_eq!(
            index_locations.simple_api_cache_control_for(&pytorch_url),
            None
        );
        assert_eq!(
            index_locations.artifact_cache_control_for(&pytorch_url),
            Some(HeaderValue::from_static(
                "max-age=365000000, immutable, public",
            ))
        );
    }

    #[test]
    fn test_pytorch_user_override_cache_control() {
        // Test that user-specified cache control overrides PyTorch defaults
        let indexes = vec![Index {
            name: Some(IndexName::from_str("pytorch").unwrap()),
            url: IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap(),
            cache_control: Some(IndexCacheControl {
                api: Some(HeaderValue::from_static("no-cache")),
                files: Some(HeaderValue::from_static("max-age=3600")),
            }),
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: uv_auth::AuthPolicy::default(),
            ignore_error_codes: None,
            hash_algorithm: None,
            exclude_newer: None,
        }];

        let index_locations = IndexLocations::new(indexes, Vec::new(), false);

        let pytorch_url = IndexUrl::from_str("https://download.pytorch.org/whl/cu118").unwrap();

        assert_eq!(
            index_locations.simple_api_cache_control_for(&pytorch_url),
            Some(HeaderValue::from_static("no-cache"))
        );
        assert_eq!(
            index_locations.artifact_cache_control_for(&pytorch_url),
            Some(HeaderValue::from_static("max-age=3600"))
        );
    }

    #[test]
    fn test_nvidia_default_cache_control() {
        // Test that NVIDIA indexes get default cache control from the getter methods
        let indexes = vec![Index {
            name: Some(IndexName::from_str("nvidia").unwrap()),
            url: IndexUrl::from_str("https://pypi.nvidia.com").unwrap(),
            cache_control: None, // No explicit cache control
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: uv_auth::AuthPolicy::default(),
            ignore_error_codes: None,
            hash_algorithm: None,
            exclude_newer: None,
        }];

        let index_locations = IndexLocations::new(indexes, Vec::new(), false);

        let nvidia_url = IndexUrl::from_str("https://pypi.nvidia.com").unwrap();

        assert_eq!(
            index_locations.simple_api_cache_control_for(&nvidia_url),
            None
        );
        assert_eq!(
            index_locations.artifact_cache_control_for(&nvidia_url),
            Some(HeaderValue::from_static(
                "max-age=365000000, immutable, public",
            ))
        );
    }
}
