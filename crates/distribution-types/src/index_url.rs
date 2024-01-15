use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::PathBuf;
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

/// A directory with distributions or a URL to an HTML file with a flat listing of distributions.
///
/// Also known as `--find-links`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum FlatIndexLocation {
    Path(PathBuf),
    Url(Url),
}

impl FromStr for FlatIndexLocation {
    type Err = FlatIndexError;

    fn from_str(location: &str) -> Result<Self, Self::Err> {
        if location.contains("://") {
            let url =
                Url::parse(location).map_err(|err| FlatIndexError::Url(location.into(), err))?;
            if url.scheme() == "file" {
                match url.to_file_path() {
                    Ok(path_buf) => Ok(Self::Path(path_buf)),
                    Err(()) => Err(FlatIndexError::FilePath(url)),
                }
            } else {
                Ok(Self::Url(url))
            }
        } else {
            Ok(Self::Path(PathBuf::from(location)))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FlatIndexError {
    #[error("Invalid file location URL: {0}")]
    Url(String, #[source] url::ParseError),
    #[error("Invalid `file://` path in URL: {0}")]
    FilePath(Url),
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
            // TODO(charlie): Warn if the user passes in arguments here alongside `--no-index`.
            Self {
                index: None,
                extra_index: Vec::new(),
                flat_index: Vec::new(),
            }
        } else {
            Self {
                index: Some(index),
                extra_index,
                flat_index,
            }
        }
    }
}

impl<'a> IndexLocations {
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index.iter().chain(self.extra_index.iter())
    }

    pub fn flat_indexes(&'a self) -> impl Iterator<Item = &'a FlatIndexLocation> + 'a {
        self.flat_index.iter()
    }

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
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index.iter().chain(self.extra_index.iter())
    }

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
