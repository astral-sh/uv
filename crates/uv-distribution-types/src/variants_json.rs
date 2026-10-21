use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use itertools::{Either, Itertools};
use serde::{Deserialize, Serialize};
use tracing::debug;

use uv_distribution_filename::VariantLabel;

// TODO(konsti): Standardize on a single schema URL once the variant metadata format is finalized.
/// The PEP 825 draft schema for variant metadata without providers.
pub const PEP_825_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";
/// The PEP 850 draft schema, which adds providers to the variant metadata.
pub const PEP_850_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/850/v0.2.0.json";

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
    /// A wheel's variant metadata does not describe exactly the variant named in its filename.
    #[error(
        "Expected `variant.json` to describe only the variant `{expected}` from the wheel filename, found {labels:?}"
    )]
    WheelLabels {
        expected: VariantLabel,
        labels: Vec<String>,
    },
}

/// Variant properties, grouped by namespace and feature.
type VariantProperties = BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

/// Compatible features and values by namespace, ordered from most to least preferred.
///
/// Namespaces are also ordered, but their ordering comes from the package instead of the provider
/// and doesn't need to be captured here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportedVariantProperties {
    properties: BTreeMap<VariantNamespace, Vec<(VariantFeature, Vec<VariantValue>)>>,
}

/// The priority of a compatible variant. Higher scores are preferred.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VariantScore {
    properties: Vec<Reverse<(usize, usize, usize)>>,
    label: Reverse<VariantLabel>,
}

/// Wheel variant metadata, as defined by PEP 825 and extended with providers by PEP 850.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", try_from = "VariantsJsonWire")]
pub struct VariantsJsonContent {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    #[serde(skip_serializing_if = "Option::is_none")]
    providers: Option<BTreeMap<VariantNamespace, VariantProvider>>,
    variants: BTreeMap<VariantLabel, VariantProperties>,
}

impl VariantsJsonContent {
    /// Parse a wheel's `variant.json`, validating its label against the wheel filename.
    ///
    /// PEP 825 requires a wheel to describe exactly the variant named in its filename.
    /// The index's `variants.json` lists every variant of the release.
    pub fn from_wheel(
        contents: &[u8],
        label: &VariantLabel,
    ) -> Result<Self, VariantsJsonParseError> {
        let metadata = Self::from_slice(contents)?;
        // TODO(konsti): Check that this label has the same set of properties in the wheel's
        // `variant.json` and the index's `variants.json`.
        if metadata.variants.keys().eq(std::iter::once(label)) {
            Ok(metadata)
        } else {
            Err(VariantsJsonParseError::WheelLabels {
                expected: label.clone(),
                labels: metadata.variants.keys().map(ToString::to_string).collect(),
            })
        }
    }

    /// Score a variant using the target's ordered compatible features and values.
    ///
    /// Returns `None` if the label is missing or any of its features is unsupported.
    pub fn score_variant(
        &self,
        label: &VariantLabel,
        supported_properties: &SupportedVariantProperties,
    ) -> Option<VariantScore> {
        // Note(konsti): This implementation doesn't have any optimizations yet, we don't know the
        // shape that big variant wheels will have so we use the easiest to read form.
        let mut properties = BTreeSet::new();
        for (namespace, features) in self.variants.get(label)? {
            let namespace_priority = self
                .default_priorities
                .namespace
                .iter()
                .position(|name| name == namespace)?;
            let supported_features = supported_properties.properties.get(namespace)?;
            for (feature, values) in features {
                let (feature_priority, (_name, supported_values)) = supported_features
                    .iter()
                    .enumerate()
                    .find(|(_feature_priority, (name, _supported_values))| name == feature)?;
                let value_priority = supported_values
                    .iter()
                    .position(|value| values.contains(value))?;
                properties.insert((namespace_priority, feature_priority, value_priority));
            }
        }
        Some(VariantScore {
            // Compare properties in priority order while preferring lower indices.
            properties: properties.into_iter().map(Reverse).collect(),
            label: Reverse(label.clone()),
        })
    }

    /// Collect ordered compatible features and values from enabled static providers.
    pub fn static_supported_properties(&self) -> SupportedVariantProperties {
        let Some(providers) = &self.providers else {
            return SupportedVariantProperties::default();
        };
        let properties = providers
            .iter()
            .filter(|(_, provider)| !provider.optional)
            .filter_map(|(namespace, provider)| {
                let ProviderKind::Static {
                    static_properties,
                    feature_order,
                } = &provider.kind
                else {
                    return None;
                };
                // PEP 850 requires feature-order for multiple features because JSON object keys
                // are unordered. With at most one feature, the order is unambiguous.
                let features = match feature_order {
                    Some(order) => Either::Left(order.iter()),
                    None => Either::Right(static_properties.keys()),
                };
                let supported = features
                    .map(|feature| (feature.clone(), static_properties[feature].clone()))
                    .collect();
                Some((namespace.clone(), supported))
            })
            .collect();
        SupportedVariantProperties { properties }
    }

    /// Parse and validate the contents of a variant metadata file.
    pub fn from_slice(contents: &[u8]) -> Result<Self, VariantsJsonParseError> {
        match serde_json::from_slice(contents) {
            Ok(metadata) => Ok(metadata),
            Err(source) => {
                if let Ok(metadata) = serde_json::from_slice::<VariantsJsonSchema>(contents)
                    && !matches!(metadata.schema.as_str(), PEP_825_SCHEMA | PEP_850_SCHEMA)
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

/// A PEP 850 provider for one variant namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VariantProvider {
    #[serde(default)]
    optional: bool,
    #[serde(flatten)]
    kind: ProviderKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, rename_all_fields = "kebab-case", deny_unknown_fields)]
enum ProviderKind {
    Static {
        static_properties: BTreeMap<VariantFeature, Vec<VariantValue>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        feature_order: Option<Vec<VariantFeature>>,
    },
    Plugin {
        requires: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin_api: Option<String>,
    },
    Builtin {
        builtin: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct VariantsJsonWire {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    providers: Option<BTreeMap<VariantNamespace, VariantProvider>>,
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
    #[error(
        "Variant providers must match the namespaces in default-priorities.namespace: expected {expected:?}, found {actual:?}"
    )]
    ProviderNamespaces {
        expected: Vec<String>,
        actual: Vec<String>,
    },
    #[error(
        "Variant provider `{namespace}` must define feature-order for multiple static features: {features:?}"
    )]
    MissingFeatureOrder {
        namespace: String,
        features: Vec<String>,
    },
    #[error(
        "Variant provider `{namespace}` has an invalid feature-order: expected a non-empty list containing each static feature in {expected:?} exactly once, found {actual:?}"
    )]
    InvalidFeatureOrder {
        namespace: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },
    #[error("Variant feature `{0} :: {1}` must have at least one value")]
    EmptyValues(String, String),
    #[error("Variant feature `{0} :: {1}` must not contain duplicate values: {2:?}")]
    DuplicateValues(String, String, Vec<String>),
    #[error("The null variant must not have properties")]
    NullProperties,
}

impl TryFrom<VariantsJsonWire> for VariantsJsonContent {
    type Error = VariantMetadataError;

    fn try_from(mut wire: VariantsJsonWire) -> Result<Self, Self::Error> {
        let namespaces = &wire.default_priorities.namespace;
        let namespace_set: BTreeSet<_> = namespaces.iter().collect();
        if namespaces.is_empty() || namespace_set.len() != namespaces.len() {
            return Err(VariantMetadataError::NamespacePriorities);
        }

        // The provider checks below validate metadata only. Supported properties are collected
        // separately and passed to `score_variant` to determine compatibility.
        if wire.providers.is_some() || wire.schema == PEP_850_SCHEMA {
            let provider_namespaces = wire
                .providers
                .iter()
                .flat_map(|providers| providers.keys())
                .collect::<BTreeSet<_>>();
            if provider_namespaces != namespace_set {
                return Err(VariantMetadataError::ProviderNamespaces {
                    expected: namespaces
                        .iter()
                        .map(|namespace| namespace.0.clone())
                        .collect(),
                    actual: provider_namespaces
                        .into_iter()
                        .map(|namespace| namespace.0.clone())
                        .collect(),
                });
            }
        }
        if let Some(providers) = &wire.providers {
            for (namespace, provider) in providers {
                let ProviderKind::Static {
                    static_properties,
                    feature_order,
                } = &provider.kind
                else {
                    continue;
                };
                match feature_order {
                    Some(order)
                        if order.is_empty()
                            || !order.iter().sorted().eq(static_properties.keys()) =>
                    {
                        return Err(VariantMetadataError::InvalidFeatureOrder {
                            namespace: namespace.0.clone(),
                            expected: static_properties
                                .keys()
                                .map(|feature| feature.0.clone())
                                .collect(),
                            actual: order.iter().map(|feature| feature.0.clone()).collect(),
                        });
                    }
                    None if static_properties.len() > 1 => {
                        return Err(VariantMetadataError::MissingFeatureOrder {
                            namespace: namespace.0.clone(),
                            features: static_properties
                                .keys()
                                .map(|feature| feature.0.clone())
                                .collect(),
                        });
                    }
                    Some(_) | None => {}
                }
                for (feature, values) in static_properties {
                    if !values.iter().all_unique() {
                        return Err(VariantMetadataError::DuplicateValues(
                            namespace.0.clone(),
                            feature.0.clone(),
                            values.iter().map(|value| value.0.clone()).collect(),
                        ));
                    }
                }
            }
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
                    if !values.iter().all_unique() {
                        return Err(VariantMetadataError::DuplicateValues(
                            namespace.0.clone(),
                            feature.0.clone(),
                            values.iter().map(|value| value.0.clone()).collect(),
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
        if !matches!(wire.schema.as_str(), PEP_825_SCHEMA | PEP_850_SCHEMA) {
            debug!("Unknown variant metadata schema `{}`", wire.schema);
        }

        Ok(Self {
            schema: wire.schema,
            default_priorities: wire.default_priorities,
            providers: wire.providers,
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
