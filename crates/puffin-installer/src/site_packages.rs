use std::collections::BTreeMap;

use anyhow::{Context, Result};
use fs_err as fs;

use distribution_types::{InstalledDist, Metadata};
use puffin_interpreter::Virtualenv;
use puffin_normalize::PackageName;

#[derive(Debug, Default)]
pub struct SitePackages(BTreeMap<PackageName, InstalledDist>);

impl SitePackages {
    /// Build an index of installed packages from the given Python executable.
    pub fn try_from_executable(venv: &Virtualenv) -> Result<Self> {
        let mut index = BTreeMap::new();

        for entry in fs::read_dir(venv.site_packages())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(dist_info) =
                    InstalledDist::try_from_path(&entry.path()).with_context(|| {
                        format!("Failed to read metadata: from {}", entry.path().display())
                    })?
                {
                    index.insert(dist_info.name().clone(), dist_info);
                }
            }
        }

        Ok(Self(index))
    }

    /// Returns an iterator over the installed distributions.
    pub fn distributions(&self) -> impl Iterator<Item = &InstalledDist> {
        self.0.values()
    }

    /// Returns the version of the given package, if it is installed.
    pub fn get(&self, name: &PackageName) -> Option<&InstalledDist> {
        self.0.get(name)
    }

    /// Remove the given package from the index, returning its version if it was installed.
    pub fn remove(&mut self, name: &PackageName) -> Option<InstalledDist> {
        self.0.remove(name)
    }

    /// Returns `true` if there are no installed packages.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of installed packages.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl IntoIterator for SitePackages {
    type Item = (PackageName, InstalledDist);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, InstalledDist>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
