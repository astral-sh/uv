use rustc_hash::FxHashMap;
use uv_pep508::{MarkerTree, PackageName};
use uv_pypi_types::Requirement;

use crate::ResolverEnvironment;

/// A set of package names associated with a given fork.
pub(crate) type ForkSet = ForkMap<()>;

/// A map from package names to their values for a given fork.
#[derive(Debug, Clone)]
pub(crate) struct ForkMap<T>(FxHashMap<PackageName, Vec<Entry<T>>>);

/// An entry in a [`ForkMap`].
#[derive(Debug, Clone)]
struct Entry<T> {
    value: T,
    marker: MarkerTree,
}

impl<T> Default for ForkMap<T> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<T> ForkMap<T> {
    /// Associate a value with the [`Requirement`] in a given fork.
    pub(crate) fn add(&mut self, requirement: &Requirement, value: T) {
        let entry = Entry {
            value,
            marker: requirement.marker,
        };

        self.0
            .entry(requirement.name.clone())
            .or_default()
            .push(entry);
    }

    /// Returns `true` if the map contains any values for a package that are compatible with the
    /// given fork.
    pub(crate) fn contains(&self, package_name: &PackageName, env: &ResolverEnvironment) -> bool {
        !self.get(package_name, env).is_empty()
    }

    /// Returns `true` if the map contains any values for a package.
    pub(crate) fn contains_key(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }

    /// Returns a list of values associated with a package that are compatible with the given fork.
    ///
    /// Compatibility implies that the markers on the requirement that contained this value
    /// are not disjoint with the given fork. Note that this does not imply that the requirement
    /// diverged in the given fork - values from overlapping forks may be combined.
    pub(crate) fn get(&self, package_name: &PackageName, env: &ResolverEnvironment) -> Vec<&T> {
        let Some(values) = self.0.get(package_name) else {
            return Vec::new();
        };
        values
            .iter()
            .filter(|entry| env.included_by_marker(entry.marker))
            .map(|entry| &entry.value)
            .collect()
    }
}
