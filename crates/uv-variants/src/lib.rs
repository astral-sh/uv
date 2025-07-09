pub mod resolved_variants;
pub mod variants_json;

use indexmap::IndexMap;

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct VariantProviderOutput {
    /// The namespace of the provider.
    pub namespace: String,
    /// Features (in order) mapped to their properties (in order).
    pub features: IndexMap<String, Vec<String>>,
}

/// The priority of a variant.
///
/// A wrapper around [`NonZeroU32`]. Higher values indicate higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariantPriority {
    NoVariant,
    Variant,
}
