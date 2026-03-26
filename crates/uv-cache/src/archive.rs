use std::convert::Infallible;
use std::path::Path;
use std::str::FromStr;

use uv_small_str::SmallString;

/// The latest version of the archive bucket.
pub static LATEST: ArchiveVersion = ArchiveVersion::V0;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum ArchiveVersion {
    V0 = 0,
}

impl From<ArchiveVersion> for u8 {
    fn from(version: ArchiveVersion) -> Self {
        match version {
            ArchiveVersion::V0 => 0,
        }
    }
}

impl TryFrom<u8> for ArchiveVersion {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::V0),
            _ => Err(value),
        }
    }
}

impl serde::Serialize for ArchiveVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8((*self).into())
    }
}

impl<'de> serde::Deserialize<'de> for ArchiveVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <u8 as serde::Deserialize>::deserialize(deserializer)?;
        Self::try_from(value).map_err(|value| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Unsigned(u64::from(value)),
                &"a valid archive version",
            )
        })
    }
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
///
/// Derived from the blake3 hash of the wheel's contents and stored as a [`SmallString`].
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ArchiveId(SmallString);

impl ArchiveId {
    /// Create a new [`ArchiveId`] from a blake3 hex digest.
    pub fn from_blake3(digest: &str) -> Self {
        Self(SmallString::from(digest))
    }
}

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
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(SmallString::from(s)))
    }
}
