use std::path::{Path, PathBuf};

use anyhow::Result;

use distribution_filename::WheelFilename;
use pep508_rs::VerbatimUrl;
use uv_normalize::PackageName;

use crate::direct_url::{DirectUrl, LocalFileUrl};
use crate::{
    BuiltDist, Dist, DistributionMetadata, InstalledMetadata, InstalledVersion, Name, SourceDist,
    VersionOrUrl,
};

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
    pub url: VerbatimUrl,
    pub path: PathBuf,
    pub editable: bool,
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
                editable: false,
            }),
            Dist::Built(BuiltDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Registry(_dist)) => {
                Self::Registry(CachedRegistryDist { filename, path })
            }
            Dist::Source(SourceDist::DirectUrl(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Git(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                path,
                editable: dist.editable,
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
            CachedDist::Url(dist) => {
                if dist.editable {
                    assert_eq!(dist.url.scheme(), "file", "{}", dist.url);
                    Ok(Some(DirectUrl::LocalFile(LocalFileUrl {
                        url: dist.url.raw().clone(),
                        editable: dist.editable,
                    })))
                } else {
                    DirectUrl::try_from(dist.url.raw()).map(Some)
                }
            }
        }
    }

    pub fn editable(&self) -> bool {
        match self {
            CachedDist::Registry(_) => false,
            CachedDist::Url(dist) => dist.editable,
        }
    }

    pub fn filename(&self) -> &WheelFilename {
        match self {
            CachedDist::Registry(dist) => &dist.filename,
            CachedDist::Url(dist) => &dist.filename,
        }
    }
}

impl CachedDirectUrlDist {
    /// Initialize a [`CachedDirectUrlDist`] from a [`WheelFilename`], [`url::Url`], and [`Path`].
    pub fn from_url(filename: WheelFilename, url: VerbatimUrl, path: PathBuf) -> Self {
        Self {
            filename,
            url,
            path,
            editable: false,
        }
    }
}

impl Name for CachedRegistryDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }
}

impl Name for CachedDirectUrlDist {
    fn name(&self) -> &PackageName {
        &self.filename.name
    }
}

impl Name for CachedDist {
    fn name(&self) -> &PackageName {
        match self {
            Self::Registry(dist) => dist.name(),
            Self::Url(dist) => dist.name(),
        }
    }
}

impl DistributionMetadata for CachedRegistryDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Version(&self.filename.version)
    }
}

impl DistributionMetadata for CachedDirectUrlDist {
    fn version_or_url(&self) -> VersionOrUrl {
        VersionOrUrl::Url(&self.url)
    }
}

impl DistributionMetadata for CachedDist {
    fn version_or_url(&self) -> VersionOrUrl {
        match self {
            Self::Registry(dist) => dist.version_or_url(),
            Self::Url(dist) => dist.version_or_url(),
        }
    }
}

impl InstalledMetadata for CachedRegistryDist {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Version(&self.filename.version)
    }
}

impl InstalledMetadata for CachedDirectUrlDist {
    fn installed_version(&self) -> InstalledVersion {
        InstalledVersion::Url(&self.url, &self.filename.version)
    }
}

impl InstalledMetadata for CachedDist {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Registry(dist) => dist.installed_version(),
            Self::Url(dist) => dist.installed_version(),
        }
    }
}
