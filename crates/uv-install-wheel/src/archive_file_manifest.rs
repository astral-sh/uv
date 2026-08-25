use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The archive-metadata-local manifest that maps payloads to shared archive-file objects.
const ARCHIVE_FILE_MANIFEST: &str = "manifest.json";

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

    /// Return the manifest entries.
    pub fn files(&self) -> &[ArchiveFileManifestEntry] {
        &self.files
    }

    /// Read the manifest from an archive metadata directory, validating its version and paths.
    pub fn read_from_metadata(metadata: &Path) -> Result<Option<Self>, io::Error> {
        let path = metadata.join(ARCHIVE_FILE_MANIFEST);
        let contents = match fs_err::read(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let manifest: Self = serde_json::from_slice(&contents)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if manifest.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive file manifest has an unsupported version",
            ));
        }
        for entry in &manifest.files {
            for path in [&entry.path, &entry.object] {
                if !is_relative_path(path) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "archive file manifest contains an unsafe path: {}",
                            path.display()
                        ),
                    ));
                }
            }
        }
        Ok(Some(manifest))
    }

    /// Atomically publish a nonempty manifest, or remove the sidecar when it has no entries.
    pub fn write_to_metadata(&self, metadata: &Path) -> Result<(), io::Error> {
        let path = metadata.join(ARCHIVE_FILE_MANIFEST);
        if self.files.is_empty() {
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

/// Return whether a path can be joined below a trusted root.
fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_manifest_rejects_unsafe_paths() -> anyhow::Result<()> {
        let metadata = assert_fs::TempDir::new()?;
        for field in ["path", "object"] {
            for path in ["", ".", "..", "../outside", "/outside"] {
                let mut manifest = serde_json::json!({
                    "version": 1,
                    "files": [{"path": "package/native.so", "object": "ab/abcdef"}],
                });
                manifest["files"][0][field] = path.into();
                fs_err::write(
                    metadata.join(ARCHIVE_FILE_MANIFEST),
                    serde_json::to_vec(&manifest)?,
                )?;
                let error = ArchiveFileManifest::read_from_metadata(metadata.path())
                    .expect_err("manifest paths must stay below their trusted roots");
                assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            }
        }
        Ok(())
    }
}
