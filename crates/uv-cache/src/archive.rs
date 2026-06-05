use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A unique identifier for an archive (unzipped wheel) in the cache.
///
/// Note: for compatibility with the existing `archive-v0` bucket, this is a newtype
/// around a `String` instead of a newtype around `uv_fastid::Id`. In the future,
/// we may want to bump to `archive-v1` and switch to using `uv_fastid::Id` directly.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(String);

/// A unique identifier for a file stored in the archive file bucket.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ArchiveFileId(PathBuf);

impl Default for ArchiveId {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveId {
    /// Generate a new unique identifier for an archive.
    pub(crate) fn new() -> Self {
        Self(uv_fastid::Id::secure().to_string())
    }

    /// Use a versioned, path-safe digest as the complete archive identifier.
    ///
    /// This does not generate or hash an identifier. Callers must ensure that the digest uniquely
    /// identifies the persisted directory contents.
    pub fn from_digest(digest: String) -> Self {
        Self(digest)
    }
}

impl ArchiveFileId {
    /// Generate a content-addressed identifier for an extracted file.
    ///
    /// The executable bit is part of the key because hard links share inode metadata.
    pub fn from_content_digest(digest: &str, executable: bool) -> Self {
        let mode = if executable { "executable" } else { "regular" };
        let shard = digest.get(..2).unwrap_or(digest);
        Self(PathBuf::from(mode).join(shard).join(digest))
    }
}

impl AsRef<Path> for ArchiveFileId {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ArchiveId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl std::fmt::Display for ArchiveId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ArchiveId {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}
