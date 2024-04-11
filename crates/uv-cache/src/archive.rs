use std::path::Path;

/// A unique identifier for an archive (unzipped wheel) in the cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(String);

impl Default for ArchiveId {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveId {
    /// Generate a new unique identifier for an archive.
    pub fn new() -> Self {
        Self(nanoid::nanoid!())
    }
}

impl AsRef<Path> for ArchiveId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
