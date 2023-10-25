use std::collections::BTreeMap;

use anyhow::Result;
use fs_err as fs;
use puffin_interpreter::Virtualenv;

use puffin_package::package_name::PackageName;

use crate::InstalledDistribution;

#[derive(Debug, Default)]
pub struct SitePackages(BTreeMap<PackageName, InstalledDistribution>);

impl SitePackages {
    /// Build an index of installed packages from the given Python executable.
    pub fn try_from_executable(venv: &Virtualenv) -> Result<Self> {
        let mut index = BTreeMap::new();

        for entry in fs::read_dir(venv.site_packages())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(dist_info) = InstalledDistribution::try_from_path(&entry.path())? {
                    index.insert(dist_info.name().clone(), dist_info);
                }
            }
        }

        Ok(Self(index))
    }

    /// Returns an iterator over the installed packages.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &InstalledDistribution)> {
        self.0.iter()
    }

    /// Returns the version of the given package, if it is installed.
    pub fn get(&self, name: &PackageName) -> Option<&InstalledDistribution> {
        self.0.get(name)
    }

    /// Remove the given package from the index, returning its version if it was installed.
    pub fn remove(&mut self, name: &PackageName) -> Option<InstalledDistribution> {
        self.0.remove(name)
    }
}

impl IntoIterator for SitePackages {
    type Item = (PackageName, InstalledDistribution);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, InstalledDistribution>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
