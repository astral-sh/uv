use std::path::Path;
use std::str::FromStr;

/// A unique identifier for an archive (unzipped wheel) in the cache.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(String);

impl ArchiveId {
    /// Create a content-addressed identifier for an archive from a SHA256 digest.
    pub fn from_sha256(digest: &str) -> Self {
        Self(digest.to_string())
    }

    /// Create a random content-addressed identifier for an archive.
    pub fn nanoid() -> Self {
        Self(nanoid::nanoid!())
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
    type Err = <String as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}
