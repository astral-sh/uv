use rustc_hash::FxHashMap;

use distribution_types::{LocalEditable, Requirements};
use pypi_types::Metadata23;
use uv_normalize::PackageName;

/// A built editable for which we know its dependencies and other static metadata.
#[derive(Debug, Clone)]
pub struct BuiltEditableMetadata {
    pub built: LocalEditable,
    pub metadata: Metadata23,
    pub requirements: Requirements,
}

/// A set of editable packages, indexed by package name.
#[derive(Debug, Default, Clone)]
pub(crate) struct Editables(FxHashMap<PackageName, BuiltEditableMetadata>);

impl Editables {
    /// Create a new set of editables from a set of requirements.
    pub(crate) fn from_requirements(requirements: Vec<BuiltEditableMetadata>) -> Self {
        Self(
            requirements
                .into_iter()
                .map(|editable| (editable.metadata.name.clone(), editable))
                .collect(),
        )
    }

    /// Get the editable for a package.
    pub(crate) fn get(&self, name: &PackageName) -> Option<&BuiltEditableMetadata> {
        self.0.get(name)
    }

    /// Returns `true` if the given package is editable.
    pub(crate) fn contains(&self, name: &PackageName) -> bool {
        self.0.contains_key(name)
    }

    /// Iterate over all editables.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &BuiltEditableMetadata> {
        self.0.values()
    }
}
