//! Cache all the wheel metadata cases:
//! * Metadata we got from a remote wheel
//!   * From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
//!   * From a remote wheel by partial zip reading
//!   * From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
//! * Metadata we got from building a source dist, keyed by the wheel name since we can have multiple wheels per source dist (e.g. for python version specific builds)

use std::path::{Path, PathBuf};

use url::Url;

use pypi_types::IndexUrl;

use crate::{digest, CanonicalUrl};

const WHEEL_METADATA_CACHE: &str = "wheel-metadata-v0";

pub enum WheelMetadataCacheShard {
    Pypi,
    Index(IndexUrl),
    Url,
}

impl WheelMetadataCacheShard {
    /// Cache structure:
    ///  * `<wheel metadata cache>/pypi/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/<sha256(index-url)>/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/url/<sha256(url)>/foo-1.0.0-py3-none-any.json`
    pub fn cache_dir(&self, cache: &Path, url: &Url) -> PathBuf {
        let cache_root = cache.join(WHEEL_METADATA_CACHE);
        match self {
            WheelMetadataCacheShard::Pypi => cache_root.join("pypi"),
            WheelMetadataCacheShard::Index(_) => cache_root
                .join("index")
                .join(digest(&CanonicalUrl::new(url))),
            WheelMetadataCacheShard::Url => {
                cache_root.join("url").join(digest(&CanonicalUrl::new(url)))
            }
        }
    }
}
