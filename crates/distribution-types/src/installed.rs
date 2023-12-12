use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use fs_err as fs;

use pep440_rs::Version;
use puffin_normalize::PackageName;
use pypi_types::{DirectUrl, Metadata21};

use crate::{Metadata, VersionOrUrl};

/// A built distribution (wheel) that is installed in a virtual environment.
#[derive(Debug, Clone)]
pub enum InstalledDist {
    /// The distribution was derived from a registry, like `PyPI`.
    Registry(InstalledRegistryDist),
    /// The distribution was derived from an arbitrary URL.
    Url(InstalledDirectUrlDist),
}

#[derive(Debug, Clone)]
pub struct InstalledRegistryDist {
    pub name: PackageName,
    pub version: Version,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InstalledDirectUrlDist {
    pub name: PackageName,
    pub version: Version,
    pub url: DirectUrl,
    pub path: PathBuf,
}

impl Metadata for InstalledRegistryDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl Metadata for InstalledDirectUrlDist {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        // TODO(charlie): Convert a `DirectUrl` to `Url`.
        VersionOrUrl::Version(&self.version)
    }
}

impl Metadata for InstalledDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::Url(dist) => dist.name(),
        }
    }

    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::Url(dist) => dist.version_or_url(),
        }
    }
}

impl InstalledDist {
    /// Try to parse a distribution from a `.dist-info` directory name (like `django-5.0a1.dist-info`).
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
        if path.extension().is_some_and(|ext| ext == "dist-info") {
            let Some(file_stem) = path.file_stem() else {
                return Ok(None);
            };
            let Some(file_stem) = file_stem.to_str() else {
                return Ok(None);
            };
            let Some((name, version)) = file_stem.split_once('-') else {
                return Ok(None);
            };

            let name = PackageName::from_str(name)?;
            let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
            return if let Some(direct_url) = Self::direct_url(path)? {
                Ok(Some(Self::Url(InstalledDirectUrlDist {
                    name,
                    version,
                    url: direct_url,
                    path: path.to_path_buf(),
                })))
            } else {
                Ok(Some(Self::Registry(InstalledRegistryDist {
                    name,
                    version,
                    path: path.to_path_buf(),
                })))
            };
        }
        Ok(None)
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn path(&self) -> &Path {
        match self {
            Self::Registry(dist) => &dist.path,
            Self::Url(dist) => &dist.path,
        }
    }

    /// Return the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        match self {
            Self::Registry(dist) => &dist.version,
            Self::Url(dist) => &dist.version,
        }
    }

    /// Read the `direct_url.json` file from a `.dist-info` directory.
    fn direct_url(path: &Path) -> Result<Option<DirectUrl>> {
        let path = path.join("direct_url.json");
        let Ok(file) = fs_err::File::open(path) else {
            return Ok(None);
        };
        let direct_url = serde_json::from_reader::<fs_err::File, DirectUrl>(file)?;
        Ok(Some(direct_url))
    }

    /// Read the `METADATA` file from a `.dist-info` directory.
    pub fn metadata(&self) -> Result<Metadata21> {
        let path = self.path().join("METADATA");
        let contents = fs::read(&path)?;
        Metadata21::parse(&contents)
            .with_context(|| format!("Failed to parse METADATA file at: {}", path.display()))
    }
}
