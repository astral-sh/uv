use std::iter::Chain;
use std::ops::Deref;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use url::Url;

static PYPI_URL: Lazy<Url> = Lazy::new(|| Url::parse("https://pypi.org/simple").unwrap());

/// The url of an index, newtype'd to avoid mixing it with file urls.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexUrl {
    Pypi,
    Url(Url),
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

/// The index URLs to use for fetching packages.
///
/// "pip treats all package sources equally" (<https://github.com/pypa/pip/issues/8606#issuecomment-788754817>),
/// and so do we, i.e. you can't rely that on any particular order of querying indices.
///
/// If the fields are none and empty, ignore the package index, instead rely on local archives and
/// caches.
#[derive(Debug, Clone)]
pub struct IndexUrls {
    pub index: Option<IndexUrl>,
    pub extra_index: Vec<IndexUrl>,
}

impl Default for IndexUrls {
    /// Just pypi
    fn default() -> Self {
        Self {
            index: Some(IndexUrl::Pypi),
            extra_index: Vec::new(),
        }
    }
}

impl IndexUrls {
    /// Determine the index URLs to use for fetching packages.
    pub fn from_args(index: IndexUrl, extra_index: Vec<IndexUrl>, no_index: bool) -> Self {
        if no_index {
            Self {
                index: None,
                extra_index: Vec::new(),
            }
        } else {
            Self {
                index: Some(index),
                extra_index,
            }
        }
    }

    pub fn no_index(&self) -> bool {
        self.index.is_none() && self.extra_index.is_empty()
    }
}

impl<'a> IntoIterator for &'a IndexUrls {
    type Item = &'a IndexUrl;
    type IntoIter = Chain<std::option::Iter<'a, IndexUrl>, std::slice::Iter<'a, IndexUrl>>;

    fn into_iter(self) -> Self::IntoIter {
        self.index.iter().chain(self.extra_index.iter())
    }
}
