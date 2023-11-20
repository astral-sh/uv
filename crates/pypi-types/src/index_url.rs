use once_cell::sync::Lazy;
use std::ops::Deref;
use url::Url;

static PYPI_URL: Lazy<Url> = Lazy::new(|| Url::parse("https://pypi.org/simple").unwrap());

/// The url of an index, newtype'd to avoid mixing it with file urls
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum IndexUrl {
    Pypi,
    Url(Url),
}

impl From<Url> for IndexUrl {
    fn from(url: Url) -> Self {
        Self::Url(url)
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
