use distribution_types::{CacheInfo, Hashed};
use serde::{Deserialize, Serialize};
use std::path::Path;

use pypi_types::HashDigest;

/// The [`Revision`] is a thin wrapper around a unique identifier for the source distribution.
///
/// A revision represents a unique version of a source distribution, at a level more granular than
/// (e.g.) the version number of the distribution itself. For example, a source distribution hosted
/// at a URL or a local file path may have multiple revisions, each representing a unique state of
/// the distribution, despite the reported version number remaining the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Revision {
    id: RevisionId,
    hashes: Vec<HashDigest>,
    cache_info: CacheInfo,
}

impl Revision {
    /// Initialize a new [`Revision`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self {
            id: RevisionId::new(),
            hashes: vec![],
            cache_info: CacheInfo::default(),
        }
    }

    /// Return the unique ID of the manifest.
    pub(crate) fn id(&self) -> &RevisionId {
        &self.id
    }

    pub(crate) fn cache_info(&self) -> &CacheInfo {
        &self.cache_info
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn into_hashes(self) -> Vec<HashDigest> {
        self.hashes
    }

    /// Return the computed hashes and cache info of the archive.
    pub(crate) fn into_metadata(self) -> (Vec<HashDigest>, CacheInfo) {
        (self.hashes, self.cache_info)
    }

    /// Set the computed hashes of the archive.
    #[must_use]
    pub(crate) fn with_hashes(mut self, hashes: Vec<HashDigest>) -> Self {
        self.hashes = hashes;
        self
    }

    /// Set the cache info of the archive.
    #[must_use]
    pub(crate) fn with_cache_info(mut self, cache_info: CacheInfo) -> Self {
        self.cache_info = cache_info;
        self
    }
}

impl Hashed for Revision {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}

/// A unique identifier for a revision of a source distribution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RevisionId(String);

impl RevisionId {
    /// Generate a new unique identifier for an archive.
    fn new() -> Self {
        Self(nanoid::nanoid!())
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for RevisionId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<Path> for RevisionId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
