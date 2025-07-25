pub mod resolved_variants;
pub mod variants_json;

use crate::variants_json::{DefaultPriorities, Variant, VariantNamespace};

use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use tracing::debug;

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
pub struct VariantProviderOutput {
    /// The namespace of the provider.
    pub namespace: String,
    /// Features (in order) mapped to their properties (in order).
    pub features: IndexMap<String, Vec<String>>,
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

/// Return a priority score for the variant (higher is better) or `None` if it isn't compatible.
pub fn score_variant(
    default_priorities: &DefaultPriorities,
    target_namespaces: &FxHashMap<VariantNamespace, VariantProviderOutput>,
    variants_properties: &Variant,
) -> Option<Vec<usize>> {
    for (namespace, features) in &**variants_properties {
        for (feature, properties) in features {
            let resolved_properties = target_namespaces
                .get(namespace)
                .and_then(|namespace| namespace.features.get(feature))?;
            if !properties
                .iter()
                .any(|property| resolved_properties.contains(property))
            {
                return None;
            }
        }
    }

    // TODO(konsti): This is performance sensitive
    let mut scores = Vec::new();
    for namespace in &default_priorities.namespace {
        // Explicit priorities are optional, but take priority over the provider
        let explicit_feature_priorities = default_priorities.feature.get(namespace);
        let Some(target_variants) = target_namespaces.get(namespace) else {
            // TODO(konsti): Can this even happen?
            debug!("missing namespace priority {namespace}");
            continue;
        };
        let feature_priorities = explicit_feature_priorities.into_iter().flatten().chain(
            target_variants.features.keys().filter(|priority| {
                explicit_feature_priorities.is_none_or(|explicit| !explicit.contains(priority))
            }),
        );

        for feature in feature_priorities {
            let value_priorities: Vec<String> = default_priorities
                .property
                .get(namespace)
                .and_then(|namespace_features| namespace_features.get(feature))
                .into_iter()
                .flatten()
                .map(ToString::to_string)
                .chain(
                    target_namespaces
                        .get(namespace)
                        .and_then(|namespace| namespace.features.get(feature).cloned())
                        .into_iter()
                        .flatten(),
                )
                .collect();
            let Some(wheel_properties) = variants_properties
                .get(namespace)
                .and_then(|namespace| namespace.get(feature))
            else {
                scores.push(0);
                continue;
            };

            // Determine the highest scoring property
            // Reversed to give a higher score to earlier entries
            let score = value_priorities.len()
                - value_priorities
                    .iter()
                    .position(|feature| wheel_properties.contains(feature))
                    .unwrap_or(value_priorities.len());
            scores.push(score);
        }
    }
    Some(scores)
}

#[cfg(test)]
mod tests {
    use crate::{
        VariantProviderOutput,
        variants_json::{DefaultPriorities, Variant, VariantNamespace},
    };
    use itertools::Itertools;

    use super::score_variant;

    use insta::assert_snapshot;
    use rustc_hash::FxHashMap;
    use serde_json::json;

    fn host() -> FxHashMap<VariantNamespace, VariantProviderOutput> {
        serde_json::from_value(json!({
            "gpu": {
                "namespace": "gpu",
                "features": {
                    // Even though they are ahead of CUDA here, they are sorted below it due to the
                    // default priorities
                    "rocm": ["rocm68"],
                    "xpu": ["xpu1"],
                    "cuda": ["cu128", "cu126"]
                }
            },
            "cpu": {
                "namespace": "cpu",
                "features": {
                    "level": ["x86_64_v2", "x86_64_v1"]
                }
            },
        }))
        .unwrap()
    }

    // Default priorities in `variants.json`
    fn default_priorities() -> DefaultPriorities {
        serde_json::from_value(json!({
            "namespace": ["gpu", "cpu", "blas", "not_used_namespace"],
            "feature": {
                "gpu": ["cuda", "not_used_feature"],
                "cpu": ["level"],
            },
            "property": {
                "cpu": {
                    "level": ["x86_64_v4", "x86_64_v3", "x86_64_v2", "x86_64_v1", "not_used_value"],
                },
            },
        }))
        .unwrap()
    }

    fn score(variant: &Variant) -> Option<String> {
        let score = score_variant(&default_priorities(), &host(), variant)?;
        Some(score.iter().map(ToString::to_string).join(", "))
    }

    #[test]
    fn incompatible_variants() {
        let incompatible_namespace: Variant = serde_json::from_value(json!({
            "serial": {
                "usb": ["usb3"],
            },
        }))
        .unwrap();
        assert_eq!(score(&incompatible_namespace), None);

        let incompatible_feature: Variant = serde_json::from_value(json!({
            "gpu": {
                "rocm": ["rocm69"],
            },
        }))
        .unwrap();
        assert_eq!(score(&incompatible_feature), None);

        let incompatible_value: Variant = serde_json::from_value(json!({
            "gpu": {
                "cuda": ["cu130"],
            },
        }))
        .unwrap();
        assert_eq!(score(&incompatible_value), None);
    }

    #[test]
    fn variant_sorting() {
        let cu128_v2: Variant = serde_json::from_value(json!({
            "gpu": {
                "cuda": ["cu128"],
            },
            "cpu": {
                "level": ["x86_64_v2"],
            },
        }))
        .unwrap();
        let cu128_v1: Variant = serde_json::from_value(json!({
            "gpu": {
                "cuda": ["cu128"],
            },
            "cpu": {
                "level": ["x86_64_v1"],
            },
        }))
        .unwrap();
        let cu126_v2: Variant = serde_json::from_value(json!({
            "gpu": {
                "cuda": ["cu126"],
            },
            "cpu": {
                "level": ["x86_64_v2"],
            },
        }))
        .unwrap();
        let cu126_v1: Variant = serde_json::from_value(json!({
            "gpu": {
                "cuda": ["cu126"],
            },
            "cpu": {
                "level": ["x86_64_v1"],
            },
        }))
        .unwrap();
        let rocm: Variant = serde_json::from_value(json!({
            "gpu": {
                "rocm": ["rocm68"],
            },
        }))
        .unwrap();
        let xpu: Variant = serde_json::from_value(json!({
            "gpu": {
                "xpu": ["xpu1"],
            },
        }))
        .unwrap();
        // If the namespace is missing, the variant is compatible but below the higher ranking
        // namespace
        let v1: Variant = serde_json::from_value(json!({
            "cpu": {
                "level": ["x86_64_v1"],
            },
        }))
        .unwrap();
        // The null variant is last.
        let null: Variant = serde_json::from_value(json!({})).unwrap();

        assert_snapshot!(score(&cu128_v2).unwrap(), @"2, 0, 0, 0, 5");
        assert_snapshot!(score(&cu128_v1).unwrap(), @"2, 0, 0, 0, 4");
        assert_snapshot!(score(&cu126_v2).unwrap(), @"1, 0, 0, 0, 5");
        assert_snapshot!(score(&cu126_v1).unwrap(), @"1, 0, 0, 0, 4");
        assert_snapshot!(score(&rocm).unwrap(), @"0, 0, 1, 0, 0");
        assert_snapshot!(score(&xpu).unwrap(), @"0, 0, 0, 1, 0");
        assert_snapshot!(score(&v1).unwrap(), @"0, 0, 0, 0, 4");
        assert_snapshot!(score(&null).unwrap(), @"0, 0, 0, 0, 0");

        let wheels = vec![
            &cu128_v2, &cu128_v1, &cu126_v2, &cu126_v1, &rocm, &xpu, &v1, &null,
        ];
        let mut wheels2 = wheels.clone();
        // "shuffle"
        wheels2.reverse();
        wheels2.sort_by(|a, b| {
            score_variant(&default_priorities(), &host(), a)
                .cmp(&score_variant(&default_priorities(), &host(), b))
                // higher is better
                .reverse()
        });
        assert_eq!(wheels2, wheels);
    }
}
