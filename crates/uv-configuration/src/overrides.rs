use either::Either;
use rustc_hash::{FxBuildHasher, FxHashMap};

use pypi_types::Requirement;
use uv_normalize::PackageName;

/// A set of overrides for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Overrides(FxHashMap<PackageName, Vec<Requirement>>);

impl Overrides {
    /// Create a new set of overrides from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut overrides: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), FxBuildHasher);
        for requirement in requirements {
            overrides
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(overrides)
    }

    /// Return an iterator over all [`Requirement`]s in the override set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the overrides for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }

    /// Apply the overrides to a set of requirements.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a Requirement>,
    ) -> impl Iterator<Item = &Requirement> {
        requirements.into_iter().flat_map(|requirement| {
            if let Some(overrides) = self.get(&requirement.name) {
                Either::Left(overrides.iter())
            } else {
                Either::Right(std::iter::once(requirement))
            }
        })
    }
}
