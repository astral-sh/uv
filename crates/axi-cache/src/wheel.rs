use std::path::{Path, PathBuf};

use url::Url;

use cache_key::{digest, CanonicalUrl};
use distribution_types::IndexUrl;

#[allow(unused_imports)] // For rustdoc
use crate::CacheBucket;

/// Cache wheels and their metadata, both from remote wheels and built from source distributions.
///
/// Use [`WheelCache::remote_wheel_dir`] for remote wheel metadata caching and
/// [`WheelCache::built_wheel_dir`] for built source distributions metadata caching.
#[derive(Debug, Clone)]
pub enum WheelCache<'a> {
    /// Either PyPI or an alternative index, which we key by index URL.
    Index(&'a IndexUrl),
    /// A direct URL dependency, which we key by URL.
    Url(&'a Url),
    /// A path dependency, which we key by URL.
    Path(&'a Url),
    /// A Git dependency, which we key by URL and SHA.
    ///
    /// Note that this variant only exists for source distributions; wheels can't be delivered
    /// through Git.
    Git(&'a Url, &'a str),
}

impl<'a> WheelCache<'a> {
    fn bucket(&self) -> PathBuf {
        match self {
            WheelCache::Index(IndexUrl::Pypi) => WheelCacheKind::Pypi.root(),
            WheelCache::Index(url) => WheelCacheKind::Index
                .root()
                .join(digest(&CanonicalUrl::new(url))),
            WheelCache::Url(url) => WheelCacheKind::Url
                .root()
                .join(digest(&CanonicalUrl::new(url))),
            WheelCache::Path(url) => WheelCacheKind::Path
                .root()
                .join(digest(&CanonicalUrl::new(url))),
            WheelCache::Git(url, sha) => WheelCacheKind::Git
                .root()
                .join(digest(&CanonicalUrl::new(url)))
                .join(sha),
        }
    }

    /// Metadata of a remote wheel. See [`CacheBucket::Wheels`]
    pub fn remote_wheel_dir(&self, package_name: impl AsRef<Path>) -> PathBuf {
        self.bucket().join(package_name)
    }

    /// Metadata of a built source distribution. See [`CacheBucket::BuiltWheels`]
    pub fn built_wheel_dir(&self, filename: impl AsRef<Path>) -> PathBuf {
        self.bucket().join(filename)
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
    /// A cache of data from a Git repository.
    Git,
}

impl WheelCacheKind {
    pub(crate) fn to_str(self) -> &'static str {
        match self {
            WheelCacheKind::Pypi => "pypi",
            WheelCacheKind::Index => "index",
            WheelCacheKind::Url => "url",
            WheelCacheKind::Path => "path",
            WheelCacheKind::Git => "git",
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
