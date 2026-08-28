use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path};

/// Executable paths in a cached wheel whose files have no executable permissions.
///
/// The manifest is stored outside the wheel and participates in its archive identity.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ArchiveManifest {
    version: u8,
    executables: BTreeSet<String>,
}

impl ArchiveManifest {
    /// Record executable paths in portable, sorted order for deterministic serialization.
    pub fn new(executables: impl IntoIterator<Item = String>) -> Self {
        Self {
            version: 1,
            executables: executables.into_iter().collect(),
        }
    }

    /// Serialize the metadata that must be hashed and persisted with the archive.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(io::Error::other)
    }

    /// Read a cached archive's manifest, if present, retaining filesystem permissions otherwise.
    pub(crate) fn read(wheel: &Path, manifests: Option<&Path>) -> io::Result<Option<Self>> {
        let (Some(id), Some(manifests)) = (wheel.file_name(), manifests) else {
            return Ok(None);
        };
        let contents = match fs_err::read(manifests.join(id).join("manifest.json")) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let manifest: Self = serde_json::from_slice(&contents)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if manifest.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "wheel archive manifest has an unsupported version",
            ));
        }
        for path in &manifest.executables {
            if path.is_empty()
                || !Path::new(path)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "wheel archive manifest contains an unsafe executable path",
                ));
            }
        }
        Ok(Some(manifest))
    }

    /// Return paths whose installed copies need executable permissions.
    pub(crate) fn executable_paths(&self) -> impl Iterator<Item = &Path> {
        self.executables.iter().map(Path::new)
    }
}
