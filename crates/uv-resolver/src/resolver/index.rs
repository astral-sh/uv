use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};
use uv_distribution::UnversionedMetadata;
use uv_distribution_types::{Dist, Identifier, IndexUrl, Name, VariantsJsonContent};
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, RegisteredOnceMap};
use uv_pep440::Version;
use uv_resolver_types::DistributionMetadataIndex;

use crate::prioritized_distribution::RegistryVariantsJson;
use crate::resolver::provider::{MetadataResponse, MetadataUnavailable, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<SharedInMemoryIndex>);

#[derive(Default)]
struct SharedInMemoryIndex {
    /// Virtual members whose static metadata omits their dynamic version.
    deferred_versions: FxHashMap<PackageName, Dist>,

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
    /// Create a lock-local index so provisional versions cannot be reused by another operation.
    pub fn for_virtual_projects(projects: Vec<(Dist, UnversionedMetadata)>) -> Self {
        let mut index = SharedInMemoryIndex::default();
        for (dist, metadata) in projects {
            // PubGrub uses a concrete version to link the member to its extras and groups.
            // Register it as deferred so declared ranges use the backend's version.
            let metadata = metadata
                .with_version(Version::new([0]))
                .with_force_relative(true);
            index.distributions.done(
                dist.distribution_id(),
                Arc::new(MetadataResponse::Found(metadata.into())),
            );
            index.deferred_versions.insert(dist.name().clone(), dist);
        }
        Self(Arc::new(index))
    }

    pub(super) fn deferred_versions(&self) -> &FxHashMap<PackageName, Dist> {
        &self.0.deferred_versions
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn implicit(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.implicit
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn explicit(&self) -> &FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>> {
        &self.0.explicit
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &DistributionMetadataIndex {
        &self.0.distributions
    }

    pub(crate) fn variants_json(
        &self,
    ) -> &FxOnceMap<RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>> {
        &self.0.variants_json
    }

    /// Return exclusive access to distribution metadata when no cloned index can use it.
    pub fn distributions_mut(&mut self) -> Option<&mut DistributionMetadataIndex> {
        Arc::get_mut(&mut self.0).map(|index| &mut index.distributions)
    }
}
