use itertools::Either;
use std::hash::BuildHasherDefault;

use rustc_hash::FxHashMap;

use pep508_rs::Requirement;
use uv_normalize::PackageName;

/// A set of overrides for a set of requirements.
#[derive(Debug, Default, Clone)]
pub(crate) struct Overrides(FxHashMap<PackageName, Vec<Requirement>>);

impl Overrides {
    /// Create a new set of overrides from a set of requirements.
    pub(crate) fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut overrides: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());
        for requirement in requirements {
            overrides
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(overrides)
    }

    /// Get the overrides for a package.
    pub(crate) fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }

    /// Apply the overrides to a set of requirements.
    pub(crate) fn apply<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> impl Iterator<Item = &Requirement> {
        requirements.iter().flat_map(|requirement| {
            if let Some(overrides) = self.get(&requirement.name) {
                Either::Left(overrides.iter())
            } else {
                Either::Right(std::iter::once(requirement))
            }
        })
    }
}
