use std::path::{Path, PathBuf};

use url::Url;

use cache_key::{cache_digest, CanonicalUrl};
use distribution_types::IndexUrl;

/// Cache wheels and their metadata, both from remote wheels and built from source distributions.
#[derive(Debug, Clone)]
pub enum WheelCache<'a> {
    /// Either PyPI or an alternative index, which we key by index URL.
    Index(&'a IndexUrl),
    /// A direct URL dependency, which we key by URL.
    Url(&'a Url),
    /// A path dependency, which we key by URL.
    Path(&'a Url),
    /// An editable dependency, which we key by URL.
    Editable(&'a Url),
    /// A Git dependency, which we key by URL and SHA.
    ///
    /// Note that this variant only exists for source distributions; wheels can't be delivered
    /// through Git.
    Git(&'a Url, &'a str),
}

impl<'a> WheelCache<'a> {
    /// The root directory for a cache bucket.
    pub fn root(&self) -> PathBuf {
        match self {
            WheelCache::Index(IndexUrl::Pypi(_)) => WheelCacheKind::Pypi.root(),
            WheelCache::Index(url) => WheelCacheKind::Index
                .root()
                .join(cache_digest(&CanonicalUrl::new(url))),
            WheelCache::Url(url) => WheelCacheKind::Url
                .root()
                .join(cache_digest(&CanonicalUrl::new(url))),
            WheelCache::Path(url) => WheelCacheKind::Path
                .root()
                .join(cache_digest(&CanonicalUrl::new(url))),
            WheelCache::Editable(url) => WheelCacheKind::Editable
                .root()
                .join(cache_digest(&CanonicalUrl::new(url))),
            WheelCache::Git(url, sha) => WheelCacheKind::Git
                .root()
                .join(cache_digest(&CanonicalUrl::new(url)))
                .join(sha),
        }
    }

    /// A subdirectory in a bucket for wheels for a specific package.
    pub fn wheel_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
        self.root().join(package_name)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum WheelCacheKind {
    /// A cache of data from PyPI.
    Pypi,
    /// A cache of data from an alternative index.
    Index,
    /// A cache of data from an arbitrary URL.
    Url,
    /// A cache of data from a local path.
    Path,
    /// A cache of data from an editable URL.
    Editable,
    /// A cache of data from a Git repository.
    Git,
}

impl WheelCacheKind {
    pub(crate) fn to_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Index => "index",
            Self::Url => "url",
            Self::Path => "path",
            Self::Editable => "editable",
            Self::Git => "git",
        }
    }

    pub(crate) fn root(self) -> PathBuf {
        Path::new(self.to_str()).to_path_buf()
    }
}

impl AsRef<Path> for WheelCacheKind {
    fn as_ref(&self) -> &Path {
        self.to_str().as_ref()
    }
}
