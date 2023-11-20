use std::path::{Path, PathBuf};

use url::Url;

use pypi_types::IndexUrl;

use crate::{digest, CanonicalUrl};

const WHEEL_METADATA_CACHE: &str = "wheel-metadata-v0";

/// Cache wheel metadata.
///
/// Wheel metadata can come from a remote wheel or from building a source
/// distribution. For a remote wheel, we try the following ways to fetch the metadata:  
/// 1. From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
/// 2. From a remote wheel by partial zip reading
/// 3. From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
pub enum WheelMetadataCache {
    Index(IndexUrl),
    Url,
}

impl WheelMetadataCache {
    /// Cache structure:
    ///  * `<wheel metadata cache>/pypi/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/<digest(index-url)>/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/url/<digest(url)>/foo-1.0.0-py3-none-any.json`
    pub fn cache_dir(&self, cache: &Path, url: &Url) -> PathBuf {
        let cache_root = cache.join(WHEEL_METADATA_CACHE);
        match self {
            WheelMetadataCache::Index(IndexUrl::Pypi) => cache_root.join("pypi"),
            WheelMetadataCache::Index(url) => cache_root
                .join("index")
                .join(digest(&CanonicalUrl::new(url))),
            WheelMetadataCache::Url => cache_root.join("url").join(digest(&CanonicalUrl::new(url))),
        }
    }
}
