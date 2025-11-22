use std::path::PathBuf;
use std::str::FromStr;

use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_small_str::SmallString;

/// The latest version of the archive bucket.
pub static LATEST: ArchiveVersion = ArchiveVersion::V1;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArchiveVersion {
    V0 = 0,
    V1 = 1,
}

impl std::fmt::Display for ArchiveVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V0 => write!(f, "0"),
            Self::V1 => write!(f, "1"),
        }
    }
}

impl FromStr for ArchiveVersion {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Self::V0),
            "1" => Ok(Self::V1),
            _ => Err(()),
        }
    }
}

/// A unique identifier for an archive (unzipped wheel) in the cache.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(SmallString);

impl ArchiveId {
    /// Return the content-addressed path for the [`ArchiveId`].
    pub fn to_path_buf(&self, version: ArchiveVersion) -> PathBuf {
        match version {
            // Version 0: A 21-digit NanoID.
            ArchiveVersion::V0 => PathBuf::from(self.0.as_ref()),
            // Version 1: A SHA256 hex digest, split into three segments.
            ArchiveVersion::V1 => {
                let mut path = PathBuf::new();
                path.push(&self.0[0..2]);
                path.push(&self.0[2..4]);
                path.push(&self.0[4..]);
                path
            }
        }
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
