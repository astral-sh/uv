use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use fs_err::tokio as fs;

use pep440_rs::Version;
use puffin_package::package_name::PackageName;

use crate::PythonExecutable;

#[derive(Debug, Default)]
pub struct SitePackages(BTreeMap<PackageName, Distribution>);

impl SitePackages {
    /// Build an index of installed packages from the given Python executable.
    pub async fn from_executable(python: &PythonExecutable) -> Result<Self> {
        let mut index = BTreeMap::new();

        let mut dir = fs::read_dir(python.site_packages()).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(dist_info) = Distribution::try_from_path(&entry.path())? {
                    index.insert(dist_info.name().clone(), dist_info);
                }
            }
        }

        Ok(Self(index))
    }

    /// Returns an iterator over the installed packages.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &Distribution)> {
        self.0.iter()
    }

    /// Returns the version of the given package, if it is installed.
    pub fn get(&self, name: &PackageName) -> Option<&Distribution> {
        self.0.get(name)
    }

    /// Remove the given package from the index, returning its version if it was installed.
    pub fn remove(&mut self, name: &PackageName) -> Option<Distribution> {
        self.0.remove(name)
    }
}

impl IntoIterator for SitePackages {
    type Item = (PackageName, Distribution);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, Distribution>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone)]
pub struct Distribution {
    name: PackageName,
    version: Version,
    path: PathBuf,
}

impl Distribution {
    /// Try to parse a (potential) `dist-info` directory into a package name and version.
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    fn try_from_path(path: &Path) -> Result<Option<Self>> {
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

            return Ok(Some(Distribution {
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
}
