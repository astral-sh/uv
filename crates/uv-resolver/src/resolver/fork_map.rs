use pep508_rs::{MarkerTree, PackageName};
use pypi_types::Requirement;
use rustc_hash::FxHashMap;

use crate::marker::is_disjoint;
use crate::ResolverMarkers;

/// A set of package names associated with a given fork.
pub(crate) type ForkSet = ForkMap<()>;

/// A map from package names to their values for a given fork.
#[derive(Debug, Clone)]
pub(crate) struct ForkMap<T>(FxHashMap<PackageName, Vec<Entry<T>>>);

/// An entry in a [`ForkMap`].
#[derive(Debug, Clone)]
struct Entry<T> {
    value: T,
    marker: Option<MarkerTree>,
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
            marker: requirement.marker.clone(),
        };

        self.0
            .entry(requirement.name.clone())
            .or_default()
            .push(entry);
    }

    /// Returns `true` if the map contains any values for a package that are compatible with the
    /// given fork.
    pub(crate) fn contains(&self, package_name: &PackageName, markers: &ResolverMarkers) -> bool {
        !self.get(package_name, markers).is_empty()
    }

    /// Returns a list of values associated with a package that are compatible with the given fork.
    ///
    /// Compatibility implies that the markers on the requirement that contained this value
    /// are not disjoint with the given fork. Note that this does not imply that the requirement
    /// diverged in the given fork - values from overlapping forks may be combined.
    pub(crate) fn get(&self, package_name: &PackageName, markers: &ResolverMarkers) -> Vec<&T> {
        let Some(values) = self.0.get(package_name) else {
            return Vec::new();
        };

        match markers {
            // If we are solving for a specific environment we already filtered
            // compatible requirements `from_manifest`.
            ResolverMarkers::SpecificEnvironment(_) => values
                .first()
                .map(|entry| &entry.value)
                .into_iter()
                .collect(),

            // Return all values that were requested with markers that are compatible
            // with the current fork, i.e. the markers are not disjoint.
            ResolverMarkers::Fork(fork) => values
                .iter()
                .filter(|entry| {
                    !entry
                        .marker
                        .as_ref()
                        .is_some_and(|marker| is_disjoint(fork, marker))
                })
                .map(|entry| &entry.value)
                .collect(),

            // If we haven't forked yet, all values are potentially compatible.
            ResolverMarkers::Universal { .. } => values.iter().map(|entry| &entry.value).collect(),
        }
    }
}
