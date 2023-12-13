use std::path::{Path, PathBuf};

use anyhow::Result;

use distribution_filename::WheelFilename;
use pep508_rs::VerbatimUrl;
use puffin_normalize::PackageName;

use crate::direct_url::{DirectUrl, LocalFileUrl};
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
    pub url: VerbatimUrl,
    pub path: PathBuf,
    pub editable: bool,
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
                    // TODO(konstin): Do this in the type system
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
}

impl CachedDirectUrlDist {
    /// Initialize a [`CachedDirectUrlDist`] from a [`WheelFilename`], [`Url`], and [`Path`].
    pub fn from_url(filename: WheelFilename, url: VerbatimUrl, path: PathBuf) -> Self {
        Self {
            filename,
            url,
            path,
            editable: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedWheel {
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The path to the wheel.
    pub path: PathBuf,
}

impl CachedWheel {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn from_path(path: &Path) -> Result<Option<Self>> {
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

    /// Convert a [`CachedWheel`] into a [`CachedRegistryDist`].
    pub fn into_registry_dist(self) -> CachedRegistryDist {
        CachedRegistryDist {
            filename: self.filename,
            path: self.path,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`].
    pub fn into_url_dist(self, url: VerbatimUrl) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url,
            path: self.path,
            editable: false,
        }
    }
}
