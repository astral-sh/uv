use std::hash::BuildHasherDefault;
use std::sync::Arc;

use crate::resolver::provider::{MetadataResponse, VersionsResponse};
use rustc_hash::FxHasher;
use uv_distribution_types::{IndexUrl, VersionId};
use uv_normalize::PackageName;
use uv_once_map::OnceMap;
use uv_variants::resolved_variants::ResolvedVariants;

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<SharedInMemoryIndex>);

#[derive(Default)]
struct SharedInMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    implicit: FxOnceMap<PackageName, Arc<VersionsResponse>>,

    explicit: FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>>,

    /// A map from package ID to metadata for that distribution.
    distributions: FxOnceMap<VersionId, Arc<MetadataResponse>>,

    /// The resolved variant priorities for a package version.
    // TODO(konsti): Why is this index url separate in implicit/explicit? should the index url be
    // an option? ask charlie about implicit vs explicit
    variant_priorities: FxOnceMap<(VersionId, Option<IndexUrl>), Arc<ResolvedVariants>>,
}

pub(crate) type FxOnceMap<K, V> = OnceMap<K, V, BuildHasherDefault<FxHasher>>;

impl InMemoryIndex {
    /// Returns a reference to the package metadata map.
    pub fn implicit(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.implicit
    }

    /// Returns a reference to the package metadata map.
    pub fn explicit(&self) -> &FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>> {
        &self.0.explicit
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &FxOnceMap<VersionId, Arc<MetadataResponse>> {
        &self.0.distributions
    }

    /// Returns a reference to the variant priorities map.
    pub fn variant_priorities(
        &self,
    ) -> &FxOnceMap<(VersionId, Option<IndexUrl>), Arc<ResolvedVariants>> {
        &self.0.variant_priorities
    }
}
