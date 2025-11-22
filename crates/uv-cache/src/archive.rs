use std::path::Path;
use std::str::FromStr;

use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_small_str::SmallString;

/// The latest version of the archive bucket.
pub static LATEST: ArchiveVersion = ArchiveVersion::V0;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArchiveVersion {
    V0 = 0,
}

impl std::fmt::Display for ArchiveVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V0 => write!(f, "0"),
        }
    }
}

impl FromStr for ArchiveVersion {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Self::V0),
            _ => Err(()),
        }
    }
}

/// A unique identifier for an archive (unzipped wheel) in the cache.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(SmallString);

impl AsRef<Path> for ArchiveId {
    fn as_ref(&self) -> &Path {
        self.0.as_ref().as_ref()
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
        Ok(Self(SmallString::from(s)))
    }
}

impl From<HashDigest> for ArchiveId {
    fn from(value: HashDigest) -> Self {
        assert_eq!(
            value.algorithm,
            HashAlgorithm::Sha256,
            "Archive IDs must be created from SHA256 digests"
        );
        Self(value.digest)
    }
}
