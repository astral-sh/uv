use std::hash::BuildHasherDefault;
use std::sync::{Arc, Mutex};

use rustc_hash::{FxHashMap, FxHasher};
use uv_distribution_types::{Dist, DistributionId, HashCollection, HashValidation, IndexUrl};
use uv_normalize::PackageName;
use uv_once_map::OnceMap;
use uv_pypi_types::HashDigest;
use uv_resolver_types::DistributionMetadataIndex;
use uv_types::HashStrategy;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};

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

    /// A direct source can be revisited after its trusted digests change. Its metadata request
    /// must then validate against the new digests before any source backend runs.
    direct: FxOnceMap<(DistributionId, DirectHashKey), Arc<MetadataResponse>>,

    /// The direct metadata verified under the completed solutions' policies, used to build output.
    resolved_direct: Mutex<FxHashMap<DistributionId, Arc<MetadataResponse>>>,
}

pub(crate) type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

/// Hash collection and validation for one direct distribution, suitable for request deduplication.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DirectHashKey {
    collection: HashCollection,
    validation: DirectHashValidation,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum DirectHashValidation {
    None,
    Any(Vec<HashDigest>),
    All(Vec<HashDigest>),
}

impl DirectHashKey {
    pub(crate) fn new(dist: &Dist, hasher: &HashStrategy) -> Self {
        let policy = hasher.metadata_policy(dist);
        let normalize = |digests: &[HashDigest]| {
            let mut digests = digests.to_vec();
            digests.sort_unstable();
            digests.dedup();
            digests
        };
        let validation = match policy.validation {
            HashValidation::None => DirectHashValidation::None,
            HashValidation::Any(digests) => DirectHashValidation::Any(normalize(digests)),
            HashValidation::All(digests) => DirectHashValidation::All(normalize(digests)),
        };
        Self {
            collection: policy.collection,
            validation,
        }
    }
}

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

    pub(crate) fn direct(
        &self,
    ) -> &FxOnceMap<(DistributionId, DirectHashKey), Arc<MetadataResponse>> {
        &self.0.direct
    }

    pub(crate) fn commit_direct(&self, id: DistributionId, metadata: Arc<MetadataResponse>) {
        self.0
            .resolved_direct
            .lock()
            .expect("distribution metadata lock is not poisoned")
            .insert(id, metadata);
    }

    pub(crate) fn resolved_direct(&self, id: &DistributionId) -> Option<Arc<MetadataResponse>> {
        self.0
            .resolved_direct
            .lock()
            .expect("distribution metadata lock is not poisoned")
            .get(id)
            .cloned()
    }
}
