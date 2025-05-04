use uv_cache::{ArchiveId, Cache, ARCHIVE_VERSION};
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
    pub version: u8,
}

impl Archive {
    /// Create a new [`Archive`] with the given ID and hashes.
    pub(crate) fn new(id: ArchiveId, hashes: HashDigests, filename: WheelFilename) -> Self {
        Self {
            id,
            hashes,
            filename,
            version: ARCHIVE_VERSION,
        }
    }

    /// Returns `true` if the archive exists in the cache.
    pub(crate) fn exists(&self, cache: &Cache) -> bool {
        self.version == ARCHIVE_VERSION && cache.archive(&self.id).exists()
    }
}

impl Hashed for Archive {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
