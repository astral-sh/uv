use itertools::Either;
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, RwLock};
use thiserror::Error;
use url::{ParseError, Url};

use uv_pep508::{VerbatimUrl, VerbatimUrlError};

use crate::{Index, Verbatim};

static PYPI_URL: LazyLock<Url> = LazyLock::new(|| Url::parse("https://pypi.org/simple").unwrap());

static DEFAULT_INDEX: LazyLock<Index> = LazyLock::new(|| {
    Index::from_index_url(IndexUrl::Pypi(VerbatimUrl::from_url(PYPI_URL.clone())))
});

/// The URL of an index to use for fetching packages (e.g., PyPI).
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum IndexUrl {
    Pypi(VerbatimUrl),
    Url(VerbatimUrl),
    Path(VerbatimUrl),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for IndexUrl {
    fn schema_name() -> String {
        "IndexUrl".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("The URL of an index to use for fetching packages (e.g., `https://pypi.org/simple`).".to_string()),
              ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

impl IndexUrl {
    /// Return the raw URL for the index.
    pub fn url(&self) -> &Url {
        match self {
            Self::Pypi(url) => url.raw(),
            Self::Url(url) => url.raw(),
            Self::Path(url) => url.raw(),
        }
    }

    /// Convert the index URL into a [`Url`].
    pub fn into_url(self) -> Url {
        match self {
            Self::Pypi(url) => url.into_url(),
            Self::Url(url) => url.into_url(),
            Self::Path(url) => url.into_url(),
        }
    }

    /// Return the redacted URL for the index, omitting any sensitive credentials.
    pub fn redacted(&self) -> Cow<'_, Url> {
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
        let url = if Path::new(s).exists() {
            VerbatimUrl::from_absolute_path(std::path::absolute(s)?)?
        } else {
            VerbatimUrl::parse_url(s)?
        };
        Ok(Self::from(url.with_given(s)))
    }
}

impl serde::ser::Serialize for IndexUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> serde::de::Deserialize<'de> for IndexUrl {
    fn deserialize<D>(deserializer: D) -> Result<IndexUrl, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        IndexUrl::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl From<VerbatimUrl> for IndexUrl {
    fn from(url: VerbatimUrl) -> Self {
        if url.scheme() == "file" {
            Self::Path(url)
        } else if *url.raw() == *PYPI_URL {
            Self::Pypi(url)
        } else {
            Self::Url(url)
        }
    }
}

impl From<IndexUrl> for Url {
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
        match &self {
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
                .filter(move |index| index.name.as_ref().map_or(true, |name| seen.insert(name)))
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
                    .filter(move |index| index.name.as_ref().map_or(true, |name| seen.insert(name)))
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
    /// The indexes will be returned in the order in which they were defined, such that the
    /// last-defined index is the last item in the vector.
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
                    .filter(move |index| index.name.as_ref().map_or(true, |name| seen.insert(name)))
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
                .filter(move |index| index.name.as_ref().map_or(true, |name| seen.insert(name)))
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
                    .filter(move |index| index.name.as_ref().map_or(true, |name| seen.insert(name)))
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
        self.implicit_indexes()
            .chain(self.default_index())
            .filter(|index| !index.explicit)
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
