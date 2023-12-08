use std::path::{Path, PathBuf};

use url::Url;

use pypi_types::IndexUrl;

#[allow(unused_imports)] // For rustdoc
use crate::CacheBucket;
use crate::{digest, CanonicalUrl};

/// Cache wheels and their metadata, both from remote wheels and built from source distributions.
///
/// Use [`WheelCache::remote_wheel_dir`] for remote wheel metadata caching and
/// [`WheelCache::built_wheel_dir`] for built source distributions metadata caching.
pub enum WheelCache<'a> {
    /// Either pypi or an alternative index, which we key by index URL.
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
            WheelCache::Index(IndexUrl::Pypi) => PathBuf::from("pypi"),
            WheelCache::Index(url) => PathBuf::from("index").join(digest(&CanonicalUrl::new(url))),
            WheelCache::Url(url) => PathBuf::from("url").join(digest(&CanonicalUrl::new(url))),
            WheelCache::Path(url) => PathBuf::from("path").join(digest(&CanonicalUrl::new(url))),
            WheelCache::Git(url, sha) => PathBuf::from("git")
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
