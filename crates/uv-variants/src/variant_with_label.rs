use std::sync::Arc;

use uv_distribution_filename::VariantLabel;
use uv_pep508::{MarkerVariantsEnvironment, VariantFeature, VariantNamespace, VariantValue};

use crate::VariantProviderOutput;
use crate::resolved_variants::ResolvedVariants;
use crate::variants_json::{Variant, VariantsJsonContent};

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariantWithLabel {
    pub variant: Variant,
    pub label: Option<VariantLabel>,
}

impl VariantWithLabel {
    /// Validate retained properties against wheel metadata without querying providers again.
    ///
    /// Each wheel feature must retain a supported value, and every retained value must be
    /// declared by the wheel. The caller validates the metadata and wheel label separately.
    pub fn validate_properties(
        &self,
        metadata: VariantsJsonContent,
        label: &VariantLabel,
    ) -> Option<Self> {
        let resolved_namespaces = self
            .variant
            .iter()
            .map(|(namespace, features)| {
                (
                    namespace.clone(),
                    Arc::new(VariantProviderOutput {
                        namespace: namespace.clone(),
                        features: features.clone().into_iter().collect(),
                    }),
                )
            })
            .collect();
        ResolvedVariants {
            variants_json: Some(metadata),
            resolved_namespaces,
            ..ResolvedVariants::default()
        }
        .compatible_variant(label)
        .filter(|validated| validated.variant == self.variant)
    }
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
