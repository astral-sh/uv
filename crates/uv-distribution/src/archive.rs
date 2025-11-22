use uv_cache::{ArchiveId, ArchiveVersion, Cache, LATEST};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::Hashed;
use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests};

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
    /// Create a new [`Archive`] with the given hashes.
    ///
    /// The archive ID is derived from the SHA256 hash in the hashes.
    pub(crate) fn new(hashes: HashDigests, filename: WheelFilename) -> Self {
        // Extract the SHA256 hash to use as the archive ID
        let sha256 = hashes
            .iter()
            .find(|digest| digest.algorithm == HashAlgorithm::Sha256)
            .expect("SHA256 hash must be present");
        let id = ArchiveId::from(sha256.clone());
        Self {
            id,
            hashes,
            filename,
            version: LATEST,
        }
    }

    /// Returns `true` if the archive exists in the cache.
    pub(crate) fn exists(&self, cache: &Cache) -> bool {
        cache.archive(&self.id, self.version).exists()
    }
}

impl Hashed for Archive {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
