use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;

use uv_distribution::PackageVariantCache;
use uv_distribution_types::{IndexUrl, VersionId};
use uv_normalize::PackageName;
use uv_once_map::OnceMap;
use uv_variants::cache::VariantProviderCache;

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

    /// A map from package ID to metadata for that distribution.
    distributions: FxOnceMap<VersionId, Arc<MetadataResponse>>,

    /// The resolved variants, indexed by provider.
    variant_providers: VariantProviderCache,

    /// The resolved variant priorities, indexed by package version.
    variant_priorities: PackageVariantCache,
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

    /// Returns a reference to the variant providers map.
    pub fn variant_providers(&self) -> &VariantProviderCache {
        &self.0.variant_providers
    }

    /// Returns a reference to the variant priorities map.
    pub fn variant_priorities(&self) -> &PackageVariantCache {
        &self.0.variant_priorities
    }
}
