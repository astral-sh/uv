use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use url::Url;

use pep508_rs::split_scheme;
use uv_fs::normalize_url_path;

static PYPI_URL: Lazy<Url> = Lazy::new(|| Url::parse("https://pypi.org/simple").unwrap());

/// The url of an index, newtype'd to avoid mixing it with file urls.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexUrl {
    Pypi,
    Url(Url),
}

impl Display for IndexUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexUrl::Pypi => Display::fmt(&*PYPI_URL, f),
            IndexUrl::Url(url) => Display::fmt(url, f),
        }
    }
}

impl FromStr for IndexUrl {
    type Err = url::ParseError;

    fn from_str(url: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(Url::parse(url)?))
    }
}

impl From<Url> for IndexUrl {
    fn from(url: Url) -> Self {
        if url == *PYPI_URL {
            Self::Pypi
        } else {
            Self::Url(url)
        }
    }
}

impl From<IndexUrl> for Url {
    fn from(index: IndexUrl) -> Self {
        match index {
            IndexUrl::Pypi => PYPI_URL.clone(),
            IndexUrl::Url(url) => url,
        }
    }
}

impl Deref for IndexUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        match &self {
            IndexUrl::Pypi => &PYPI_URL,
            IndexUrl::Url(url) => url,
        }
    }
}

/// A directory with distributions or a URL to an HTML file with a flat listing of distributions.
///
/// Also known as `--find-links`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum FlatIndexLocation {
    Path(PathBuf),
    Url(Url),
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
        if let Some((scheme, path)) = split_scheme(s) {
            if scheme == "file" {
                // Ex) `file:///home/ferris/project/scripts/...` or `file:../ferris/`
                let path = path.strip_prefix("//").unwrap_or(path);

                // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                let path = normalize_url_path(path);

                let path = PathBuf::from(path.as_ref());
                Ok(Self::Path(path))
            } else {
                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                let url = Url::parse(s)?;
                Ok(Self::Url(url))
            }
        } else {
            // Ex) `../ferris/`
            let path = PathBuf::from(s);
            Ok(Self::Path(path))
        }
    }
}

impl Display for FlatIndexLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FlatIndexLocation::Path(path) => Display::fmt(&path.display(), f),
            FlatIndexLocation::Url(url) => Display::fmt(url, f),
        }
    }
}

/// The index locations to use for fetching packages.
///
/// "pip treats all package sources equally" (<https://github.com/pypa/pip/issues/8606#issuecomment-788754817>),
/// and so do we, i.e., you can't rely that on any particular order of querying indices.
///
/// If the fields are none and empty, ignore the package index, instead rely on local archives and
/// caches.
///
/// From a pip perspective, this type merges `--index-url`, `--extra-index-url`, and `--find-links`.
#[derive(Debug, Clone)]
pub struct IndexLocations {
    index: Option<IndexUrl>,
    extra_index: Vec<IndexUrl>,
    flat_index: Vec<FlatIndexLocation>,
}

impl Default for IndexLocations {
    /// By default, use the `PyPI` index.
    fn default() -> Self {
        Self {
            index: Some(IndexUrl::Pypi),
            extra_index: Vec::new(),
            flat_index: Vec::new(),
        }
    }
}

impl IndexLocations {
    /// Determine the index URLs to use for fetching packages.
    pub fn from_args(
        index: IndexUrl,
        extra_index: Vec<IndexUrl>,
        flat_index: Vec<FlatIndexLocation>,
        no_index: bool,
    ) -> Self {
        if no_index {
            Self {
                index: None,
                extra_index: Vec::new(),
                flat_index,
            }
        } else {
            Self {
                index: Some(index),
                extra_index,
                flat_index,
            }
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
        if no_index {
            Self {
                index: None,
                extra_index: Vec::new(),
                flat_index,
            }
        } else {
            Self {
                index: self.index.or(index),
                extra_index: self.extra_index.into_iter().chain(extra_index).collect(),
                flat_index: self.flat_index.into_iter().chain(flat_index).collect(),
            }
        }
    }
}

impl<'a> IndexLocations {
    /// Return an iterator over all [`IndexUrl`] entries.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index.iter().chain(self.extra_index.iter())
    }

    /// Return the primary [`IndexUrl`] entry.
    pub fn index(&'a self) -> Option<&'a IndexUrl> {
        self.index.as_ref()
    }

    /// Return an iterator over the extra [`IndexUrl`] entries.
    pub fn extra_index(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.extra_index.iter()
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
        }
    }
}

/// The index URLs to use for fetching packages.
///
/// From a pip perspective, this type merges `--index-url` and `--extra-index-url`.
#[derive(Debug, Clone)]
pub struct IndexUrls {
    index: Option<IndexUrl>,
    extra_index: Vec<IndexUrl>,
}

impl Default for IndexUrls {
    /// By default, use the `PyPI` index.
    fn default() -> Self {
        Self {
            index: Some(IndexUrl::Pypi),
            extra_index: Vec::new(),
        }
    }
}

impl<'a> IndexUrls {
    /// Return an iterator over the [`IndexUrl`] entries.
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index.iter().chain(self.extra_index.iter())
    }

    /// Return `true` if no index is configured.
    pub fn no_index(&self) -> bool {
        self.index.is_none() && self.extra_index.is_empty()
    }
}

impl From<IndexLocations> for IndexUrls {
    fn from(locations: IndexLocations) -> Self {
        Self {
            index: locations.index,
            extra_index: locations.extra_index,
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
