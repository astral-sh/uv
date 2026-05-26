use std::path::Path;

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

    /// Returns `true` if the archive exists in the cache and is not corrupted.
    pub(crate) fn exists(&self, cache: &Cache) -> bool {
        if self.version != ARCHIVE_VERSION {
            return false;
        }
        let path = cache.archive(&self.id);
        if !path.is_dir() {
            return false;
        }
        // Validate the archive is not corrupted. A power outage during
        // `Cache::persist()` can leave the directory intact but with 0-byte
        // files. Check that the archive contains a non-empty METADATA file in
        // a `.dist-info` directory.
        has_non_empty_metadata(&path)
    }
}

/// Returns `true` if the given path contains a `.dist-info/METADATA` file
/// with non-zero size.
///
/// This validates that an unzipped wheel archive is not corrupted (e.g.,
/// from a power outage during `Cache::persist()` that leaves 0-byte files).
fn has_non_empty_metadata(path: &Path) -> bool {
    let Ok(entries) = fs_err::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.ends_with(".dist-info") {
            continue;
        }
        let metadata_file = entry.path().join("METADATA");
        if let Ok(meta) = metadata_file.metadata() {
            if meta.is_file() && meta.len() > 0 {
                return true;
            }
        }
    }
    false
}

impl Hashed for Archive {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_non_empty_metadata_valid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dist_info = temp_dir.path().join("foo-1.0.dist-info");
        fs_err::create_dir(&dist_info).unwrap();
        fs_err::write(dist_info.join("METADATA"), "Name: foo\nVersion: 1.0\n").unwrap();
        assert!(has_non_empty_metadata(temp_dir.path()));
    }

    #[test]
    fn test_has_non_empty_metadata_empty_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dist_info = temp_dir.path().join("foo-1.0.dist-info");
        fs_err::create_dir(&dist_info).unwrap();
        fs_err::write(dist_info.join("METADATA"), "").unwrap();
        assert!(!has_non_empty_metadata(temp_dir.path()));
    }

    #[test]
    fn test_has_non_empty_metadata_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert!(!has_non_empty_metadata(temp_dir.path()));
    }

    #[test]
    fn test_has_non_empty_metadata_no_dist_info() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs_err::write(temp_dir.path().join("some_file.txt"), "content").unwrap();
        assert!(!has_non_empty_metadata(temp_dir.path()));
    }

    #[test]
    fn test_has_non_empty_metadata_dist_info_no_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dist_info = temp_dir.path().join("foo-1.0.dist-info");
        fs_err::create_dir(&dist_info).unwrap();
        fs_err::write(dist_info.join("WHEEL"), "Wheel-Version: 1.0\n").unwrap();
        assert!(!has_non_empty_metadata(temp_dir.path()));
    }
}
