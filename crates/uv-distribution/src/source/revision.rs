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
///
/// Note: for compatibility with the existing `sdists-v9` bucket, this is a newtype around a
/// `String` rather than a newtype around `uv_fastid::Id`. In the future, we may want to bump
/// to `sdists-v10` and switch to using `uv_fastid::Id` directly.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RevisionId(String);

impl RevisionId {
    /// Generate a new unique identifier for an archive.
    fn new() -> Self {
        Self(uv_fastid::Id::insecure().to_string())
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

    /// Regression test for <https://github.com/astral-sh/uv/issues/19298>.
    #[test]
    fn deserialize_legacy_nanoid_revision() {
        // A representative 21-character nanoid ID, drawn from the same alphabet
        // used by both the old `nanoid` crate and `uv_fastid`.
        let legacy = Revision {
            id: RevisionId("HM0NxJml5hc7UjbfTWT1r".to_string()),
            hashes: HashDigests::empty(),
        };
        let bytes = rmp_serde::to_vec(&legacy).expect("serialize legacy revision");
        let parsed: Revision = rmp_serde::from_slice(&bytes).expect("deserialize legacy revision");
        assert_eq!(parsed.id().as_str(), "HM0NxJml5hc7UjbfTWT1r");
    }

    #[test]
    fn round_trip_current_revision() {
        let original = Revision::new();
        let bytes = rmp_serde::to_vec(&original).expect("serialize revision");
        let parsed: Revision = rmp_serde::from_slice(&bytes).expect("deserialize revision");
        assert_eq!(parsed.id().as_str(), original.id().as_str());
    }
}
