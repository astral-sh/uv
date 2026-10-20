use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use tracing::debug;

use uv_distribution_filename::VariantLabel;

// TODO(konsti): Update the schema URL once PEP 825 is finalized.
/// The schema for the PEP 825 draft.
const VARIANT_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";

/// An error parsing a variant metadata file.
#[derive(Debug, thiserror::Error)]
pub enum VariantsJsonParseError {
    /// The metadata could not be parsed and uses an unknown schema.
    #[error("Unknown variant metadata schema:\n  `{schema}`:\n    {source}")]
    UnknownSchema {
        schema: String,
        #[source]
        source: serde_json::Error,
    },
    /// The metadata is not valid JSON or cannot be deserialized.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Variant properties, grouped by namespace and feature.
type VariantProperties = BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

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
        match serde_json::from_slice(contents) {
            Ok(metadata) => Ok(metadata),
            Err(source) => {
                if let Ok(metadata) = serde_json::from_slice::<VariantsJsonSchema>(contents)
                    && metadata.schema != VARIANT_SCHEMA
                {
                    debug!("Unknown variant metadata schema `{}`", metadata.schema);
                    return Err(VariantsJsonParseError::UnknownSchema {
                        schema: metadata.schema,
                        source,
                    });
                }
                Err(VariantsJsonParseError::Json(source))
            }
        }
    }
}

/// Default priorities for the namespaces used by a release's variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultPriorities {
    namespace: Vec<VariantNamespace>,
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
        let namespaces = &wire.default_priorities.namespace;
        let namespace_set: HashSet<_> = namespaces.iter().collect();
        if namespaces.is_empty() || namespace_set.len() != namespaces.len() {
            return Err(VariantMetadataError::NamespacePriorities);
        }

        for (label, properties) in &mut wire.variants {
            for (namespace, features) in properties.iter_mut() {
                if !features.is_empty() && !namespace_set.contains(namespace) {
                    return Err(VariantMetadataError::MissingNamespace(namespace.0.clone()));
                }
                for (feature, values) in features {
                    if values.is_empty() {
                        return Err(VariantMetadataError::EmptyValues(
                            namespace.0.clone(),
                            feature.0.clone(),
                        ));
                    }
                    if values.iter().collect::<HashSet<_>>().len() != values.len() {
                        return Err(VariantMetadataError::DuplicateValues(
                            namespace.0.clone(),
                            feature.0.clone(),
                        ));
                    }
                    if label.as_str() == "null" {
                        return Err(VariantMetadataError::NullProperties);
                    }
                }
            }
            properties.retain(|_, features| !features.is_empty());
        }

        // Parse leniently for forward compatibility: accept unknown schema versions when their
        // contents pass validation.
        if wire.schema != VARIANT_SCHEMA {
            debug!("Unknown variant metadata schema `{}`", wire.schema);
        }

        Ok(Self {
            schema: wire.schema,
            default_priorities: wire.default_priorities,
            variants: wire.variants,
        })
    }
}

/// The schema field alone, used to identify unknown schemas when parsing fails.
#[derive(Deserialize)]
struct VariantsJsonSchema {
    #[serde(rename = "$schema")]
    schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct VariantNamespace(String);

impl TryFrom<String> for VariantNamespace {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(value))
        } else {
            Err(VariantMetadataError::InvalidNamespace(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct VariantFeature(String);

impl TryFrom<String> for VariantFeature {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(value))
        } else {
            Err(VariantMetadataError::InvalidFeature(value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct VariantValue(String);

impl TryFrom<String> for VariantValue {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, true) {
            Ok(Self(value))
        } else {
            Err(VariantMetadataError::InvalidValue(value))
        }
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
