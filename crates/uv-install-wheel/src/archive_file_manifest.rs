use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The archive-metadata-local manifest that maps payloads to shared archive-file objects.
pub const ARCHIVE_FILE_MANIFEST: &str = "manifest.json";

/// A manifest for payloads stored in the content-addressed archive-file bucket.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveFileManifest {
    version: u8,
    files: Vec<ArchiveFileManifestEntry>,
}

impl ArchiveFileManifest {
    /// Create a new archive-file manifest.
    pub fn new(files: Vec<ArchiveFileManifestEntry>) -> Self {
        Self { version: 1, files }
    }

    /// Return whether the manifest contains no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Return the manifest entries.
    pub fn files(&self) -> &[ArchiveFileManifestEntry] {
        &self.files
    }

    /// Read the manifest from an archive metadata directory, if present.
    pub fn read_from_metadata(metadata: &Path) -> Result<Option<Self>, io::Error> {
        let path = metadata.join(ARCHIVE_FILE_MANIFEST);
        let contents = match fs_err::read(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let manifest = serde_json::from_slice(&contents)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        Ok(Some(manifest))
    }

    /// Write the manifest to an archive metadata directory.
    pub fn write_to_metadata(&self, metadata: &Path) -> Result<(), io::Error> {
        let path = metadata.join(ARCHIVE_FILE_MANIFEST);
        if self.is_empty() {
            match fs_err::remove_file(path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
            match fs_err::remove_dir(metadata) {
                Ok(()) => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound
                    ) => {}
                Err(err) => return Err(err),
            }
            return Ok(());
        }

        fs_err::create_dir_all(metadata)?;
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        uv_fs::write_atomic_sync(path, contents)
    }
}

/// A single archive file stored in the archive-file bucket.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveFileManifestEntry {
    path: PathBuf,
    object: PathBuf,
}

impl ArchiveFileManifestEntry {
    /// Create a new manifest entry.
    pub fn new(path: PathBuf, object: PathBuf) -> Self {
        Self { path, object }
    }

    /// Return the archive-relative path for the payload.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the archive-file-bucket-relative object path.
    pub fn object(&self) -> &Path {
        &self.object
    }
}
