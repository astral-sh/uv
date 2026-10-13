use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use uv_distribution_filename::VariantLabel;

/// The schema for the PEP 825 draft. Compatibility between draft versions is not guaranteed.
const VARIANT_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";

/// An error parsing a variant metadata file.
pub type VariantsJsonParseError = serde_json::Error;

/// Variant properties, grouped by namespace and feature.
type VariantProperties = BTreeMap<String, BTreeMap<String, Vec<String>>>;

/// Index-level metadata for wheel variants, as defined by PEP 825.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", try_from = "VariantsJsonWire")]
pub struct VariantsJsonContent {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    variants: BTreeMap<VariantLabel, VariantProperties>,
}

impl VariantsJsonContent {
    /// Parse and validate the contents of a variant metadata file.
    pub fn from_slice(contents: &[u8]) -> Result<Self, VariantsJsonParseError> {
        serde_json::from_slice(contents)
    }
}

/// Default priorities for the namespaces used by a release's variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultPriorities {
    namespace: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct VariantsJsonWire {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    variants: BTreeMap<VariantLabel, VariantProperties>,
}

#[derive(Debug, thiserror::Error)]
enum VariantMetadataError {
    #[error("Unsupported variant metadata schema: {0}")]
    UnsupportedSchema(String),
    #[error("Invalid variant namespace: `{0}`")]
    InvalidNamespace(String),
    #[error("Invalid variant feature: `{0}`")]
    InvalidFeature(String),
    #[error("Invalid variant value: `{0}`")]
    InvalidValue(String),
    #[error("Variant namespace priorities must be non-empty and unique")]
    NamespacePriorities,
    #[error("Variant namespace `{0}` is missing from default-priorities.namespace")]
    MissingNamespace(String),
    #[error("Variant feature `{0} :: {1}` must have at least one value")]
    EmptyValues(String, String),
    #[error("Variant feature `{0} :: {1}` must not contain duplicate values")]
    DuplicateValues(String, String),
    #[error("The null variant must not have properties")]
    NullProperties,
}

impl TryFrom<VariantsJsonWire> for VariantsJsonContent {
    type Error = VariantMetadataError;

    fn try_from(mut wire: VariantsJsonWire) -> Result<Self, Self::Error> {
        if wire.schema != VARIANT_SCHEMA {
            return Err(VariantMetadataError::UnsupportedSchema(wire.schema));
        }

        let namespaces = &wire.default_priorities.namespace;
        if namespaces.is_empty()
            || namespaces.iter().collect::<HashSet<_>>().len() != namespaces.len()
        {
            return Err(VariantMetadataError::NamespacePriorities);
        }
        for namespace in namespaces {
            if !valid_component(namespace, false) {
                return Err(VariantMetadataError::InvalidNamespace(namespace.clone()));
            }
        }

        for (label, properties) in &mut wire.variants {
            for (namespace, features) in properties.iter_mut() {
                if !valid_component(namespace, false) {
                    return Err(VariantMetadataError::InvalidNamespace(namespace.clone()));
                }
                if !features.is_empty() && !namespaces.contains(namespace) {
                    return Err(VariantMetadataError::MissingNamespace(namespace.clone()));
                }
                for (feature, values) in features {
                    if !valid_component(feature, false) {
                        return Err(VariantMetadataError::InvalidFeature(feature.clone()));
                    }
                    if values.is_empty() {
                        return Err(VariantMetadataError::EmptyValues(
                            namespace.clone(),
                            feature.clone(),
                        ));
                    }
                    for value in values.iter() {
                        if !valid_component(value, true) {
                            return Err(VariantMetadataError::InvalidValue(value.clone()));
                        }
                    }
                    values.sort_unstable();
                    if values.windows(2).any(|pair| pair[0] == pair[1]) {
                        return Err(VariantMetadataError::DuplicateValues(
                            namespace.clone(),
                            feature.clone(),
                        ));
                    }
                    if label.as_str() == "null" {
                        return Err(VariantMetadataError::NullProperties);
                    }
                }
            }
            properties.retain(|_, features| !features.is_empty());
        }

        Ok(Self {
            schema: wire.schema,
            default_priorities: wire.default_priorities,
            variants: wire.variants,
        })
    }
}

fn valid_component(value: &str, allow_period: bool) -> bool {
    !value.is_empty()
        && value.bytes().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == b'_'
                || (allow_period && character == b'.')
        })
}
