use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::VariantProviderOutput;
use crate::variants_json::{VariantNamespace, VariantsJsonContent};

#[derive(Debug, Clone)]
pub struct ResolvedVariants {
    pub variants_json: VariantsJsonContent,
    pub target_variants: FxHashMap<VariantNamespace, Arc<VariantProviderOutput>>,
}
