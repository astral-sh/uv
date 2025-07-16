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
/// A wrapper around [`NonZeroU32`]. Higher values indicate higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariantPriority {
    NoVariant,
    Variant,
}

/// Return a priority score for the variant (higher is better) or `None` if it isn't compatible.
pub fn score_variant(
    default_priorities: &DefaultPriorities,
    target_namespaces: &FxHashMap<VariantNamespace, VariantProviderOutput>,
    variants_properties: &Variant,
) -> Option<Vec<usize>> {
    for (namespace, features) in variants_properties {
        for (feature, properties) in features {
            let Some(resolved_properties) = target_namespaces
                .get(namespace)
                .and_then(|namespace| namespace.features.get(feature))
            else {
                return None;
            };
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
            if !value_priorities.contains(feature) {
                debug!("missing value priority {feature}");
            }
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
