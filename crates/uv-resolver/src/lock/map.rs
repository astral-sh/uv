use rustc_hash::FxHashMap;

use crate::lock::{Package, PackageId};

/// A map from package to values, indexed by [`PackageId`].
#[derive(Debug, Clone)]
pub struct PackageMap<T>(FxHashMap<PackageId, T>);

impl<T> Default for PackageMap<T> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<T> PackageMap<T> {
    /// Insert a value by [`PackageId`].
    pub fn insert(&mut self, package: Package, value: T) -> Option<T> {
        self.0.insert(package.id, value)
    }

    /// Get a value by [`PackageId`].
    pub(crate) fn get(&self, package_id: &PackageId) -> Option<&T> {
        self.0.get(package_id)
    }
}

impl<T> FromIterator<(Package, T)> for PackageMap<T> {
    fn from_iter<I: IntoIterator<Item = (Package, T)>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|(package, value)| (package.id, value))
                .collect(),
        )
    }
}

impl<T> Extend<(Package, T)> for PackageMap<T> {
    fn extend<I: IntoIterator<Item = (Package, T)>>(&mut self, iter: I) {
        self.0
            .extend(iter.into_iter().map(|(package, value)| (package.id, value)));
    }
}
