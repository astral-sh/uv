use std::sync::{Arc, Mutex};

use uv_distribution_types::{IndexMetadata, RequirementSource};
use uv_normalize::PackageName;

use crate::pubgrub::IndexId;
use crate::resolver::ForkMap;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// A map of package names to their explicit index.
///
/// For example, given:
/// ```toml
/// [[tool.uv.index]]
/// name = "pytorch"
/// url = "https://download.pytorch.org/whl/cu130"
///
/// [tool.uv.sources]
/// torch = { index = "pytorch" }
/// ```
///
/// [`Indexes`] would contain a single entry mapping `torch` to `https://download.pytorch.org/whl/cu130`.
#[derive(Debug, Default, Clone)]
pub(crate) struct Indexes {
    initial: ForkMap<IndexMetadata>,
    resources: Arc<Mutex<Vec<IndexMetadata>>>,
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
                index: Some(index), ..
            } = &requirement.source
            else {
                continue;
            };
            indexes.add(requirement.as_ref(), index.clone());
        }

        Self {
            initial: indexes,
            resources: Arc::default(),
        }
    }

    /// Returns `true` if the map contains any indexes for a package.
    pub(crate) fn contains_key(&self, name: &PackageName) -> bool {
        self.initial.contains_key(name)
    }

    /// Return the explicit index used for a package in the given fork.
    pub(crate) fn get(&self, name: &PackageName, env: &ResolverEnvironment) -> Vec<&IndexMetadata> {
        self.initial.get(name, env)
    }

    /// Give an explicit registry a stable candidate identity across forks and retries.
    pub(crate) fn intern(&self, index: &IndexMetadata) -> IndexId {
        let mut resources = self
            .resources
            .lock()
            .expect("index resource lock is not poisoned");
        if let Some(position) = resources.iter().position(|existing| existing == index) {
            return IndexId(position);
        }
        let id = IndexId(resources.len());
        resources.push(index.clone());
        id
    }

    pub(crate) fn resource(&self, id: IndexId) -> IndexMetadata {
        self.resources
            .lock()
            .expect("index resource lock is not poisoned")[id.0]
            .clone()
    }
}
