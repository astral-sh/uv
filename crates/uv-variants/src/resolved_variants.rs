use std::cmp::Reverse;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use tracing::warn;

use uv_distribution_filename::VariantLabel;
use uv_pep508::VariantNamespace;

use crate::VariantProviderOutput;
use crate::variant_with_label::VariantWithLabel;
use crate::variants_json::VariantsJsonContent;

/// A wheel variant's priority, with larger values preferred.
///
/// Compare the best property first, prefer longer lists on a shared prefix, then use the
/// lexically smaller label. Platform and build tags only break ties within the same label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantScore {
    properties: Vec<Reverse<(usize, usize, usize)>>,
    label: Reverse<VariantLabel>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedVariants {
    pub variants_json: Option<VariantsJsonContent>,
    pub resolved_namespaces: FxHashMap<VariantNamespace, Arc<VariantProviderOutput>>,
    /// Namespaces where the prototype provider's `enable-if` did not match.
    pub disabled_namespaces: FxHashSet<VariantNamespace>,
}

impl ResolvedVariants {
    /// Return a priority score, or `None` if any feature has no supported value.
    pub fn score_variant(&self, label: &VariantLabel) -> Option<VariantScore> {
        let metadata = self.variants_json.as_ref()?;
        let Some(variant) = metadata.variants.get(label) else {
            warn!("Variant {label} is missing in variants.json");
            return None;
        };

        let mut properties = Vec::new();
        for (namespace, features) in &**variant {
            if self.disabled_namespaces.contains(namespace) {
                return None;
            }
            let namespace_priority = metadata
                .default_priorities
                .namespace
                .iter()
                .position(|name| name == namespace)?;
            for (feature, values) in features {
                let provider = self.resolved_namespaces.get(namespace)?;
                let (feature_priority, _, supported_values) =
                    provider.features.get_full(feature)?;
                let value_priority = supported_values
                    .iter()
                    .position(|value| values.contains(value))?;
                properties.push(Reverse((
                    namespace_priority,
                    feature_priority,
                    value_priority,
                )));
            }
        }
        properties.sort_unstable_by(|left, right| right.cmp(left));
        Some(VariantScore {
            properties,
            label: Reverse(label.clone()),
        })
    }

    /// Properties of the selected wheel that are supported by the target system.
    ///
    /// PEP 825 requires all supported values here, even though ordering uses only the best one.
    pub fn compatible_variant(&self, label: &VariantLabel) -> Option<VariantWithLabel> {
        self.score_variant(label)?;
        let mut variant = self.variants_json.as_ref()?.variants.get(label)?.clone();
        variant.retain_values(|namespace, feature, value| {
            self.resolved_namespaces
                .get(namespace)
                .and_then(|provider| provider.features.get(feature))
                .is_some_and(|supported| supported.contains(value))
        });
        Some(VariantWithLabel {
            variant,
            label: Some(label.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use rustc_hash::FxHashSet;
    use serde_json::json;

    use super::ResolvedVariants;
    use crate::variants_json::VARIANT_SCHEMA;

    fn resolved() -> Result<ResolvedVariants, serde_json::Error> {
        Ok(ResolvedVariants {
            variants_json: Some(serde_json::from_value(json!({
                "$schema": VARIANT_SCHEMA,
                "default-priorities": {"namespace": ["nvidia", "x86_64"]},
                "variants": {
                    "gpu": {"nvidia": {"cuda": ["13.0"]}},
                    "gpu_cpuv2": {"nvidia": {"cuda": ["13.0"]}, "x86_64": {"level": ["v2"]}},
                    "gpu_cpuv4": {"nvidia": {"cuda": ["13.0"]}, "x86_64": {"level": ["v4"]}},
                    "cpuv4": {"x86_64": {"level": ["v4"]}},
                    "multi": {"nvidia": {"cuda": ["12.0", "13.0", "14.0"]}},
                    "unsupported": {"nvidia": {"cuda": ["14.0"]}},
                    "unsupported_feature": {"nvidia": {"unknown": ["on"]}},
                    "null": {}
                }
            }))?),
            resolved_namespaces: serde_json::from_value(json!({
                "nvidia": {"namespace": "nvidia", "features": {"cuda": ["13.0", "12.0"]}},
                "x86_64": {"namespace": "x86_64", "features": {"level": ["v4", "v3", "v2"]}}
            }))?,
            disabled_namespaces: FxHashSet::default(),
        })
    }

    #[test]
    fn pep825_variant_ordering() -> Result<(), Box<dyn std::error::Error>> {
        let resolved = resolved()?;
        let mut variants = resolved
            .variants_json
            .as_ref()
            .expect("test metadata")
            .variants
            .keys()
            .filter_map(|label| Some((resolved.score_variant(label)?, label.as_str())))
            .collect::<Vec<_>>();
        variants.sort_by(|left, right| right.0.cmp(&left.0));
        assert_debug_snapshot!(variants.iter().map(|(_, label)| label).collect::<Vec<_>>(), @r#"
        [
            "gpu_cpuv4",
            "gpu_cpuv2",
            "gpu",
            "multi",
            "cpuv4",
            "null",
        ]
        "#);
        Ok(())
    }

    #[test]
    fn pep825_best_supported_value() -> Result<(), Box<dyn std::error::Error>> {
        let mut resolved = resolved()?;
        // Package metadata no longer overrides the provider's feature and value priorities.
        resolved.variants_json = Some(serde_json::from_value(json!({
            "$schema": VARIANT_SCHEMA,
            "default-priorities": {
                "namespace": ["nvidia"],
                "feature": {"nvidia": ["unknown", "cuda"]},
                "property": {"nvidia": {"cuda": ["14.0", "12.0", "13.0"]}}
            },
            "variants": {
                "first": {"nvidia": {"cuda": ["13.0"]}},
                "second": {"nvidia": {"cuda": ["12.0", "14.0"]}},
                "third": {"nvidia": {"cuda": ["14.0"]}}
            }
        }))?);
        assert!(
            resolved.score_variant(&"first".parse()?) > resolved.score_variant(&"second".parse()?)
        );
        assert!(resolved.score_variant(&"third".parse()?).is_none());
        Ok(())
    }

    #[test]
    fn pep825_compatible_marker_properties() -> Result<(), Box<dyn std::error::Error>> {
        let resolved = resolved()?;
        let variant = resolved.compatible_variant(&"multi".parse()?);
        assert_debug_snapshot!(variant, @r#"
        Some(
            VariantWithLabel {
                variant: Variant(
                    {
                        VariantNamespace(
                            "nvidia",
                        ): {
                            VariantFeature(
                                "cuda",
                            ): [
                                VariantValue(
                                    "12.0",
                                ),
                                VariantValue(
                                    "13.0",
                                ),
                            ],
                        },
                    },
                ),
                label: Some(
                    VariantLabel(
                        "multi",
                    ),
                ),
            },
        )
        "#);
        Ok(())
    }
}
