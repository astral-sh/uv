use uv_cache::{ArchiveId, ArchiveVersion, LATEST};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::Hashed;
use uv_pypi_types::{HashDigest, HashDigests};

/// An archive (unzipped wheel) that exists in the local cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Archive {
    /// The unique ID of the entry in the wheel's archive bucket.
    pub id: ArchiveId,
    /// The computed hashes of the archive.
    pub hashes: HashDigests,
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The version of the archive bucket.
    pub version: ArchiveVersion,
}

impl Archive {
    /// Create a new [`Archive`] with the given blake3 digest and hashes.
    ///
    /// The archive ID is derived from the blake3 hash of the wheel's contents.
    pub(crate) fn new(blake3_digest: &str, hashes: HashDigests, filename: WheelFilename) -> Self {
        let id = ArchiveId::from_blake3(blake3_digest);
        Self {
            id,
            hashes,
            filename,
            version: LATEST,
        }
    }
}

impl Hashed for Archive {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
