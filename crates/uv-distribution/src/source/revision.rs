use serde::{Deserialize, Serialize};
use std::path::Path;
use uv_distribution_types::Hashed;
use uv_fastid::Id;

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
    size: Option<u64>,
}

impl Revision {
    /// Initialize a new [`Revision`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self {
            id: RevisionId::new(),
            hashes: HashDigests::empty(),
            size: None,
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

    /// Return the size of the downloaded archive.
    pub(crate) fn size(&self) -> Option<u64> {
        self.size
    }

    /// Set the computed hashes of the archive.
    #[must_use]
    pub(crate) fn with_hashes(mut self, hashes: HashDigests) -> Self {
        self.hashes = hashes;
        self
    }

    /// Set the size of the downloaded archive.
    #[must_use]
    pub(crate) fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}

impl Hashed for Revision {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}

/// A unique identifier for a revision of a source distribution.
///
/// The identifier is serialized as a string in the source-distribution cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RevisionId(Id);

impl RevisionId {
    /// Generate a new unique identifier for a source distribution revision.
    fn new() -> Self {
        Self(Id::secure())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_current_revision() {
        for size in [None, Some(42)] {
            let mut original = Revision::new().with_hashes(HashDigests::from(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                    .parse::<HashDigest>()
                    .expect("valid SHA-256 digest"),
            ));
            if let Some(size) = size {
                original = original.with_size(size);
            }
            let bytes = rmp_serde::to_vec(&original).expect("serialize revision");
            let parsed: Revision = rmp_serde::from_slice(&bytes).expect("deserialize revision");
            assert_eq!(parsed.id().as_str(), original.id().as_str());
            assert_eq!(parsed.hashes(), original.hashes());
            assert_eq!(parsed.size(), original.size());
        }
    }

    #[test]
    fn serialize_revision_id_as_string() {
        #[derive(Deserialize)]
        struct SerializedRevision {
            id: String,
            hashes: HashDigests,
            size: Option<u64>,
        }

        let revision = Revision::new().with_size(42);
        let bytes = rmp_serde::to_vec(&revision).expect("serialize revision");
        let serialized: SerializedRevision =
            rmp_serde::from_slice(&bytes).expect("deserialize revision with string ID");

        assert_eq!(serialized.id.as_str(), revision.id().as_str());
        assert_eq!(serialized.hashes, HashDigests::empty());
        assert_eq!(serialized.size, Some(42));
    }
}
