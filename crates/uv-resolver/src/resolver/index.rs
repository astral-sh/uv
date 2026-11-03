use std::hash::BuildHasherDefault;
use std::path::Path;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};
use uv_distribution::UnversionedMetadata;
use uv_distribution_types::{Dist, IndexUrl, Name, VariantsJsonContent};
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, RegisteredOnceMap};
use uv_resolver_types::DistributionMetadataIndex;

use crate::prioritized_distribution::RegistryVariantsJson;
use crate::resolver::provider::{MetadataUnavailable, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex {
    shared: Arc<SharedInMemoryIndex>,
    /// Static requirements belong to the workspace being solved. Built distribution metadata
    /// can also be reused by other solves sharing this index.
    manifests: Option<Arc<FxHashMap<PackageName, WorkspaceManifest>>>,
}

/// Static metadata from a workspace member's directory.
struct WorkspaceManifest {
    path: Box<Path>,
    metadata: Arc<UnversionedMetadata>,
}

#[derive(Default)]
struct SharedInMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    implicit: FxOnceMap<PackageName, Arc<VersionsResponse>>,

    explicit: FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>>,

    /// A map from a concrete distribution to its metadata.
    distributions: DistributionMetadataIndex,

    /// A map from an index variant metadata file to its parsed contents.
    variants_json: FxOnceMap<RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>>,
}

pub(crate) type ParsedVariantsJsonResponse = Result<VariantsJsonContent, MetadataUnavailable>;
pub(crate) type FxOnceMap<K, V> = RegisteredOnceMap<K, V, BuildHasherDefault<FxHasher>>;
pub(crate) type FxRegisteredEntry<'a, K, V> =
    RegisteredEntry<'a, K, V, BuildHasherDefault<FxHasher>>;

impl InMemoryIndex {
    /// Attach the current workspace's manifests while sharing the caches for distributions and indexes.
    #[must_use]
    pub fn with_workspace_manifests(
        &self,
        projects: Vec<(Box<Path>, UnversionedMetadata)>,
    ) -> Self {
        let manifests: FxHashMap<_, _> = projects
            .into_iter()
            .map(|(path, metadata)| {
                (
                    metadata.requirements.name.clone(),
                    WorkspaceManifest {
                        path,
                        metadata: Arc::new(metadata),
                    },
                )
            })
            .collect();
        Self {
            shared: self.shared.clone(),
            manifests: (!manifests.is_empty()).then(|| Arc::new(manifests)),
        }
    }

    pub(crate) fn workspace_manifest(
        &self,
        name: &PackageName,
    ) -> Option<&Arc<UnversionedMetadata>> {
        Some(&self.manifests.as_ref()?.get(name)?.metadata)
    }

    /// Return the manifest only for the workspace directory from which it was read.
    pub fn manifest_for_dist(&self, dist: &Dist) -> Option<&UnversionedMetadata> {
        let manifest = self.manifests.as_ref()?.get(dist.name())?;
        (uv_fs::is_same_file_allow_missing(&manifest.path, dist.source_tree()?) == Some(true))
            .then_some(&manifest.metadata)
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn implicit(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.shared.implicit
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn explicit(&self) -> &FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>> {
        &self.shared.explicit
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &DistributionMetadataIndex {
        &self.shared.distributions
    }

    pub(crate) fn variants_json(
        &self,
    ) -> &FxOnceMap<RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>> {
        &self.shared.variants_json
    }

    /// Return exclusive access to distribution metadata when no cloned index can use it.
    pub fn distributions_mut(&mut self) -> Option<&mut DistributionMetadataIndex> {
        Arc::get_mut(&mut self.shared).map(|index| &mut index.distributions)
    }
}
