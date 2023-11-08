use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use url::Url;

use crate::traits::BaseDistribution;
use crate::{BuiltDistribution, Distribution, SourceDistribution, VersionOrUrl};
use pep440_rs::Version;
use puffin_normalize::PackageName;

use crate::direct_url::DirectUrl;

/// A built distribution (wheel) that exists in a local cache.
#[derive(Debug, Clone)]
pub enum CachedDistribution {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(CachedRegistryDistribution),
    /// The distribution exists at an arbitrary URL.
    Url(CachedDirectUrlDistribution),
}

#[derive(Debug, Clone)]
pub struct CachedRegistryDistribution {
    pub name: PackageName,
    pub version: Version,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CachedDirectUrlDistribution {
    pub name: PackageName,
    pub url: Url,
    pub path: PathBuf,
}

impl BaseDistribution for CachedRegistryDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.version)
    }
}

impl BaseDistribution for CachedDirectUrlDistribution {
    fn name(&self) -> &PackageName {
        &self.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl BaseDistribution for CachedDistribution {
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

impl CachedDistribution {
    /// Initialize a [`CachedDistribution`] from a [`Distribution`].
    pub fn from_remote(remote: Distribution, path: PathBuf) -> Self {
        match remote {
            Distribution::Built(BuiltDistribution::Registry(dist)) => {
                Self::Registry(CachedRegistryDistribution {
                    name: dist.name,
                    version: dist.version,
                    path,
                })
            }
            Distribution::Built(BuiltDistribution::DirectUrl(dist)) => {
                Self::Url(CachedDirectUrlDistribution {
                    name: dist.name,
                    url: dist.url,
                    path,
                })
            }
            Distribution::Source(SourceDistribution::Registry(dist)) => {
                Self::Registry(CachedRegistryDistribution {
                    name: dist.name,
                    version: dist.version,
                    path,
                })
            }
            Distribution::Source(SourceDistribution::DirectUrl(dist)) => {
                Self::Url(CachedDirectUrlDistribution {
                    name: dist.name,
                    url: dist.url,
                    path,
                })
            }
            Distribution::Source(SourceDistribution::Git(dist)) => {
                Self::Url(CachedDirectUrlDistribution {
                    name: dist.name,
                    url: dist.url,
                    path,
                })
            }
        }
    }

    /// Return the [`Path`] at which the distribution is stored on-disk.
    pub fn path(&self) -> &Path {
        match self {
            Self::Registry(dist) => &dist.path,
            Self::Url(dist) => &dist.path,
        }
    }

    /// Return the [`DirectUrl`] of the distribution, if it exists.
    pub fn direct_url(&self) -> Result<Option<DirectUrl>> {
        match self {
            CachedDistribution::Registry(_) => Ok(None),
            CachedDistribution::Url(dist) => DirectUrl::try_from(&dist.url).map(Some),
        }
    }
}

impl CachedDirectUrlDistribution {
    pub fn from_url(name: PackageName, url: Url, path: PathBuf) -> Self {
        Self { name, url, path }
    }
}

impl CachedRegistryDistribution {
    /// Try to parse a distribution from a cached directory name (like `django-5.0a1`).
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
        let Some(file_name) = path.file_name() else {
            return Ok(None);
        };
        let Some(file_name) = file_name.to_str() else {
            return Ok(None);
        };
        let Some((name, version)) = file_name.rsplit_once('-') else {
            return Ok(None);
        };

        let name = PackageName::from_str(name)?;
        let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
        let path = path.to_path_buf();

        Ok(Some(Self {
            name,
            version,
            path,
        }))
    }
}
