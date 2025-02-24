use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::{ConflictItem, RequirementSource};

use crate::resolver::ForkMap;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

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
pub(crate) struct Indexes(ForkMap<Entry>);

#[derive(Debug, Clone)]
struct Entry {
    index: IndexUrl,
    conflict: Option<ConflictItem>,
}

impl Indexes {
    /// Determine the set of explicit, pinned indexes in the [`Manifest`].
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut indexes = ForkMap::default();

        for requirement in manifest.requirements(env, dependencies) {
            let RequirementSource::Registry {
                index: Some(index),
                conflict,
                ..
            } = &requirement.source
            else {
                continue;
            };
            let index = IndexUrl::from(VerbatimUrl::from_url(index.clone()));
            let conflict = conflict.clone();
            indexes.add(&requirement, Entry { index, conflict });
        }

        Self(indexes)
    }

    /// Returns `true` if the map contains any indexes for a package.
    pub(crate) fn contains_key(&self, name: &PackageName) -> bool {
        self.0.contains_key(name)
    }

    /// Return the explicit index used for a package in the given fork.
    pub(crate) fn get(&self, name: &PackageName, env: &ResolverEnvironment) -> Vec<&IndexUrl> {
        let entries = self.0.get(name, env);
        entries
            .iter()
            .filter(|entry| {
                entry
                    .conflict
                    .as_ref()
                    .is_none_or(|conflict| env.included_by_group(conflict.as_ref()))
            })
            .map(|entry| &entry.index)
            .collect()
    }
}
