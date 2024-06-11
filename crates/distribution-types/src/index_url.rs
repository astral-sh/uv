use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use itertools::Either;
use once_cell::sync::Lazy;
use url::Url;

use pep508_rs::{expand_env_vars, split_scheme, strip_host, Scheme, VerbatimUrl};
use uv_fs::normalize_url_path;

use crate::Verbatim;

static PYPI_URL: Lazy<Url> = Lazy::new(|| Url::parse("https://pypi.org/simple").unwrap());

static DEFAULT_INDEX_URL: Lazy<IndexUrl> =
    Lazy::new(|| IndexUrl::Pypi(VerbatimUrl::from_url(PYPI_URL.clone())));

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
            format: Some("uri".to_owned()),
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

impl FromStr for IndexUrl {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(s)?;
        let url = VerbatimUrl::from_url(url).with_given(s.to_owned());
        if *url.raw() == *PYPI_URL {
            Ok(Self::Pypi(url))
        } else if url.scheme() == "file" {
            Ok(Self::Path(url))
        } else {
            Ok(Self::Url(url))
        }
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
        if *url.raw() == *PYPI_URL {
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
    Path(PathBuf),
    Url(Url),
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for FlatIndexLocation {
    fn schema_name() -> String {
        "FlatIndexLocation".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            format: Some("uri".to_owned()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("The path to a directory of distributions, or a URL to an HTML file with a flat listing of distributions.".to_string()),
              ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
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

impl FromStr for FlatIndexLocation {
    type Err = url::ParseError;

    /// Parse a raw string for a `--find-links` entry, which could be a URL or a local path.
    ///
    /// For example:
    /// - `file:///home/ferris/project/scripts/...`
    /// - `file:../ferris/`
    /// - `../ferris/`
    /// - `https://download.pytorch.org/whl/torch_stable.html`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Expand environment variables.
        let expanded = expand_env_vars(s);

        // Parse the expanded path.
        if let Some((scheme, path)) = split_scheme(&expanded) {
            match Scheme::parse(scheme) {
                // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
                Some(Scheme::File) => {
                    // Strip the leading slashes, along with the `localhost` host, if present.
                    let path = strip_host(path);

                    // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                    let path = normalize_url_path(path);

                    let path = PathBuf::from(path.as_ref());
                    Ok(Self::Path(path))
                }

                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                Some(_) => {
                    let url = Url::parse(expanded.as_ref())?;
                    Ok(Self::Url(url))
                }

                // Ex) `C:\Users\ferris\wheel-0.42.0.tar.gz`
                None => {
                    let path = PathBuf::from(expanded.as_ref());
                    Ok(Self::Path(path))
                }
            }
        } else {
            // Ex) `../ferris/`
            let path = PathBuf::from(expanded.as_ref());
            Ok(Self::Path(path))
        }
    }
}

impl Display for FlatIndexLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(path) => Display::fmt(&path.display(), f),
            Self::Url(url) => Display::fmt(url, f),
        }
    }
}

/// The index locations to use for fetching packages. By default, uses the PyPI index.
///
/// From a pip perspective, this type merges `--index-url`, `--extra-index-url`, and `--find-links`.
#[derive(Debug, Clone)]
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
                FlatIndexLocation::Url(url) => Some(url),
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

#[cfg(test)]
#[cfg(unix)]
mod test {
    use super::*;

    #[test]
    fn parse_find_links() {
        assert_eq!(
            FlatIndexLocation::from_str("file:///home/ferris/project/scripts/...").unwrap(),
            FlatIndexLocation::Path(PathBuf::from("/home/ferris/project/scripts/..."))
        );
        assert_eq!(
            FlatIndexLocation::from_str("file:../ferris/").unwrap(),
            FlatIndexLocation::Path(PathBuf::from("../ferris/"))
        );
        assert_eq!(
            FlatIndexLocation::from_str("../ferris/").unwrap(),
            FlatIndexLocation::Path(PathBuf::from("../ferris/"))
        );
        assert_eq!(
            FlatIndexLocation::from_str("https://download.pytorch.org/whl/torch_stable.html")
                .unwrap(),
            FlatIndexLocation::Url(
                Url::parse("https://download.pytorch.org/whl/torch_stable.html").unwrap()
            )
        );
    }
}
