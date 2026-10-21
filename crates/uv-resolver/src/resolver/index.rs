use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;
use uv_distribution_types::{
    Dist, DistributionId, IndexUrl, RegistryVariantsJson, SelectedVariant,
    SupportedVariantProperties, VariantsJsonContent,
};
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, RegisteredOnceMap};
use uv_resolver_types::DistributionMetadataIndex;

use crate::resolver::provider::{MetadataResponse, MetadataUnavailable, VersionsResponse};

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

    /// A map from an index variant metadata file to its parsed contents.
    variants_json: FxOnceMap<RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>>,

    /// Target-specific properties discovered for verified variant metadata.
    variant_properties: FxOnceMap<RegistryVariantsJson, Arc<SupportedVariantProperties>>,

    /// Target-specific variant selections for direct wheels.
    direct_variants: DirectWheelVariantIndex,

    /// Dependency metadata combined across the wheels retained by a universal resolution.
    variant_dependencies: FxOnceMap<Arc<Dist>, Arc<MetadataResponse>>,
}

#[derive(Debug)]
pub(crate) struct ParsedVariantsJson {
    pub(crate) content: VariantsJsonContent,
    pub(crate) artifact: RegistryVariantsJson,
}

pub(crate) type ParsedVariantsJsonResponse = Result<Arc<ParsedVariantsJson>, MetadataUnavailable>;
pub(crate) type FxOnceMap<K, V> = RegisteredOnceMap<K, V, BuildHasherDefault<FxHasher>>;
pub(crate) type FxRegisteredEntry<'a, K, V> =
    RegisteredEntry<'a, K, V, BuildHasherDefault<FxHasher>>;

/// Selected variant properties shared between direct-wheel lookahead and resolution.
pub type DirectWheelVariantIndex =
    FxOnceMap<DistributionId, Result<Option<SelectedVariant>, Arc<uv_distribution::Error>>>;

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

    /// Returns the selected variants for direct wheels.
    pub fn direct_variants(&self) -> &DirectWheelVariantIndex {
        &self.0.direct_variants
    }

    pub(crate) fn variants_json(
        &self,
    ) -> &FxOnceMap<RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>> {
        &self.0.variants_json
    }

    pub(crate) fn variant_properties(
        &self,
    ) -> &FxOnceMap<RegistryVariantsJson, Arc<SupportedVariantProperties>> {
        &self.0.variant_properties
    }

    pub(crate) fn variant_dependencies(&self) -> &FxOnceMap<Arc<Dist>, Arc<MetadataResponse>> {
        &self.0.variant_dependencies
    }

    /// Return exclusive access to distribution metadata when no cloned index can use it.
    pub fn distributions_mut(&mut self) -> Option<&mut DistributionMetadataIndex> {
        Arc::get_mut(&mut self.0).map(|index| &mut index.distributions)
    }
}
