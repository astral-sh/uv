use std::hash::BuildHasherDefault;

use rustc_hash::FxHashMap;

use distribution_types::{LocalEditable, UvRequirements};
use pypi_types::Metadata23;
use uv_normalize::PackageName;

/// A set of editable packages, indexed by package name.
#[derive(Debug, Default, Clone)]
pub(crate) struct Editables(FxHashMap<PackageName, (LocalEditable, Metadata23, UvRequirements)>);

impl Editables {
    /// Create a new set of editables from a set of requirements.
    pub(crate) fn from_requirements(
        requirements: Vec<(LocalEditable, Metadata23, UvRequirements)>,
    ) -> Self {
        let mut editables =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());
        for (editable_requirement, metadata, requirements) in requirements {
            editables.insert(
                metadata.name.clone(),
                (editable_requirement, metadata, requirements),
            );
        }
        Self(editables)
    }

    /// Get the editable for a package.
    pub(crate) fn get(
        &self,
        name: &PackageName,
    ) -> Option<&(LocalEditable, Metadata23, UvRequirements)> {
        self.0.get(name)
    }

    /// Iterate over all editables.
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = &(LocalEditable, Metadata23, UvRequirements)> {
        self.0.values()
    }
}
