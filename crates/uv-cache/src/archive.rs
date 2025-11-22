use std::convert::Infallible;
use std::path::Path;
use std::str::FromStr;

/// A unique identifier for an archive (unzipped wheel) in the cache.
///
/// Note: this remains a newtype around a [`String`] for compatibility with existing cache links
/// and serialized metadata.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(String);

impl ArchiveId {
    /// Create a content-addressed identifier for an archive from a SHA256 digest.
    pub fn from_sha256(digest: &str) -> Self {
        Self(digest.to_string())
    }

    /// Create a random identifier for an archive.
    pub fn nanoid() -> Self {
        Self(uv_fastid::Id::insecure().to_string())
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
