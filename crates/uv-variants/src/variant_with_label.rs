use crate::variants_json::Variant;
use uv_distribution_filename::VariantLabel;
use uv_pep508::{MarkerVariantsEnvironment, VariantFeature, VariantNamespace, VariantValue};

#[derive(Debug, Default)]
pub struct VariantWithLabel {
    pub variant: Variant,
    pub label: Option<VariantLabel>,
}

impl MarkerVariantsEnvironment for VariantWithLabel {
    fn contains_namespace(&self, namespace: &VariantNamespace) -> bool {
        self.variant.contains_namespace(namespace)
    }

    fn contains_feature(&self, namespace: &VariantNamespace, feature: &VariantFeature) -> bool {
        self.variant.contains_feature(namespace, feature)
    }

    fn contains_property(
        &self,
        namespace: &VariantNamespace,
        feature: &VariantFeature,
        value: &VariantValue,
    ) -> bool {
        self.variant.contains_property(namespace, feature, value)
    }

    fn contains_base_namespace(&self, base: &str, namespace: &VariantNamespace) -> bool {
        self.variant.contains_base_namespace(base, namespace)
    }

    fn contains_base_feature(
        &self,
        base: &str,
        namespace: &VariantNamespace,
        feature: &VariantFeature,
    ) -> bool {
        self.variant.contains_base_feature(base, namespace, feature)
    }

    fn contains_base_property(
        &self,
        base: &str,
        namespace: &VariantNamespace,
        feature: &VariantFeature,
        value: &VariantValue,
    ) -> bool {
        self.variant
            .contains_base_property(base, namespace, feature, value)
    }

    fn label(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(uv_distribution_filename::VariantLabel::as_str)
    }

    fn is_universal(&self) -> bool {
        false
    }
}
