use std::collections::HashMap;

use pep440_rs::Version;
use puffin_client::File;
use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;

#[derive(Debug, Default)]
pub struct Resolution(HashMap<PackageName, PinnedPackage>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub(crate) fn new(packages: HashMap<PackageName, PinnedPackage>) -> Self {
        Self(packages)
    }

    /// Iterate over the pinned packages in this resolution.
    pub fn iter(&self) -> impl Iterator<Item = (&PackageName, &PinnedPackage)> {
        self.0.iter()
    }

    /// Iterate over the wheels in this resolution.
    pub fn into_files(self) -> impl Iterator<Item = File> {
        self.0.into_values().map(|package| package.file)
    }

    /// Return the number of pinned packages in this resolution.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug)]
pub struct PinnedPackage {
    pub metadata: Metadata21,
    pub file: File,
}

impl PinnedPackage {
    pub fn version(&self) -> &Version {
        &self.metadata.version
    }
}
