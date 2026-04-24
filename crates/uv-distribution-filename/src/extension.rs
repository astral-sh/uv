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
#[rkyv(derive(Debug))]
pub enum SourceDistExtension {
    TarGz,
    Legacy(LegacySourceDistExtension),
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
#[rkyv(derive(Debug))]
pub enum LegacySourceDistExtension {
    Tar,
    TarBz2,
    TarLz,
    TarLzma,
    TarXz,
    TarZst,
    Tbz,
    Tgz,
    Tlz,
    Txz,
    Zip,
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

    /// Return the name for the extension.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Wheel => "whl",
            Self::Source(ext) => ext.name(),
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
            "gz" if is_tar(path.as_ref()) => Ok(Self::TarGz),
            "zip" => Ok(Self::Legacy(LegacySourceDistExtension::Zip)),
            "tar" => Ok(Self::Legacy(LegacySourceDistExtension::Tar)),
            "tgz" => Ok(Self::Legacy(LegacySourceDistExtension::Tgz)),
            "tbz" => Ok(Self::Legacy(LegacySourceDistExtension::Tbz)),
            "txz" => Ok(Self::Legacy(LegacySourceDistExtension::Txz)),
            "tlz" => Ok(Self::Legacy(LegacySourceDistExtension::Tlz)),
            "bz2" if is_tar(path.as_ref()) => Ok(Self::Legacy(LegacySourceDistExtension::TarBz2)),
            "xz" if is_tar(path.as_ref()) => Ok(Self::Legacy(LegacySourceDistExtension::TarXz)),
            "lz" if is_tar(path.as_ref()) => Ok(Self::Legacy(LegacySourceDistExtension::TarLz)),
            "lzma" if is_tar(path.as_ref()) => Ok(Self::Legacy(LegacySourceDistExtension::TarLzma)),
            "zst" if is_tar(path.as_ref()) => Ok(Self::Legacy(LegacySourceDistExtension::TarZst)),
            _ => Err(ExtensionError::SourceDist),
        }
    }

    /// Return the name for the extension.
    pub fn name(&self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Legacy(LegacySourceDistExtension::Tar) => "tar",
            Self::Legacy(LegacySourceDistExtension::TarBz2) => "tar.bz2",
            Self::Legacy(LegacySourceDistExtension::TarLz) => "tar.lz",
            Self::Legacy(LegacySourceDistExtension::TarLzma) => "tar.lzma",
            Self::Legacy(LegacySourceDistExtension::TarXz) => "tar.xz",
            Self::Legacy(LegacySourceDistExtension::TarZst) => "tar.zst",
            Self::Legacy(LegacySourceDistExtension::Tbz) => "tbz",
            Self::Legacy(LegacySourceDistExtension::Tgz) => "tgz",
            Self::Legacy(LegacySourceDistExtension::Tlz) => "tlz",
            Self::Legacy(LegacySourceDistExtension::Txz) => "txz",
            Self::Legacy(LegacySourceDistExtension::Zip) => "zip",
        }
    }

    /// Returns `true` if the extension conforms to [PEP 625](https://peps.python.org/pep-0625/)'s
    /// naming requirements.
    ///
    /// PEP 625 mandates `.tar.gz`; `.zip` is also accepted for backwards compatibility.
    pub fn is_pep625_compliant(&self) -> bool {
        matches!(
            self,
            Self::TarGz | Self::Legacy(LegacySourceDistExtension::Zip)
        )
    }
}

impl Display for SourceDistExtension {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Error, Debug)]
pub enum ExtensionError {
    #[error(
        "`.whl`, `.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`"
    )]
    Dist,
    #[error(
        "`.tar.gz`, `.zip`, `.tar.bz2`, `.tar.lz`, `.tar.lzma`, `.tar.xz`, `.tar.zst`, `.tar`, `.tbz`, `.tgz`, `.tlz`, or `.txz`"
    )]
    SourceDist,
}
