use std::path::PathBuf;

use distribution_types::Hashed;
use pypi_types::HashDigest;

/// An archive (unzipped wheel) that exists in the local cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Archive {
    /// The path to the archive entry in the wheel's archive bucket.
    pub path: PathBuf,
    /// The computed hashes of the archive.
    pub hashes: Vec<HashDigest>,
}

impl Archive {
    /// Create a new [`Archive`] with the given path and hashes.
    pub(crate) fn new(path: PathBuf, hashes: Vec<HashDigest>) -> Self {
        Self { path, hashes }
    }

    /// Return the path to the archive entry in the wheel's archive bucket.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Return the computed hashes of the archive.
    pub fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}

impl Hashed for Archive {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}
