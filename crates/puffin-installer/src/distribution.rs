use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};

use distribution_filename::WheelFilename;
use pep440_rs::Version;
use puffin_client::File;
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;

/// A built distribution (wheel), which either exists remotely or locally.
#[derive(Debug, Clone)]
pub enum Distribution {
    Remote(RemoteDistribution),
    Cached(CachedDistribution),
    Installed(InstalledDistribution),
}

impl Distribution {
    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Remote(dist) => dist.name(),
            Self::Cached(dist) => dist.name(),
            Self::Installed(dist) => dist.name(),
        }
    }

    /// Return the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        match self {
            Self::Remote(dist) => dist.version(),
            Self::Cached(dist) => dist.version(),
            Self::Installed(dist) => dist.version(),
        }
    }

    /// Return an identifier for a built distribution (wheel). The ID should be equivalent to the
    /// `.dist-info` directory name, i.e., `<distribution>-<version>.dist-info`, where
    /// `distribution` is the normalized package name with hyphens replaced by underscores.
    pub fn id(&self) -> String {
        match self {
            Self::Remote(dist) => dist.id(),
            Self::Cached(dist) => dist.id(),
            Self::Installed(dist) => dist.id(),
        }
    }
}

impl From<RemoteDistribution> for Distribution {
    fn from(dist: RemoteDistribution) -> Self {
        Self::Remote(dist)
    }
}

impl From<CachedDistribution> for Distribution {
    fn from(dist: CachedDistribution) -> Self {
        Self::Cached(dist)
    }
}

impl From<InstalledDistribution> for Distribution {
    fn from(dist: InstalledDistribution) -> Self {
        Self::Installed(dist)
    }
}

/// A built distribution (wheel) that exists as a remote file (e.g., on `PyPI`).
#[derive(Debug, Clone)]
pub struct RemoteDistribution {
    name: PackageName,
    version: Version,
    file: File,
}

impl RemoteDistribution {
    /// Try to parse a remote distribution from a remote file (like `django-5.0a1-py3-none-any.whl`).
    pub fn from_file(file: File) -> Result<Self> {
        let filename = WheelFilename::from_str(&file.filename)?;
        let name = PackageName::normalize(&filename.distribution);
        Ok(Self {
            name,
            version: filename.version.clone(),
            file,
        })
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn file(&self) -> &File {
        &self.file
    }

    pub fn id(&self) -> String {
        format!("{}-{}", DistInfoName::from(self.name()), self.version())
    }
}

/// A built distribution (wheel) that exists in a local cache.
#[derive(Debug, Clone)]
pub struct CachedDistribution {
    name: PackageName,
    version: Version,
    path: PathBuf,
}

impl CachedDistribution {
    /// Initialize a new cached distribution.
    pub fn new(name: PackageName, version: Version, path: PathBuf) -> Self {
        Self {
            name,
            version,
            path,
        }
    }

    /// Try to parse a distribution from a cached directory name (like `django-5.0a1`).
    pub(crate) fn try_from_path(path: &Path) -> Result<Option<Self>> {
        let Some(file_name) = path.file_name() else {
            return Ok(None);
        };
        let Some(file_name) = file_name.to_str() else {
            return Ok(None);
        };
        let Some((name, version)) = file_name.split_once('-') else {
            return Ok(None);
        };

        let name = PackageName::normalize(name);
        let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
        let path = path.to_path_buf();

        Ok(Some(CachedDistribution {
            name,
            version,
            path,
        }))
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn id(&self) -> String {
        format!("{}-{}", DistInfoName::from(self.name()), self.version())
    }
}

/// A built distribution (wheel) that exists in a virtual environment.
#[derive(Debug, Clone)]
pub struct InstalledDistribution {
    name: PackageName,
    version: Version,
    path: PathBuf,
}

impl InstalledDistribution {
    /// Initialize a new installed distribution.
    pub fn new(name: PackageName, version: Version, path: PathBuf) -> Self {
        Self {
            name,
            version,
            path,
        }
    }

    /// Try to parse a distribution from a `.dist-info` directory name (like `django-5.0a1.dist-info`).
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub(crate) fn try_from_path(path: &Path) -> Result<Option<Self>> {
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

            let name = PackageName::normalize(name);
            let version = Version::from_str(version).map_err(|err| anyhow!(err))?;
            let path = path.to_path_buf();

            return Ok(Some(Self {
                name,
                version,
                path,
            }));
        }

        Ok(None)
    }

    pub fn name(&self) -> &PackageName {
        &self.name
    }

    pub fn version(&self) -> &Version {
        &self.version
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn id(&self) -> String {
        format!("{}-{}", DistInfoName::from(self.name()), self.version())
    }
}
