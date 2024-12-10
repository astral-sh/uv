use rustc_hash::FxHashMap;
use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;

use crate::resolver::ResolverEnvironment;
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
        env: &ResolverEnvironment,
    ) -> Result<(), ResolveError> {
        if let Some(previous) = self.0.insert(package_name.clone(), index.clone()) {
            if &previous != index {
                let mut conflicts = vec![previous.to_string(), index.to_string()];
                conflicts.sort();
                return Err(ResolveError::ConflictingIndexesForEnvironment {
                    package_name: package_name.clone(),
                    indexes: conflicts,
                    env: env.clone(),
                });
            }
        }
        Ok(())
    }
}
