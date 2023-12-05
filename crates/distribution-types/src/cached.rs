use std::path::{Path, PathBuf};

use anyhow::Result;
use url::Url;

use distribution_filename::WheelFilename;
use puffin_normalize::PackageName;

use crate::direct_url::DirectUrl;
use crate::traits::Metadata;
use crate::{BuiltDist, Dist, SourceDist, VersionOrUrl};

/// A built distribution (wheel) that exists in the local cache.
#[derive(Debug, Clone)]
pub enum CachedDist {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(CachedRegistryDist),
    /// The distribution exists at an arbitrary URL.
    Url(CachedDirectUrlDist),
}

#[derive(Debug, Clone)]
pub struct CachedRegistryDist {
    pub filename: WheelFilename,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CachedDirectUrlDist {
    pub filename: WheelFilename,
    pub url: Url,
    pub path: PathBuf,
}

impl Metadata for CachedRegistryDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.filename.version)
    }
}

impl Metadata for CachedDirectUrlDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }

    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl Metadata for CachedDist {
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

impl CachedDist {
    /// Initialize a [`CachedDist`] from a [`Dist`].
    pub fn from_remote(remote: Dist, filename: WheelFilename, path: PathBuf) -> Self {
        match remote {
            Dist::Built(BuiltDist::Registry(_dist)) => {
                Self::Registry(CachedRegistryDist { filename, path })
            }
            Dist::Built(BuiltDist::DirectUrl(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
            }),
            Dist::Built(BuiltDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
            }),
            Dist::Source(SourceDist::Registry(_dist)) => {
                Self::Registry(CachedRegistryDist { filename, path })
            }
            Dist::Source(SourceDist::DirectUrl(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
            }),
            Dist::Source(SourceDist::Git(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
            }),
            Dist::Source(SourceDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
            }),
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
            CachedDist::Registry(_) => Ok(None),
            CachedDist::Url(dist) => DirectUrl::try_from(&dist.url).map(Some),
        }
    }
}

impl CachedDirectUrlDist {
    /// Initialize a [`CachedDirectUrlDist`] from a [`WheelFilename`], [`Url`], and [`Path`].
    pub fn from_url(filename: WheelFilename, url: Url, path: PathBuf) -> Self {
        Self {
            filename,
            url,
            path,
        }
    }
}

impl CachedRegistryDist {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn try_from_path(path: &Path) -> Result<Option<Self>> {
        let Some(file_name) = path.file_name() else {
            return Ok(None);
        };
        let Some(file_name) = file_name.to_str() else {
            return Ok(None);
        };
        let Ok(filename) = WheelFilename::from_stem(file_name) else {
            return Ok(None);
        };
        if path.is_file() {
            return Ok(None);
        }

        let path = path.to_path_buf();

        Ok(Some(Self { filename, path }))
    }
}
