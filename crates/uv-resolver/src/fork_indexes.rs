use rustc_hash::FxHashMap;
use uv_distribution_types::IndexMetadata;
use uv_normalize::PackageName;

/// See [`crate::resolver::ForkState`].
#[derive(Default, Debug, Clone)]
pub(crate) struct ForkIndexes(FxHashMap<PackageName, IndexMetadata>);

impl ForkIndexes {
    /// Get the [`IndexMetadata`] selected for a package in this fork.
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&IndexMetadata> {
        self.0.get(package_name)
    }
}

impl FromIterator<(PackageName, IndexMetadata)> for ForkIndexes {
    fn from_iter<T: IntoIterator<Item = (PackageName, IndexMetadata)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
