use serde::ser::SerializeMap;
use uv_cache::{ARCHIVE_VERSION, ArchiveId, Cache};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::Hashed;
use uv_pypi_types::{HashDigest, HashDigests};

/// An archive (unzipped wheel) that exists in the local cache.
#[derive(Debug, Clone, serde::Deserialize)]
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

impl serde::Serialize for Archive {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Cache buckets are shared with older uv versions, whose readers can ignore unknown map
        // entries but reject MessagePack arrays with additional fields.
        let mut map = serializer.serialize_map(Some(5))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("hashes", &self.hashes)?;
        map.serialize_entry("filename", &self.filename)?;
        map.serialize_entry("version", &self.version)?;
        map.serialize_entry("size", &self.size)?;
        map.end()
    }
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

    #[test]
    fn deserialize_uv_0_12_archive() {
        #[derive(serde::Serialize)]
        struct ReleasedArchive {
            id: ArchiveId,
            hashes: HashDigests,
            filename: WheelFilename,
            version: u8,
            size: Option<u64>,
        }

        let released = ReleasedArchive {
            id: ArchiveId::default(),
            hashes: HashDigests::empty(),
            filename: WheelFilename::from_str("iniconfig-2.0.0-py3-none-any.whl")
                .expect("valid wheel filename"),
            version: ARCHIVE_VERSION,
            size: Some(42),
        };
        let bytes = rmp_serde::to_vec(&released).expect("serialize uv 0.12 archive");
        let archive: Archive = rmp_serde::from_slice(&bytes).expect("deserialize uv 0.12 archive");

        assert_eq!(archive.size, Some(42));
    }

    #[test]
    fn serialize_archive_for_legacy_reader() {
        #[derive(serde::Deserialize)]
        struct LegacyArchive {
            id: ArchiveId,
            hashes: HashDigests,
            filename: WheelFilename,
            version: u8,
        }

        let archive = Archive::new(
            ArchiveId::default(),
            HashDigests::empty(),
            WheelFilename::from_str("iniconfig-2.0.0-py3-none-any.whl")
                .expect("valid wheel filename"),
            Some(42),
        );
        let bytes = rmp_serde::to_vec(&archive).expect("serialize archive");
        let legacy: LegacyArchive =
            rmp_serde::from_slice(&bytes).expect("deserialize archive with legacy reader");

        assert_eq!(legacy.id, archive.id);
        assert_eq!(legacy.hashes, archive.hashes);
        assert_eq!(legacy.filename, archive.filename);
        assert_eq!(legacy.version, archive.version);
    }
}
