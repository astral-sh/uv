use std::fmt::{Display, Formatter};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DistExtension {
    Wheel,
    Source(SourceDistExtension),
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum SourceDistExtension {
    Zip,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    TarLzma,
    Tar,
}

impl DistExtension {
    /// Extract the [`DistExtension`] from a path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ExtensionError> {
        let Some(extension) = path.as_ref().extension().and_then(|ext| ext.to_str()) else {
            return Err(ExtensionError::Dist);
        };

        match extension {
            "whl" => Ok(Self::Wheel),
            _ => SourceDistExtension::from_path(path)
                .map(Self::Source)
                .map_err(|_| ExtensionError::Dist),
        }
    }
}

impl SourceDistExtension {
    /// Extract the [`SourceDistExtension`] from a path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ExtensionError> {
        /// Returns true if the path is a tar file (e.g., `.tar.gz`).
        fn is_tar(path: &Path) -> bool {
            path.file_stem().is_some_and(|stem| {
                Path::new(stem)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
            })
        }

        let Some(extension) = path.as_ref().extension().and_then(|ext| ext.to_str()) else {
            return Err(ExtensionError::SourceDist);
        };

        match extension {
            "zip" => Ok(Self::Zip),
            "tar" => Ok(Self::Tar),
            "tgz" => Ok(Self::TarGz),
            "tbz" => Ok(Self::TarBz2),
            "txz" => Ok(Self::TarXz),
            "tlz" => Ok(Self::TarLzma),
            "gz" if is_tar(path.as_ref()) => Ok(Self::TarGz),
            "bz2" if is_tar(path.as_ref()) => Ok(Self::TarBz2),
            "xz" if is_tar(path.as_ref()) => Ok(Self::TarXz),
            "lz" | "lzma" if is_tar(path.as_ref()) => Ok(Self::TarLzma),
            "zst" if is_tar(path.as_ref()) => Ok(Self::TarZst),
            _ => Err(ExtensionError::SourceDist),
        }
    }
}

impl Display for SourceDistExtension {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zip => f.write_str("zip"),
            Self::TarGz => f.write_str("tar.gz"),
            Self::TarBz2 => f.write_str("tar.bz2"),
            Self::TarXz => f.write_str("tar.xz"),
            Self::TarZst => f.write_str("tar.zst"),
            Self::TarLzma => f.write_str("tar.lzma"),
            Self::Tar => f.write_str("tar"),
        }
    }
}

#[derive(Error, Debug)]
pub enum ExtensionError {
    #[error("`.whl`, `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`")]
    Dist,
    #[error("`.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`")]
    SourceDist,
}
