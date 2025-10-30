use rustc_hash::FxHashSet;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;

/// A set of exclusions for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Excludes(FxHashSet<PackageName>);

impl Excludes {
    /// Create a new set of exclusions from a list of package names.
    pub fn from_package_names(names: Vec<PackageName>) -> Self {
        Self(names.into_iter().collect())
    }

    /// Return an iterator over all package names in the exclusion set.
    pub fn iter(&self) -> impl Iterator<Item = &PackageName> {
        self.0.iter()
    }

    /// Check if a package is excluded.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.0.contains(name)
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
