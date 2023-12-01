use std::path::PathBuf;

use url::Url;

use pypi_types::IndexUrl;

#[allow(unused_imports)] // For rustdoc
use crate::CacheBucket;
use crate::{digest, CanonicalUrl};

/// Cache wheel metadata, both from remote wheels and built from source distributions.
///
/// Use [`WheelMetadataCache::wheel_dir`] for remote wheel metadata caching and
/// [`WheelMetadataCache::built_wheel_dir`] for built source distributions metadata caching.
pub enum WheelMetadataCache<'a> {
    /// Either pypi or an alternative index, which we key by index url
    Index(&'a IndexUrl),
    /// A direct url dependency, which we key by url
    Url(&'a Url),
    /// A git dependency, which we key by repository url. We use the revision as filename.
    ///
    /// Note that this variant only exists for source distributions, wheels can't be delivered
    /// through git.
    Git(&'a Url),
}

impl<'a> WheelMetadataCache<'a> {
    fn bucket(&self) -> PathBuf {
        match self {
            WheelMetadataCache::Index(IndexUrl::Pypi) => PathBuf::from("pypi"),
            WheelMetadataCache::Index(url) => {
                PathBuf::from("index").join(digest(&CanonicalUrl::new(url)))
            }
            WheelMetadataCache::Url(url) => {
                PathBuf::from("url").join(digest(&CanonicalUrl::new(url)))
            }
            WheelMetadataCache::Git(url) => {
                PathBuf::from("git").join(digest(&CanonicalUrl::new(url)))
            }
        }
    }

    /// Metadata of a remote wheel. See [`CacheBucket::WheelMetadata`]
    pub fn wheel_dir(&self) -> PathBuf {
        self.bucket()
    }

    /// Metadata of a built source distribution. See [`CacheBucket::BuiltWheels`]
    pub fn built_wheel_dir(&self, filename: &str) -> PathBuf {
        self.bucket().join(filename)
    }
}
