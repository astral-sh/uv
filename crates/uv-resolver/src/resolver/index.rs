use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;
use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, RegisteredOnceMap};
use uv_resolver_types::DistributionMetadataIndex;

use crate::prioritized_distribution::RegistryVariantsJson;
use crate::resolver::provider::{VariantsJsonResponse, VersionsResponse};

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<SharedInMemoryIndex>);

#[derive(Default)]
struct SharedInMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    implicit: FxOnceMap<PackageName, Arc<VersionsResponse>>,

    explicit: FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>>,

    /// A map from a concrete distribution to its metadata.
    distributions: DistributionMetadataIndex,

    /// A map from an index variant metadata file to its response.
    variants_json: FxOnceMap<RegistryVariantsJson, Arc<VariantsJsonResponse>>,
}

pub(crate) type FxOnceMap<K, V> = RegisteredOnceMap<K, V, BuildHasherDefault<FxHasher>>;
pub(crate) type FxRegisteredEntry<'a, K, V> =
    RegisteredEntry<'a, K, V, BuildHasherDefault<FxHasher>>;

impl InMemoryIndex {
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
    ) -> &FxOnceMap<RegistryVariantsJson, Arc<VariantsJsonResponse>> {
        &self.0.variants_json
    }

    /// Return exclusive access to distribution metadata when no cloned index can use it.
    pub fn distributions_mut(&mut self) -> Option<&mut DistributionMetadataIndex> {
        Arc::get_mut(&mut self.0).map(|index| &mut index.distributions)
    }
}
