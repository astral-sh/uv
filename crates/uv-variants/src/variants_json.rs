use std::collections::BTreeMap;
use std::ops::Deref;

use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use uv_distribution_filename::VariantLabel;
use uv_pep508::{
    MarkerTree, MarkerVariantsEnvironment, Requirement, VariantFeature, VariantNamespace,
    VariantValue,
};
use uv_pypi_types::VerbatimParsedUrl;

type VariantProperties = BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

/// Mapping of namespaces in a variant
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "VariantProperties")]
pub struct Variant(VariantProperties);

impl MarkerVariantsEnvironment for Variant {
    fn contains_namespace(&self, namespace: &VariantNamespace) -> bool {
        self.0
            .get(namespace)
            .is_some_and(|features| features.values().any(|values| !values.is_empty()))
    }

    fn contains_feature(&self, namespace: &VariantNamespace, feature: &VariantFeature) -> bool {
        let Some(features) = self.0.get(namespace) else {
            return false;
        };

        let Some(properties) = features.get(feature) else {
            return false;
        };

        !properties.is_empty()
    }

    fn contains_property(
        &self,
        namespace: &VariantNamespace,
        feature: &VariantFeature,
        value: &VariantValue,
    ) -> bool {
        let Some(features) = self.0.get(namespace) else {
            return false;
        };

        let Some(values) = features.get(feature) else {
            return false;
        };

        values.iter().any(|values| values == value)
    }

    fn contains_base_namespace(&self, _prefix: &str, _namespace: &VariantNamespace) -> bool {
        false
    }

    fn contains_base_feature(
        &self,
        _prefix: &str,
        _namespace: &VariantNamespace,
        _feature: &VariantFeature,
    ) -> bool {
        false
    }

    fn contains_base_property(
        &self,
        _prefix: &str,
        _namespace: &VariantNamespace,
        _feature: &VariantFeature,
        _value: &VariantValue,
    ) -> bool {
        false
    }
}

impl Deref for Variant {
    type Target = BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Schema for the draft implemented here. Draft versions do not imply compatibility.
///
/// PEP 825 will replace this URL with one on packaging.python.org when finalized.
pub const VARIANT_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";

#[derive(Debug, thiserror::Error)]
pub enum VariantMetadataError {
    #[error("Unsupported variant metadata schema: {0}")]
    UnsupportedSchema(String),
    #[error("Variant namespace priorities must be non-empty and unique")]
    NamespacePriorities,
    #[error("Variant namespace `{0}` is missing from default-priorities.namespace")]
    MissingNamespace(VariantNamespace),
    #[error("Variant feature `{0} :: {1}` must have at least one value")]
    EmptyValues(VariantNamespace, VariantFeature),
    #[error("Variant feature `{0} :: {1}` must not contain duplicate values")]
    DuplicateValues(VariantNamespace, VariantFeature),
    #[error("The null variant must not have properties")]
    NullProperties,
    #[error("Variant metadata has inconsistent namespace priorities")]
    InconsistentPriorities,
    #[error("Variant label `{0}` has inconsistent properties")]
    InconsistentVariant(VariantLabel),
    #[error("Wheel variant metadata must contain exactly the label `{0}`")]
    WheelLabel(VariantLabel),
}

impl TryFrom<VariantProperties> for Variant {
    type Error = VariantMetadataError;

    fn try_from(mut properties: VariantProperties) -> Result<Self, Self::Error> {
        for (namespace, features) in &mut properties {
            for (feature, values) in features {
                if values.is_empty() {
                    return Err(VariantMetadataError::EmptyValues(
                        namespace.clone(),
                        feature.clone(),
                    ));
                }
                // Values are sets, serialized in lexical order.
                values.sort_unstable();
                let count = values.len();
                values.dedup();
                if values.len() != count {
                    return Err(VariantMetadataError::DuplicateValues(
                        namespace.clone(),
                        feature.clone(),
                    ));
                }
            }
        }
        properties.retain(|_, features| !features.is_empty());
        Ok(Self(properties))
    }
}

impl Variant {
    pub(crate) fn retain_values(
        &mut self,
        mut supported: impl FnMut(&VariantNamespace, &VariantFeature, &VariantValue) -> bool,
    ) {
        self.0.retain(|namespace, features| {
            features.retain(|feature, values| {
                values.retain(|value| supported(namespace, feature, value));
                !values.is_empty()
            });
            !features.is_empty()
        });
    }

    fn has_properties(&self) -> bool {
        self.0
            .values()
            .any(|features| features.values().any(|values| !values.is_empty()))
    }
}

/// Combined index metadata for wheel variants, as defined by PEP 825.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", try_from = "VariantsJsonWire")]
pub struct VariantsJsonContent {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub default_priorities: DefaultPriorities,
    /// Provider discovery is a prototype extension, outside PEP 825.
    #[serde(skip_serializing_if = "FxHashMap::is_empty")]
    pub providers: FxHashMap<VariantNamespace, Provider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub static_properties:
        Option<BTreeMap<VariantNamespace, IndexMap<VariantFeature, Vec<VariantValue>>>>,
    pub variants: BTreeMap<VariantLabel, Variant>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VariantsJsonWire {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    #[serde(default)]
    providers: FxHashMap<VariantNamespace, Provider>,
    static_properties:
        Option<BTreeMap<VariantNamespace, IndexMap<VariantFeature, Vec<VariantValue>>>>,
    variants: BTreeMap<VariantLabel, Variant>,
}

impl TryFrom<VariantsJsonWire> for VariantsJsonContent {
    type Error = VariantMetadataError;

    fn try_from(wire: VariantsJsonWire) -> Result<Self, Self::Error> {
        let content = Self {
            schema: wire.schema,
            default_priorities: wire.default_priorities,
            providers: wire.providers,
            static_properties: wire.static_properties,
            variants: wire.variants,
        };
        content.validate()?;
        Ok(content)
    }
}

impl VariantsJsonContent {
    pub fn validate(&self) -> Result<(), VariantMetadataError> {
        if self.schema != VARIANT_SCHEMA {
            return Err(VariantMetadataError::UnsupportedSchema(self.schema.clone()));
        }
        let namespaces = &self.default_priorities.namespace;
        if namespaces.is_empty()
            || namespaces.iter().collect::<FxHashSet<_>>().len() != namespaces.len()
        {
            return Err(VariantMetadataError::NamespacePriorities);
        }
        for (label, variant) in &self.variants {
            if label.as_str() == "null" && variant.has_properties() {
                return Err(VariantMetadataError::NullProperties);
            }
            for namespace in variant.keys() {
                if !namespaces.contains(namespace) {
                    return Err(VariantMetadataError::MissingNamespace(namespace.clone()));
                }
            }
        }
        Ok(())
    }

    /// Verify that metadata read from a wheel names that wheel's variant.
    pub fn validate_wheel(&self, label: &VariantLabel) -> Result<(), VariantMetadataError> {
        self.validate()?;
        if self.variants.len() != 1 || !self.variants.contains_key(label) {
            return Err(VariantMetadataError::WheelLabel(label.clone()));
        }
        Ok(())
    }

    /// Combine consistent metadata, independently of the order of its sources.
    pub fn merge(&mut self, other: &Self) -> Result<(), VariantMetadataError> {
        self.validate()?;
        other.validate()?;
        let namespaces = &self.default_priorities.namespace;
        let other_namespaces = &other.default_priorities.namespace;
        if !namespaces.starts_with(other_namespaces) && !other_namespaces.starts_with(namespaces) {
            return Err(VariantMetadataError::InconsistentPriorities);
        }
        for (label, variant) in &other.variants {
            if self
                .variants
                .get(label)
                .is_some_and(|current| current != variant)
            {
                return Err(VariantMetadataError::InconsistentVariant(label.clone()));
            }
        }
        if other_namespaces.len() > namespaces.len() {
            self.default_priorities
                .namespace
                .clone_from(other_namespaces);
        }
        self.variants.extend(other.variants.clone());
        Ok(())
    }

    /// Return just the standardized fields when writing an interoperable lockfile.
    #[must_use]
    pub fn without_provider_extensions(&self) -> Self {
        Self {
            providers: FxHashMap::default(),
            static_properties: None,
            ..self.clone()
        }
    }
}

/// A `{name}-{version}.dist-info/variant.json` file.
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(clippy::zero_sized_map_values)]
pub struct DistInfoVariantsJson {
    pub variants: FxHashMap<VariantLabel, serde::de::IgnoredAny>,
}

impl DistInfoVariantsJson {
    /// Returns the label for the current variant.
    pub fn label(&self) -> Option<&VariantLabel> {
        let mut keys = self.variants.keys();
        let label = keys.next()?;
        if keys.next().is_some() {
            None
        } else {
            Some(label)
        }
    }
}

/// Default provider priorities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DefaultPriorities {
    /// Default namespace priorities
    pub namespace: Vec<VariantNamespace>,
}

/// A `namespace :: feature :: property` entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantPropertyType {
    pub namespace: VariantNamespace,
    pub feature: VariantFeature,
    pub value: VariantValue,
}

/// Provider information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Provider {
    /// Environment marker specifying when to enable the plugin.
    #[serde(
        skip_serializing_if = "uv_pep508::marker::ser::is_empty",
        serialize_with = "uv_pep508::marker::ser::serialize",
        default
    )]
    pub enable_if: MarkerTree,
    /// Whether this is an install-time provider. `false` means that it is an `AoT` provider instead.
    ///
    /// Defaults to `true`
    pub install_time: Option<bool>,
    /// Whether this is an optional provider.
    ///
    /// If it is `true`, the provider is not used unless the user opts in to it.
    ///
    /// Defaults to `false`
    #[serde(default)]
    pub optional: bool,
    /// Object reference to plugin class
    pub plugin_api: Option<String>,
    /// Dependency specifiers for how to install the plugin
    pub requires: Option<Vec<Requirement<VerbatimParsedUrl>>>,
}

#[cfg(test)]
mod tests {
    use super::{VARIANT_SCHEMA, VariantsJsonContent};
    use insta::assert_snapshot;
    use serde_json::json;

    #[test]
    fn pep825_metadata_validation() -> Result<(), serde_json::Error> {
        let metadata = json!({
            "$schema": VARIANT_SCHEMA,
            "default-priorities": {"namespace": ["cpu"]},
            "variants": {"fast": {"cpu": {"level": ["v3"]}}, "null": {}}
        });
        let mut errors = Vec::new();
        for (field, value) in [
            (
                "/$schema",
                json!("https://variants-schema.wheelnext.dev/peps/825/v0.1.0.json"),
            ),
            ("/default-priorities/namespace", json!([])),
            ("/default-priorities/namespace", json!(["cpu", "cpu"])),
            ("/default-priorities/namespace", json!(["gpu"])),
            ("/variants/fast/cpu/level", json!([])),
            ("/variants/null", json!({"cpu": {"level": ["v3"]}})),
        ] {
            let mut invalid = metadata.clone();
            if let Some(destination) = invalid.pointer_mut(field) {
                *destination = value;
            }
            errors.push(
                serde_json::from_value::<VariantsJsonContent>(invalid)
                    .expect_err("invalid metadata")
                    .to_string(),
            );
        }
        assert_snapshot!(errors.join("\n"), @r#"
        Unsupported variant metadata schema: https://variants-schema.wheelnext.dev/peps/825/v0.1.0.json
        Variant namespace priorities must be non-empty and unique
        Variant namespace priorities must be non-empty and unique
        Variant namespace `cpu` is missing from default-priorities.namespace
        Variant feature `cpu :: level` must have at least one value
        The null variant must not have properties
        "#);
        let valid: VariantsJsonContent = serde_json::from_value(metadata)?;
        assert!(valid.providers.is_empty());
        Ok(())
    }

    #[test]
    fn pep825_metadata_merge() -> Result<(), Box<dyn std::error::Error>> {
        let first: VariantsJsonContent = serde_json::from_value(json!({
            "$schema": VARIANT_SCHEMA,
            "default-priorities": {"namespace": ["cpu"]},
            "variants": {"fast": {"cpu": {"level": ["v3", "v2"]}}}
        }))?;
        let second: VariantsJsonContent = serde_json::from_value(json!({
            "$schema": VARIANT_SCHEMA,
            "default-priorities": {"namespace": ["cpu", "gpu"]},
            "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
        }))?;
        let mut forward = first.clone();
        forward.merge(&second)?;
        let mut backward = second.clone();
        backward.merge(&first)?;
        assert_eq!(forward, backward);
        assert_eq!(forward.default_priorities, second.default_priorities);
        assert_eq!(forward.variants.len(), 2);
        assert!(first.validate_wheel(&"fast".parse()?).is_ok());
        assert!(first.validate_wheel(&"other".parse()?).is_err());
        assert!(forward.validate_wheel(&"fast".parse()?).is_err());
        let mut inconsistent = second.clone();
        inconsistent.default_priorities.namespace.reverse();
        assert!(forward.merge(&inconsistent).is_err());
        inconsistent = second;
        inconsistent.variants.insert(
            "fast".parse()?,
            serde_json::from_value(json!({"cpu": {"level": ["v4"]}}))?,
        );
        assert!(forward.merge(&inconsistent).is_err());
        Ok(())
    }
}
