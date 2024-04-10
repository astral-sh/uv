use distribution_types::Hashed;
use serde::{Deserialize, Serialize};

use pypi_types::HashDigest;

/// The [`Revision`] is a thin wrapper around a unique identifier for the source distribution.
///
/// A revision represents a unique version of a source distribution, at a level more granular than
/// (e.g.) the version number of the distribution itself. For example, a source distribution hosted
/// at a URL or a local file path may have multiple revisions, each representing a unique state of
/// the distribution, despite the reported version number remaining the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Revision {
    id: String,
    hashes: Vec<HashDigest>,
}

impl Revision {
    /// Initialize a new [`Revision`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self {
            id: nanoid::nanoid!(),
            hashes: vec![],
        }
    }

    /// Return the unique ID of the manifest.
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }

    /// Return the computed hashes of the archive.
    pub(crate) fn into_hashes(self) -> Vec<HashDigest> {
        self.hashes
    }

    /// Set the computed hashes of the archive.
    #[must_use]
    pub(crate) fn with_hashes(mut self, hashes: Vec<HashDigest>) -> Self {
        self.hashes = hashes;
        self
    }
}

impl Hashed for Revision {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}
