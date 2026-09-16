use std::hash::BuildHasherDefault;
use std::iter;
use std::sync::{Arc, Mutex};

use rustc_hash::{FxHashMap, FxHasher};
use uv_distribution_types::{
    Dist, DistributionId, HashCollection, HashValidation, IndexMetadata, SourceDist,
};
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

    /// A flat index and Simple API index at the same URL expose different package versions.
    explicit: FxOnceMap<(PackageName, IndexMetadata), Arc<VersionsResponse>>,

    /// A map from a concrete distribution to its metadata. Source trees that do not require hash
    /// validation share this map with project commands so edits and in-memory metadata are visible.
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

    /// Source trees cannot produce archive hashes. Without required validation, their preparatory
    /// and solver metadata can therefore share the same cache, regardless of hash collection.
    pub(crate) fn uses_project_cache(&self, dist: &Dist) -> bool {
        match dist {
            Dist::Built(_) => false,
            Dist::Source(source) => match source {
                SourceDist::Directory(_) => self.validation == DirectHashValidation::None,
                SourceDist::Registry(_)
                | SourceDist::DirectUrl(_)
                | SourceDist::GitDirectory(_)
                | SourceDist::GitPath(_)
                | SourceDist::Path(_) => false,
            },
        }
    }
}

impl InMemoryIndex {
    /// Returns a reference to the package metadata map.
    pub(crate) fn implicit(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.implicit
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn explicit(
        &self,
    ) -> &FxOnceMap<(PackageName, IndexMetadata), Arc<VersionsResponse>> {
        &self.0.explicit
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &DistributionMetadataIndex {
        &self.0.distributions
    }

    /// Supply metadata for a source tree before the next solve, replacing any completed output.
    pub fn insert_project_metadata(&self, id: DistributionId, metadata: Arc<MetadataResponse>) {
        self.invalidate_project_metadata(&id);
        if let Some(alternate) = Self::other_directory_mode(&id) {
            self.0.distributions.done(alternate, metadata.clone());
        }
        self.0.distributions.done(id, metadata);
    }

    /// Discard source-tree metadata after editing a project and before starting the next solve.
    pub fn invalidate_project_metadata(
        &self,
        id: &DistributionId,
    ) -> Option<Arc<MetadataResponse>> {
        let alternate = Self::other_directory_mode(id);
        let mut committed = self
            .0
            .resolved_direct
            .lock()
            .expect("distribution metadata lock is not poisoned");
        let mut previous = None;
        for id in iter::once(id).chain(alternate.as_ref()) {
            let resolved = committed.remove(id);
            let cached = self.0.distributions.remove(id);
            previous = previous.or(cached).or(resolved);
        }
        previous
    }

    /// A project edit changes both build modes, even if preparatory metadata used a bare URL.
    fn other_directory_mode(id: &DistributionId) -> Option<DistributionId> {
        match id {
            DistributionId::Url(url) => Some(DistributionId::EditableDirectory(url.clone())),
            DistributionId::EditableDirectory(url) => Some(DistributionId::Url(url.clone())),
            DistributionId::PathBuf(_)
            | DistributionId::Digest(_)
            | DistributionId::AbsoluteUrl(_)
            | DistributionId::RelativeUrl(..) => None,
        }
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
