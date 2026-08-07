use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;
use uv_distribution_types::IndexMetadata;
use uv_normalize::PackageName;

use crate::ResolveError;
use crate::resolver::ResolverEnvironment;

/// See [`crate::resolver::ForkState`].
#[derive(Default, Debug, Clone)]
pub(crate) struct ForkIndexes(FxHashMap<PackageName, IndexMetadata>);

impl ForkIndexes {
    /// Get the [`Index`] previously used for a package in this fork.
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&IndexMetadata> {
        self.0.get(package_name)
    }

    /// Check that this is the only [`Index`] used for this package in this fork.
    pub(crate) fn insert(
        &mut self,
        package_name: PackageName,
        index: IndexMetadata,
        env: &ResolverEnvironment,
    ) -> Result<(), ResolveError> {
        match self.0.entry(package_name) {
            Entry::Occupied(mut occupied) => {
                let previous = occupied.insert(index);
                if previous != *occupied.get() {
                    let mut conflicts = vec![previous.url, occupied.get().url.clone()];
                    conflicts.sort();
                    return Err(ResolveError::ConflictingIndexesForEnvironment {
                        package_name: occupied.key().clone(),
                        indexes: conflicts,
                        env: env.clone(),
                    });
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(index);
            }
        }
        Ok(())
    }
}
