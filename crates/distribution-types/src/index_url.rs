use itertools::Either;
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, RwLock};
use thiserror::Error;
use url::{ParseError, Url};

use pep508_rs::{VerbatimUrl, VerbatimUrlError};

use crate::Verbatim;

static PYPI_URL: LazyLock<Url> = LazyLock::new(|| Url::parse("https://pypi.org/simple").unwrap());

static DEFAULT_INDEX_URL: LazyLock<IndexUrl> =
    LazyLock::new(|| IndexUrl::Pypi(VerbatimUrl::from_url(PYPI_URL.clone())));

/// The URL of an index to use for fetching packages (e.g., PyPI).
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
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

impl From<FlatIndexLocation> for IndexUrl {
    fn from(location: FlatIndexLocation) -> Self {
        match location {
            FlatIndexLocation::Path(url) => Self::Path(url),
            FlatIndexLocation::Url(url) => Self::Url(url),
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

/// A directory with distributions or a URL to an HTML file with a flat listing of distributions.
///
/// Also known as `--find-links`.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum FlatIndexLocation {
    Path(VerbatimUrl),
    Url(VerbatimUrl),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for FlatIndexLocation {
    fn schema_name() -> String {
        "FlatIndexLocation".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("The path to a directory of distributions, or a URL to an HTML file with a flat listing of distributions.".to_string()),
              ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

impl FlatIndexLocation {
    /// Return the raw URL for the `--find-links` index.
    pub fn url(&self) -> &Url {
        match self {
            Self::Url(url) => url.raw(),
            Self::Path(url) => url.raw(),
        }
    }

    /// Return the redacted URL for the `--find-links` index, omitting any sensitive credentials.
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

impl Display for FlatIndexLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url(url) => Display::fmt(url, f),
            Self::Path(url) => Display::fmt(url, f),
        }
    }
}

impl Verbatim for FlatIndexLocation {
    fn verbatim(&self) -> Cow<'_, str> {
        match self {
            Self::Url(url) => url.verbatim(),
            Self::Path(url) => url.verbatim(),
        }
    }
}

impl FromStr for FlatIndexLocation {
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

impl serde::ser::Serialize for FlatIndexLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> serde::de::Deserialize<'de> for FlatIndexLocation {
    fn deserialize<D>(deserializer: D) -> Result<FlatIndexLocation, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FlatIndexLocation::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl From<VerbatimUrl> for FlatIndexLocation {
    fn from(url: VerbatimUrl) -> Self {
        if url.scheme() == "file" {
            Self::Path(url)
        } else {
            Self::Url(url)
        }
    }
}

/// The index locations to use for fetching packages. By default, uses the PyPI index.
///
/// From a pip perspective, this type merges `--index-url`, `--extra-index-url`, and `--find-links`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IndexLocations {
    index: Option<IndexUrl>,
    extra_index: Vec<IndexUrl>,
    flat_index: Vec<FlatIndexLocation>,
    no_index: bool,
}

impl Default for IndexLocations {
    /// By default, use the `PyPI` index.
    fn default() -> Self {
        Self {
            index: Some(DEFAULT_INDEX_URL.clone()),
            extra_index: Vec::new(),
            flat_index: Vec::new(),
            no_index: false,
        }
    }
}

impl IndexLocations {
    /// Determine the index URLs to use for fetching packages.
    pub fn new(
        index: Option<IndexUrl>,
        extra_index: Vec<IndexUrl>,
        flat_index: Vec<FlatIndexLocation>,
        no_index: bool,
    ) -> Self {
        Self {
            index,
            extra_index,
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
    pub fn combine(
        self,
        index: Option<IndexUrl>,
        extra_index: Vec<IndexUrl>,
        flat_index: Vec<FlatIndexLocation>,
        no_index: bool,
    ) -> Self {
        Self {
            index: self.index.or(index),
            extra_index: self.extra_index.into_iter().chain(extra_index).collect(),
            flat_index: self.flat_index.into_iter().chain(flat_index).collect(),
            no_index: self.no_index || no_index,
        }
    }

    /// Returns `true` if no index configuration is set, i.e., the [`IndexLocations`] matches the
    /// default configuration.
    pub fn is_none(&self) -> bool {
        self.index.is_none()
            && self.extra_index.is_empty()
            && self.flat_index.is_empty()
            && !self.no_index
    }
}

impl<'a> IndexLocations {
    /// Return the primary [`IndexUrl`] entry.
    ///
    /// If `--no-index` is set, return `None`.
    ///
    /// If no index is provided, use the `PyPI` index.
    pub fn index(&'a self) -> Option<&'a IndexUrl> {
        if self.no_index {
            None
        } else {
            match self.index.as_ref() {
                Some(index) => Some(index),
                None => Some(&DEFAULT_INDEX_URL),
            }
        }
    }

    /// Return an iterator over the extra [`IndexUrl`] entries.
    pub fn extra_index(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            Either::Right(self.extra_index.iter())
        }
    }

    /// Return an iterator over all [`IndexUrl`] entries.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index().into_iter().chain(self.extra_index())
    }

    /// Return an iterator over the [`FlatIndexLocation`] entries.
    pub fn flat_index(&'a self) -> impl Iterator<Item = &'a FlatIndexLocation> + 'a {
        self.flat_index.iter()
    }

    /// Return the `--no-index` flag.
    pub fn no_index(&self) -> bool {
        self.no_index
    }

    /// Clone the index locations into a [`IndexUrls`] instance.
    pub fn index_urls(&'a self) -> IndexUrls {
        IndexUrls {
            index: self.index.clone(),
            extra_index: self.extra_index.clone(),
            no_index: self.no_index,
        }
    }

    /// Return an iterator over all [`Url`] entries.
    pub fn urls(&'a self) -> impl Iterator<Item = &'a Url> + 'a {
        self.indexes()
            .map(IndexUrl::url)
            .chain(self.flat_index.iter().filter_map(|index| match index {
                FlatIndexLocation::Path(_) => None,
                FlatIndexLocation::Url(url) => Some(url.raw()),
            }))
    }
}

/// The index URLs to use for fetching packages.
///
/// From a pip perspective, this type merges `--index-url` and `--extra-index-url`.
#[derive(Debug, Clone)]
pub struct IndexUrls {
    index: Option<IndexUrl>,
    extra_index: Vec<IndexUrl>,
    no_index: bool,
}

impl Default for IndexUrls {
    /// By default, use the `PyPI` index.
    fn default() -> Self {
        Self {
            index: Some(DEFAULT_INDEX_URL.clone()),
            extra_index: Vec::new(),
            no_index: false,
        }
    }
}

impl<'a> IndexUrls {
    /// Return the fallback [`IndexUrl`] entry.
    ///
    /// If `--no-index` is set, return `None`.
    ///
    /// If no index is provided, use the `PyPI` index.
    fn index(&'a self) -> Option<&'a IndexUrl> {
        if self.no_index {
            None
        } else {
            match self.index.as_ref() {
                Some(index) => Some(index),
                None => Some(&DEFAULT_INDEX_URL),
            }
        }
    }

    /// Return an iterator over the extra [`IndexUrl`] entries.
    fn extra_index(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        if self.no_index {
            Either::Left(std::iter::empty())
        } else {
            Either::Right(self.extra_index.iter())
        }
    }

    /// Return an iterator over all [`IndexUrl`] entries in order.
    ///
    /// Prioritizes the extra indexes over the main index.
    ///
    /// If `no_index` was enabled, then this always returns an empty
    /// iterator.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.extra_index().chain(self.index())
    }
}

impl From<IndexLocations> for IndexUrls {
    fn from(locations: IndexLocations) -> Self {
        Self {
            index: locations.index,
            extra_index: locations.extra_index,
            no_index: locations.no_index,
        }
    }
}

/// A map of [`IndexUrl`]s to their capabilities.
///
/// For now, we only support a single capability (range requests), and we only store an index if
/// it _doesn't_ support range requests. The benefit is that the map is almost always empty, so
/// validating capabilities is extremely cheap.
#[derive(Debug, Default, Clone)]
pub struct IndexCapabilities(Arc<RwLock<FxHashSet<IndexUrl>>>);

impl IndexCapabilities {
    /// Returns `true` if the given [`IndexUrl`] supports range requests.
    pub fn supports_range_requests(&self, index_url: &IndexUrl) -> bool {
        !self.0.read().unwrap().contains(index_url)
    }

    /// Mark an [`IndexUrl`] as not supporting range requests.
    pub fn set_supports_range_requests(&self, index_url: IndexUrl, supports: bool) {
        if supports {
            self.0.write().unwrap().remove(&index_url);
        } else {
            self.0.write().unwrap().insert(index_url);
        }
    }
}
