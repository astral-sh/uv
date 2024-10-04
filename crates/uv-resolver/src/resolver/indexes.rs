use crate::{DependencyMode, Manifest, ResolveError, ResolverMarkers};

use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::RequirementSource;

/// A map of package names to their explicit index.
///
/// For example, given:
/// ```toml
/// [[tool.uv.index]]
/// name = "pytorch"
/// url = "https://download.pytorch.org/whl/cu121"
///
/// [tool.uv.sources]
/// torch = { index = "pytorch" }
/// ```
///
/// [`Indexes`] would contain a single entry mapping `torch` to `https://download.pytorch.org/whl/cu121`.
#[derive(Debug, Default, Clone)]
pub(crate) struct Indexes(FxHashMap<PackageName, IndexUrl>);

impl Indexes {
    /// Determine the set of explicit, pinned indexes in the [`Manifest`].
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &ResolverMarkers,
        dependencies: DependencyMode,
    ) -> Result<Self, ResolveError> {
        let mut indexes = FxHashMap::<PackageName, IndexUrl>::default();

        for requirement in manifest.requirements(markers, dependencies) {
            let RequirementSource::Registry {
                index: Some(index), ..
            } = &requirement.source
            else {
                continue;
            };
            let index = IndexUrl::from(VerbatimUrl::from_url(index.clone()));
            match indexes.entry(requirement.name.clone()) {
                Entry::Occupied(entry) => {
                    let existing = entry.get();
                    if *existing != index {
                        return Err(ResolveError::ConflictingIndexes(
                            requirement.name.clone(),
                            existing.to_string(),
                            index.to_string(),
                        ));
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(index);
                }
            }
        }

        Ok(Self(indexes))
    }

    /// Return the explicit index for a given [`PackageName`].
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&IndexUrl> {
        self.0.get(package_name)
    }
}
