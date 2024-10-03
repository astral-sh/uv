use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;
use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;

use crate::resolver::ResolverMarkers;
use crate::ResolveError;

/// See [`crate::resolver::ForkState`].
#[derive(Default, Debug, Clone)]
pub(crate) struct ForkIndexes(FxHashMap<PackageName, IndexUrl>);

impl ForkIndexes {
    /// Get the [`IndexUrl`] previously used for a package in this fork.
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&IndexUrl> {
        self.0.get(package_name)
    }

    /// Check that this is the only [`IndexUrl`] used for this package in this fork.
    pub(crate) fn insert(
        &mut self,
        package_name: &PackageName,
        index: &IndexUrl,
        fork_markers: &ResolverMarkers,
    ) -> Result<(), ResolveError> {
        match self.0.entry(package_name.clone()) {
            Entry::Occupied(previous) => {
                if previous.get() != index {
                    let mut conflicts = vec![previous.get().to_string(), index.to_string()];
                    conflicts.sort();
                    return match fork_markers {
                        ResolverMarkers::Universal { .. }
                        | ResolverMarkers::SpecificEnvironment(_) => {
                            Err(ResolveError::ConflictingIndexesUniversal(
                                package_name.clone(),
                                conflicts,
                            ))
                        }
                        ResolverMarkers::Fork(fork_markers) => {
                            Err(ResolveError::ConflictingIndexesFork {
                                package_name: package_name.clone(),
                                indexes: conflicts,
                                fork_markers: fork_markers.clone(),
                            })
                        }
                    };
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(index.clone());
            }
        }
        Ok(())
    }
}
