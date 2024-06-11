use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use distribution_filename::WheelFilename;
use pep508_rs::VerbatimUrl;
use pypi_types::{HashDigest, ParsedPathUrl};
use uv_normalize::PackageName;

use crate::{
    BuiltDist, Dist, DistributionMetadata, Hashed, InstalledMetadata, InstalledVersion, Name,
    ParsedUrl, SourceDist, VersionOrUrlRef,
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
    pub hashes: Vec<HashDigest>,
}

#[derive(Debug, Clone)]
pub struct CachedDirectUrlDist {
    pub filename: WheelFilename,
    pub url: VerbatimUrl,
    pub path: PathBuf,
    pub editable: bool,
    pub hashes: Vec<HashDigest>,
}

impl CachedDist {
    /// Initialize a [`CachedDist`] from a [`Dist`].
    pub fn from_remote(
        remote: Dist,
        filename: WheelFilename,
        hashes: Vec<HashDigest>,
        path: PathBuf,
    ) -> Self {
        match remote {
            Dist::Built(BuiltDist::Registry(_dist)) => Self::Registry(CachedRegistryDist {
                filename,
                path,
                hashes,
            }),
            Dist::Built(BuiltDist::DirectUrl(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
                path,
                editable: false,
            }),
            Dist::Built(BuiltDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Registry(_dist)) => Self::Registry(CachedRegistryDist {
                filename,
                path,
                hashes,
            }),
            Dist::Source(SourceDist::DirectUrl(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Git(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Path(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
                path,
                editable: false,
            }),
            Dist::Source(SourceDist::Directory(dist)) => Self::Url(CachedDirectUrlDist {
                filename,
                url: dist.url,
                hashes,
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

    /// Return the [`ParsedUrl`] of the distribution, if it exists.
    pub fn parsed_url(&self) -> Result<Option<ParsedUrl>> {
        match self {
            Self::Registry(_) => Ok(None),
            Self::Url(dist) => {
                if dist.editable {
                    assert_eq!(dist.url.scheme(), "file", "{}", dist.url);
                    let path = dist
                        .url
                        .to_file_path()
                        .map_err(|()| anyhow!("Invalid path in file URL"))?;
                    Ok(Some(ParsedUrl::Path(ParsedPathUrl {
                        url: dist.url.raw().clone(),
                        install_path: path.clone(),
                        lock_path: path,
                        editable: dist.editable,
                    })))
                } else {
                    Ok(Some(ParsedUrl::try_from(dist.url.to_url())?))
                }
            }
        }
    }

    /// Returns the [`WheelFilename`] of the distribution.
    pub fn filename(&self) -> &WheelFilename {
        match self {
            Self::Registry(dist) => &dist.filename,
            Self::Url(dist) => &dist.filename,
        }
    }
}

impl Hashed for CachedRegistryDist {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}

impl CachedDirectUrlDist {
    /// Initialize a [`CachedDirectUrlDist`] from a [`WheelFilename`], [`url::Url`], and [`Path`].
    pub fn from_url(
        filename: WheelFilename,
        url: VerbatimUrl,
        hashes: Vec<HashDigest>,
        path: PathBuf,
    ) -> Self {
        Self {
            filename,
            url,
            hashes,
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
    fn version_or_url(&self) -> VersionOrUrlRef {
        VersionOrUrlRef::Version(&self.filename.version)
    }
}

impl DistributionMetadata for CachedDirectUrlDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
        VersionOrUrlRef::Url(&self.url)
    }
}

impl DistributionMetadata for CachedDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
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
