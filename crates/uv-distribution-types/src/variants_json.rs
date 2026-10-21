use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use itertools::{Either, Itertools};
use serde::{Deserialize, Serialize};
use tracing::debug;

use uv_distribution_filename::VariantLabel;
pub use uv_pep508::{VariantFeature, VariantNamespace, VariantValue};

// TODO(konsti): Standardize on a single schema URL once the variant metadata format is finalized.
/// The PEP 825 draft schema for variant metadata without providers.
pub const PEP_825_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";
/// The PEP 850 draft schema, which adds providers to the variant metadata.
pub const PEP_850_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/850/v0.2.0.json";

const NULL_VARIANT_LABEL: VariantLabel = VariantLabel::from_static("null");

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
        "Expected `variant.json` to describe only the variant `{expected}` from the wheel filename, found {labels:?}",
        labels = .labels.iter().map(ToString::to_string).collect_vec(),
    )]
    WheelLabels {
        expected: VariantLabel,
        labels: Vec<VariantLabel>,
    },
}

/// Variant properties, grouped by namespace and feature.
pub type VariantProperties =
    BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

/// The variant properties used to resolve a wheel's dependencies for an installation target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedVariant {
    pub label: VariantLabel,
    pub properties: VariantProperties,
}

/// Required variant properties unsupported by the target system.
#[derive(Debug)]
pub struct UnsupportedVariantProperties(VariantProperties);

impl fmt::Display for UnsupportedVariantProperties {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first_feature = true;
        for (namespace, features) in &self.0 {
            for (feature, values) in features {
                if first_feature {
                    first_feature = false;
                } else {
                    f.write_str(" and ")?;
                }
                let (open, close) = if values.len() > 1 {
                    ("(", ")")
                } else {
                    ("", "")
                };
                f.write_str(open)?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" or ")?;
                    }
                    write!(f, "`{namespace} :: {feature} :: {value}`")?;
                }
                f.write_str(close)?;
            }
        }
        Ok(())
    }
}

/// Compatible features and values by namespace, ordered from most to least preferred.
///
/// Namespaces are also ordered, but their ordering comes from the package instead of the provider
/// and doesn't need to be captured here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportedVariantProperties {
    properties: BTreeMap<VariantNamespace, Vec<SupportedVariantFeature>>,
}

impl SupportedVariantProperties {
    /// Add the ordered compatible features of a provider.
    pub fn insert(&mut self, namespace: VariantNamespace, features: Vec<SupportedVariantFeature>) {
        self.properties.insert(namespace, features);
    }

    /// Validate a user-provided target description.
    pub fn validate(&self) -> Result<(), VariantMetadataError> {
        for (namespace, features) in &self.properties {
            if !features.iter().map(|(name, _)| name).all_unique() {
                return Err(VariantMetadataError::DuplicateFeatures(namespace.clone()));
            }
            for (feature, values) in features {
                if values.is_empty() {
                    return Err(VariantMetadataError::EmptyValues(
                        namespace.clone(),
                        feature.clone(),
                    ));
                }
                if !values.iter().all_unique() {
                    return Err(VariantMetadataError::DuplicateValues {
                        namespace: namespace.clone(),
                        feature: feature.clone(),
                        values: values.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// User policy for wheel variant providers.
#[derive(Debug, Clone, Default)]
pub struct VariantPolicy {
    target: Option<SupportedVariantProperties>,
}

impl VariantPolicy {
    pub fn new(target: SupportedVariantProperties) -> Self {
        Self {
            target: Some(target),
        }
    }

    /// An explicit target replaces all detected and metadata-provided properties.
    pub fn target(&self) -> Option<&SupportedVariantProperties> {
        self.target.as_ref()
    }

    /// Suppress host detection for a foreign target without explicit properties.
    #[must_use]
    pub fn with_explicit_platform(mut self, explicit: bool) -> Self {
        if explicit && self.target.is_none() {
            self.target = Some(SupportedVariantProperties::default());
        }
        self
    }
}

/// A supported feature and its compatible values, ordered from most to least preferred.
pub type SupportedVariantFeature = (VariantFeature, Vec<VariantValue>);

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
    /// The label when this metadata describes exactly one wheel variant.
    pub fn wheel_label(&self) -> Option<&VariantLabel> {
        self.variants.keys().exactly_one().ok()
    }

    /// Providers declared by the variant metadata.
    pub fn providers(&self) -> impl Iterator<Item = (&VariantNamespace, &VariantProvider)> {
        self.providers.iter().flat_map(|providers| providers.iter())
    }

    /// The variant label, if this describes a single compatible wheel variant.
    pub fn compatible_wheel_label(
        &self,
        supported_properties: &SupportedVariantProperties,
    ) -> Option<&VariantLabel> {
        let label = self.wheel_label()?;
        self.score_variant(label, supported_properties)?;
        Some(label)
    }

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
                labels: metadata.variants.into_keys().collect(),
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
        let mut properties = BTreeSet::new();
        for (namespace, features) in self.variants.get(label)? {
            let namespace_priority = self
                .default_priorities
                .namespace
                .iter()
                .position(|name| name == namespace)?;
            let supported_features = supported_properties.properties.get(namespace)?;
            for (feature, values) in features {
                let (feature_priority, value_priority) =
                    score_feature(supported_features, feature, values)?;
                properties.insert((namespace_priority, feature_priority, value_priority));
            }
        }
        Some(VariantScore {
            // Compare properties in priority order while preferring lower indices.
            properties: properties.into_iter().map(Reverse).collect(),
            label: Reverse(label.clone()),
        })
    }

    /// Return the wheel properties supported by the installation target.
    pub fn select_variant(
        &self,
        label: &VariantLabel,
        supported: &SupportedVariantProperties,
    ) -> Option<SelectedVariant> {
        let mut properties = BTreeMap::new();
        for (namespace, features) in self.variants.get(label)? {
            let supported_features = supported.properties.get(namespace)?;
            let mut selected_features = BTreeMap::new();
            for (feature, values) in features {
                let (_, supported_values) = supported_features
                    .iter()
                    .find(|(supported_feature, _)| supported_feature == feature)?;
                let mut selected_values = values
                    .iter()
                    .filter(|value| supported_values.contains(value))
                    .cloned()
                    .collect::<Vec<_>>();
                selected_values.sort_unstable();
                if selected_values.is_empty() {
                    return None;
                }
                selected_features.insert(feature.clone(), selected_values);
            }
            properties.insert(namespace.clone(), selected_features);
        }
        Some(SelectedVariant {
            label: label.clone(),
            properties,
        })
    }

    /// Verify that selected properties describe a complete supported subset of this wheel.
    pub fn validates_selected_variant(&self, selected: &SelectedVariant) -> bool {
        let Some((label, properties)) = self.variants.iter().exactly_one().ok() else {
            return false;
        };
        if label != &selected.label || !properties.keys().eq(selected.properties.keys()) {
            return false;
        }
        properties.iter().all(|(namespace, features)| {
            let selected_features = &selected.properties[namespace];
            features.keys().eq(selected_features.keys())
                && features.iter().all(|(feature, values)| {
                    let selected_values = &selected_features[feature];
                    !selected_values.is_empty()
                        && selected_values.windows(2).all(|pair| pair[0] < pair[1])
                        && selected_values.iter().all(|value| values.contains(value))
                })
        })
    }

    /// Return the unsupported properties for a variant present in this metadata, if any.
    pub fn unsupported_variant_properties(
        &self,
        label: &VariantLabel,
        supported_properties: &SupportedVariantProperties,
    ) -> Option<UnsupportedVariantProperties> {
        let properties = self.variants.get(label)?;
        let mut unsupported = BTreeMap::new();
        for (namespace, features) in properties {
            let supported_features = supported_properties.properties.get(namespace);
            let unsupported_features = features
                .iter()
                .filter(|(feature, values)| {
                    supported_features
                        .and_then(|supported| score_feature(supported, feature, values))
                        .is_none()
                })
                .map(|(feature, values)| (feature.clone(), values.clone()))
                .collect::<BTreeMap<_, _>>();
            if !unsupported_features.is_empty() {
                unsupported.insert(namespace.clone(), unsupported_features);
            }
        }
        (!unsupported.is_empty()).then_some(UnsupportedVariantProperties(unsupported))
    }

    /// Collect ordered compatible features and values from enabled static providers.
    pub fn static_supported_properties(&self) -> SupportedVariantProperties {
        SupportedVariantProperties {
            properties: self
                .providers()
                .filter(|(_, provider)| !provider.optional)
                .filter_map(|(namespace, provider)| {
                    Some((namespace.clone(), provider.static_properties()?))
                })
                .collect(),
        }
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

/// Return the priority of the first supported value for a feature.
fn score_feature(
    supported_features: &[SupportedVariantFeature],
    feature: &VariantFeature,
    values: &[VariantValue],
) -> Option<(usize, usize)> {
    let (feature_priority, (_, supported_values)) = supported_features
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == feature)?;
    let value_priority = supported_values
        .iter()
        .position(|value| values.contains(value))?;
    Some((feature_priority, value_priority))
}

/// Default priorities for the namespaces used by a release's variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultPriorities {
    namespace: Vec<VariantNamespace>,
}

/// A PEP 850 provider for one variant namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantProvider {
    #[serde(default)]
    optional: bool,
    #[serde(flatten)]
    kind: VariantProviderKind,
}

impl VariantProvider {
    pub fn optional(&self) -> bool {
        self.optional
    }

    pub fn kind(&self) -> &VariantProviderKind {
        &self.kind
    }

    /// Return the static features in preference order.
    pub fn static_properties(&self) -> Option<Vec<SupportedVariantFeature>> {
        let VariantProviderKind::Static {
            static_properties,
            feature_order,
        } = &self.kind
        else {
            return None;
        };
        // Multiple static features require an explicit order because JSON object keys are unordered.
        let features = match feature_order {
            Some(order) => Either::Left(order.iter()),
            None => Either::Right(static_properties.keys()),
        };
        Some(
            features
                .map(|feature| (feature.clone(), static_properties[feature].clone()))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, rename_all_fields = "kebab-case", deny_unknown_fields)]
pub enum VariantProviderKind {
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
pub enum VariantMetadataError {
    #[error("Variant namespace priorities must be non-empty and unique")]
    NamespacePriorities,
    #[error("Variant namespace `{0}` is missing from default-priorities.namespace")]
    MissingNamespace(VariantNamespace),
    #[error(
        "Variant providers must match the namespaces in default-priorities.namespace: expected {expected:?}, found {actual:?}",
        expected = .expected.iter().map(VariantNamespace::as_str).collect_vec(),
        actual = .actual.iter().map(VariantNamespace::as_str).collect_vec(),
    )]
    ProviderNamespaces {
        expected: Vec<VariantNamespace>,
        actual: Vec<VariantNamespace>,
    },
    #[error(
        "Variant provider `{namespace}` must define feature-order for multiple static features: {features:?}",
        features = .features.iter().map(VariantFeature::as_str).collect_vec(),
    )]
    MissingFeatureOrder {
        namespace: VariantNamespace,
        features: Vec<VariantFeature>,
    },
    #[error(
        "Variant provider `{namespace}` has an invalid feature-order: expected a non-empty list containing each static feature in {expected:?} exactly once, found {actual:?}",
        expected = .expected.iter().map(VariantFeature::as_str).collect_vec(),
        actual = .actual.iter().map(VariantFeature::as_str).collect_vec(),
    )]
    InvalidFeatureOrder {
        namespace: VariantNamespace,
        expected: Vec<VariantFeature>,
        actual: Vec<VariantFeature>,
    },
    #[error("Variant feature `{0} :: {1}` must have at least one value")]
    EmptyValues(VariantNamespace, VariantFeature),
    #[error(
        "Variant feature `{namespace} :: {feature}` must not contain duplicate values: {values:?}",
        values = .values.iter().map(VariantValue::as_str).collect_vec(),
    )]
    DuplicateValues {
        namespace: VariantNamespace,
        feature: VariantFeature,
        values: Vec<VariantValue>,
    },
    #[error("Variant provider `{0}` returned duplicate features")]
    DuplicateFeatures(VariantNamespace),
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
                    expected: namespaces.clone(),
                    actual: provider_namespaces.into_iter().cloned().collect(),
                });
            }
        }
        if let Some(providers) = &wire.providers {
            for (namespace, provider) in providers {
                let VariantProviderKind::Static {
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
                            namespace: namespace.clone(),
                            expected: static_properties.keys().cloned().collect(),
                            actual: order.clone(),
                        });
                    }
                    None if static_properties.len() > 1 => {
                        return Err(VariantMetadataError::MissingFeatureOrder {
                            namespace: namespace.clone(),
                            features: static_properties.keys().cloned().collect(),
                        });
                    }
                    Some(_) | None => {}
                }
                for (feature, values) in static_properties {
                    if !values.iter().all_unique() {
                        return Err(VariantMetadataError::DuplicateValues {
                            namespace: namespace.clone(),
                            feature: feature.clone(),
                            values: values.clone(),
                        });
                    }
                }
            }
        }

        for (label, properties) in &mut wire.variants {
            for (namespace, features) in properties.iter_mut() {
                if !features.is_empty() && !namespace_set.contains(namespace) {
                    return Err(VariantMetadataError::MissingNamespace(namespace.clone()));
                }
                for (feature, values) in features {
                    if values.is_empty() {
                        return Err(VariantMetadataError::EmptyValues(
                            namespace.clone(),
                            feature.clone(),
                        ));
                    }
                    if !values.iter().all_unique() {
                        return Err(VariantMetadataError::DuplicateValues {
                            namespace: namespace.clone(),
                            feature: feature.clone(),
                            values: values.clone(),
                        });
                    }
                    if label == &NULL_VARIANT_LABEL {
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
