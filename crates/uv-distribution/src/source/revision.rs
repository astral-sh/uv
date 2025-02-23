use serde::{Deserialize, Serialize};
use std::path::Path;
use uv_distribution_types::Hashed;

use uv_pypi_types::{HashDigest, HashDigests};

/// The [`Revision`] is a thin wrapper around a unique identifier for the source distribution.
///
/// A revision represents a unique version of a source distribution, at a level more granular than
/// (e.g.) the version number of the distribution itself. For example, a source distribution hosted
/// at a URL or a local file path may have multiple revisions, each representing a unique state of
/// the distribution, despite the reported version number remaining the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Revision {
    id: RevisionId,
    hashes: HashDigests,
}

impl Revision {
    /// Initialize a new [`Revision`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self {
            id: RevisionId::new(),
            hashes: HashDigests::empty(),
        }
    }

    /// Return the unique ID of the manifest.
    pub(crate) fn id(&self) -> &RevisionId {
        &self.id
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn into_hashes(self) -> HashDigests {
        self.hashes
    }

    /// Set the computed hashes of the archive.
    #[must_use]
    pub(crate) fn with_hashes(mut self, hashes: HashDigests) -> Self {
        self.hashes = hashes;
        self
    }
}

impl Hashed for Revision {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
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
