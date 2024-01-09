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

/// A directory with distributions or a url to an html file with a flat listing of distributions
///
/// Also known as `--find-links`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum FlatIndexLocation {
    Path(PathBuf),
    Url(Url),
}

impl FromStr for FlatIndexLocation {
    /// We currently only use this in clap so we only need human readability
    type Err = anyhow::Error;

    fn from_str(location: &str) -> Result<Self, Self::Err> {
        if location.contains("://") {
            let url = Url::parse(location)?;
            if url.scheme() == "file" {
                if let Ok(path_buf) = url.to_file_path() {
                    return Ok(Self::Path(path_buf));
                }
                Err(anyhow::format_err!("Invalid file:// url: {url}"))
            } else {
                Ok(Self::Url(url))
            }
        } else {
            Ok(Self::Path(PathBuf::from(location)))
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
///
/// From a pip perspective, this type merges `--index-url`, `--extra-index-url` and `--find-links`.
#[derive(Debug, Clone)]
pub struct IndexLocations {
    index: Option<IndexUrl>,
    extra_index: Vec<IndexUrl>,
    flat_index: Vec<FlatIndexLocation>,
}

impl Default for IndexLocations {
    /// Just pypi
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

    pub fn no_index(&self) -> bool {
        self.index.is_none() && self.extra_index.is_empty()
    }
}

impl<'a> IndexLocations {
    pub fn indexes(&'a self) -> impl Iterator<Item = &'a IndexUrl> + 'a {
        self.index.iter().chain(self.extra_index.iter())
    }

    pub fn flat_indexes(&'a self) -> impl Iterator<Item = &'a FlatIndexLocation> + 'a {
        self.flat_index.iter()
    }
}
