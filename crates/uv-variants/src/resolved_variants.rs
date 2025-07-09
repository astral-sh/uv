use crate::VariantProviderOutput;
use crate::variants_json::{VariantNamespace, VariantsJsonContent};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct ResolvedVariants {
    pub variants_json: VariantsJsonContent,
    pub target_variants: FxHashMap<VariantNamespace, VariantProviderOutput>,
}
