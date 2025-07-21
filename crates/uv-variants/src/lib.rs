use indexmap::IndexMap;

use uv_pep508::{VariantFeature, VariantNamespace, VariantValue};

pub mod cache;
pub mod resolved_variants;
pub mod variants_json;

/// Wire format between with the Python shim for provider plugins.
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct VariantProviderOutput {
    /// The namespace of the provider.
    pub namespace: VariantNamespace,
    /// Features (in order) mapped to their properties (in order).
    pub features: IndexMap<VariantFeature, Vec<VariantValue>>,
}

/// The priority of a variant.
///
/// When we first create a `PrioritizedDist`, there is no information about which variants are
/// supported yet. In universal resolution, we don't need to query this information. In platform
/// specific resolution, we determine a best variant for the current platform - if any - after
/// selecting a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariantPriority {
    /// A variant wheel, it's unclear whether it's compatible.
    ///
    /// Variants only become supported after we ran the provider plugins.
    Unknown,
    /// A non-variant wheel.
    NonVariant,
    /// The supported variant wheel in this prioritized dist with the highest score.
    BestVariant,
}
