use std::hash::BuildHasherDefault;

use rustc_hash::FxHashMap;

use distribution_types::LocalEditable;
use pypi_types::Metadata21;
use uv_normalize::PackageName;

/// A set of editable packages, indexed by package name.
#[derive(Debug, Default, Clone)]
pub(crate) struct Editables(FxHashMap<PackageName, (LocalEditable, Metadata21)>);

impl Editables {
    /// Create a new set of editables from a set of requirements.
    pub(crate) fn from_requirements(requirements: Vec<(LocalEditable, Metadata21)>) -> Self {
        let mut editables =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());
        for (editable_requirement, metadata) in requirements {
            editables.insert(metadata.name.clone(), (editable_requirement, metadata));
        }
        Self(editables)
    }

    /// Get the editable for a package.
    pub(crate) fn get(&self, name: &PackageName) -> Option<&(LocalEditable, Metadata21)> {
        self.0.get(name)
    }

    /// Iterate over all editables.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(LocalEditable, Metadata21)> {
        self.0.values()
    }
}
