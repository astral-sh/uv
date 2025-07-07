use crate::VariantKeyConfig;
use crate::variants_json::{VariantNamespace, VariantsJsonContent};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct ResolvedVariants {
    pub variants_json: VariantsJsonContent,
    pub resolved_priorities: FxHashMap<VariantNamespace, Vec<VariantKeyConfig>>,
}
