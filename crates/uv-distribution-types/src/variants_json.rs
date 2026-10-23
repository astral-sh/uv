use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::str::FromStr;

use itertools::{Either, Itertools};
use serde::{Deserialize, Serialize};
use tracing::debug;

use uv_distribution_filename::VariantLabel;

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
type VariantProperties = BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

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
                    write!(f, "`{namespace} :: {} :: {}`", feature.0, value.0)?;
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
    properties: BTreeMap<VariantNamespace, Vec<(VariantFeature, Vec<VariantValue>)>>,
}

/// A supported feature and its compatible values, ordered from most to least preferred.
#[derive(Debug, Clone)]
pub struct SupportedVariantFeature {
    pub name: VariantFeature,
    pub values: Vec<VariantValue>,
}

/// A non-static provider requested by variant metadata.
#[derive(Clone, Copy)]
pub enum VariantProviderRequest<'a> {
    Plugin {
        requires: &'a [String],
        plugin_api: Option<&'a str>,
    },
    Builtin(&'a str),
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

    /// Collect compatible features from static and detected providers.
    pub fn supported_properties<'a>(
        &self,
        detected: impl IntoIterator<Item = (&'a VariantNamespace, &'a [SupportedVariantFeature])>,
    ) -> SupportedVariantProperties {
        let mut supported = self.static_supported_properties();
        for (namespace, features) in detected {
            let features = features
                .iter()
                .map(|feature| (feature.name.clone(), feature.values.clone()))
                .collect();
            supported.properties.insert(namespace.clone(), features);
        }
        supported
    }

    /// Return the enabled providers that need properties to be detected.
    pub fn provider_requests(
        &self,
    ) -> impl Iterator<Item = (&VariantNamespace, VariantProviderRequest<'_>)> {
        self.providers
            .iter()
            .flat_map(|providers| providers.iter())
            .filter_map(|(namespace, provider)| {
                if provider.optional {
                    return None;
                }
                let request = match &provider.kind {
                    ProviderKind::Plugin {
                        requires,
                        plugin_api,
                    } => VariantProviderRequest::Plugin {
                        requires,
                        plugin_api: plugin_api.as_deref(),
                    },
                    ProviderKind::Builtin { builtin } => VariantProviderRequest::Builtin(builtin),
                    ProviderKind::Static { .. } => return None,
                };
                Some((namespace, request))
            })
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
    supported_features: &[(VariantFeature, Vec<VariantValue>)],
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
pub enum VariantMetadataError {
    #[error("Invalid variant namespace: `{0}`")]
    InvalidNamespace(String),
    #[error("Invalid variant feature: `{0}`")]
    InvalidFeature(String),
    #[error("Invalid variant value: `{0}`")]
    InvalidValue(String),
    #[error("Variant namespace priorities must be non-empty and unique")]
    NamespacePriorities,
    #[error("Variant namespace `{0}` is missing from default-priorities.namespace")]
    MissingNamespace(VariantNamespace),
    #[error(
        "Variant providers must match the namespaces in default-priorities.namespace: expected {expected:?}, found {actual:?}",
        expected = .expected.iter().map(|namespace| &namespace.0).collect_vec(),
        actual = .actual.iter().map(|namespace| &namespace.0).collect_vec(),
    )]
    ProviderNamespaces {
        expected: Vec<VariantNamespace>,
        actual: Vec<VariantNamespace>,
    },
    #[error(
        "Variant provider `{namespace}` must define feature-order for multiple static features: {features:?}",
        features = .features.iter().map(|feature| &feature.0).collect_vec(),
    )]
    MissingFeatureOrder {
        namespace: VariantNamespace,
        features: Vec<VariantFeature>,
    },
    #[error(
        "Variant provider `{namespace}` has an invalid feature-order: expected a non-empty list containing each static feature in {expected:?} exactly once, found {actual:?}",
        expected = .expected.iter().map(|feature| &feature.0).collect_vec(),
        actual = .actual.iter().map(|feature| &feature.0).collect_vec(),
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
        values = .values.iter().map(|value| &value.0).collect_vec(),
    )]
    DuplicateValues {
        namespace: VariantNamespace,
        feature: VariantFeature,
        values: Vec<VariantValue>,
    },
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

/// A validated variant namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantNamespace(Cow<'static, str>);

impl VariantNamespace {
    /// Create a variant namespace from a static string.
    ///
    /// An invalid namespace causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, false), "invalid variant namespace");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantNamespace {
    type Err = VariantMetadataError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantNamespace {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantMetadataError::InvalidNamespace(value))
        }
    }
}

/// A validated variant feature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantFeature(Cow<'static, str>);

impl VariantFeature {
    /// Create a variant feature from a static string.
    ///
    /// An invalid feature causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, false), "invalid variant feature");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantFeature {
    type Err = VariantMetadataError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantFeature {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantMetadataError::InvalidFeature(value))
        }
    }
}

/// A validated variant value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantValue(Cow<'static, str>);

impl VariantValue {
    /// Create a variant value from a static string.
    ///
    /// An invalid value causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, true), "invalid variant value");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantValue {
    type Err = VariantMetadataError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantValue {
    type Error = VariantMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, true) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantMetadataError::InvalidValue(value))
        }
    }
}

const fn valid_component(value: &str, allow_period: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let character = bytes[index];
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == b'_'
            || (allow_period && character == b'.'))
        {
            return false;
        }
        index += 1;
    }
    true
}
