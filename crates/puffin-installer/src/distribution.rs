use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};

use pep440_rs::Version;
use puffin_client::File;
use puffin_package::package_name::PackageName;
use wheel_filename::WheelFilename;

/// A built distribution (wheel), which either exists remotely or locally.
#[derive(Debug, Clone)]
pub enum Distribution {
    Remote(RemoteDistribution),
    Local(LocalDistribution),
}

impl Distribution {
    /// Return the normalized [`PackageName`] of the distribution.
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Remote(dist) => dist.name(),
            Self::Local(dist) => dist.name(),
        }
    }

    /// Return the [`Version`] of the distribution.
    pub fn version(&self) -> &Version {
        match self {
            Self::Remote(dist) => dist.version(),
            Self::Local(dist) => dist.version(),
        }
    }

    /// Return an identifier for a built distribution (wheel). The ID should be equivalent to the
    /// `.dist-info` directory name, i.e., `<distribution>-<version>.dist-info`, where
    /// `distribution` is the normalized package name with hyphens replaced by underscores.
    pub fn id(&self) -> String {
        match self {
            Self::Remote(dist) => dist.id(),
            Self::Local(dist) => dist.id(),
        }
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
        let version = Version::from_str(&filename.version).map_err(|err| anyhow!(err))?;
        Ok(Self {
            name,
            version,
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
        format!("{}-{}", self.name().replace('-', "_"), self.version())
    }
}

/// A built distribution (wheel) that exists as a local file (e.g., in the wheel cache).
#[derive(Debug, Clone)]
pub struct LocalDistribution {
    name: PackageName,
    version: Version,
    path: PathBuf,
}

impl LocalDistribution {
    /// Try to parse a cached distribution from a directory name (like `django-5.0a1`).
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

        Ok(Some(LocalDistribution {
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
        format!("{}-{}", self.name().replace('-', "_"), self.version())
    }
}
