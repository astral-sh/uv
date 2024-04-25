use std::hash::BuildHasherDefault;

use distribution_types::UvRequirement;
use rustc_hash::FxHashMap;

use uv_normalize::PackageName;

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints(FxHashMap<PackageName, Vec<UvRequirement>>);

impl Constraints {
    /// Create a new set of constraints from a set of requirements.
    pub fn from_requirements(requirements: Vec<UvRequirement>) -> Self {
        let mut constraints: FxHashMap<PackageName, Vec<UvRequirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());
        for requirement in requirements {
            constraints
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(constraints)
    }

    /// Return an iterator over all [`UvRequirement`]s in the constraint set.
    pub fn requirements(&self) -> impl Iterator<Item = &UvRequirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the constraints for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<UvRequirement>> {
        self.0.get(name)
    }

    /// Apply the constraints to a set of requirements.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a UvRequirement>,
    ) -> impl Iterator<Item = &UvRequirement> {
        requirements.into_iter().flat_map(|requirement| {
            std::iter::once(requirement).chain(
                self.get(&requirement.name)
                    .into_iter()
                    .flat_map(|constraints| constraints.iter()),
            )
        })
    }
}
