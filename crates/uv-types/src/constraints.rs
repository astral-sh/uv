use std::hash::BuildHasherDefault;

use rustc_hash::FxHashMap;

use pep508_rs::Requirement;
use uv_normalize::PackageName;

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints(FxHashMap<PackageName, Vec<Requirement>>);

impl Constraints {
    /// Create a new set of constraints from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut constraints: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());
        for requirement in requirements {
            constraints
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(constraints)
    }

    /// Return an iterator over all [`Requirement`]s in the constraint set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the constraints for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }
}
