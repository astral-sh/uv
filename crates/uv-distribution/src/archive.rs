use uv_cache::{ARCHIVE_VERSION, ArchiveId, Cache};
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
    /// The size of the downloaded archive.
    #[serde(default)]
    pub size: Option<u64>,
}

impl Archive {
    /// Create a new [`Archive`] with the given ID and hashes.
    pub(crate) fn new(
        id: ArchiveId,
        hashes: HashDigests,
        filename: WheelFilename,
        size: Option<u64>,
    ) -> Self {
        Self {
            id,
            hashes,
            filename,
            version: ARCHIVE_VERSION,
            size,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn deserialize_legacy_archive() {
        #[derive(serde::Serialize)]
        struct LegacyArchive {
            id: ArchiveId,
            hashes: HashDigests,
            filename: WheelFilename,
            version: u8,
        }

        let legacy = LegacyArchive {
            id: ArchiveId::default(),
            hashes: HashDigests::empty(),
            filename: WheelFilename::from_str("iniconfig-2.0.0-py3-none-any.whl")
                .expect("valid wheel filename"),
            version: ARCHIVE_VERSION,
        };
        let bytes = rmp_serde::to_vec(&legacy).expect("serialize legacy archive");
        let archive: Archive = rmp_serde::from_slice(&bytes).expect("deserialize legacy archive");

        assert_eq!(archive.size, None);
    }
}
