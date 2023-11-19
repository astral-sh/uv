use std::ops::Deref;
use url::Url;

/// The url of an index, newtype'd to avoid mixing it with file urls
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct IndexUrl(Url);

impl From<Url> for IndexUrl {
    fn from(url: Url) -> Self {
        Self(url)
    }
}

impl From<IndexUrl> for Url {
    fn from(index: IndexUrl) -> Self {
        index.0
    }
}

impl Deref for IndexUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
