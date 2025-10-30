use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;

/// A set of exclusions for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Excludes(FxHashMap<PackageName, Vec<Requirement>>);

impl Excludes {
    /// Create a new set of exclusions from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut excludes: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), FxBuildHasher);
        for requirement in requirements {
            excludes
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(excludes)
    }

    /// Return an iterator over all [`Requirement`]s in the exclusion set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Check if a package is excluded.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.0.contains_key(name)
    }

    /// Filter out excluded requirements from a set of requirements.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a Requirement>,
    ) -> impl Iterator<Item = &'a Requirement> {
        requirements
            .into_iter()
            .filter(|requirement| !self.contains(&requirement.name))
    }
}
