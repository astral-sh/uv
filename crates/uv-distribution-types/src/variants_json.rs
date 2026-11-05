use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use itertools::{Either, Itertools};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::debug;

use uv_distribution_filename::VariantLabel;
use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError, VersionSpecifier};
use uv_pep508::MarkerVariant;
pub use uv_pep508::{VariantFeature, VariantNamespace, VariantProperties, VariantValue};
use version_ranges::Ranges;

use crate::IndexUrl;

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

/// The variant properties used to resolve a wheel's dependencies for an installation target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectedVariant {
    pub label: VariantLabel,
    /// The subset supported by the target, used to evaluate dependency markers.
    pub properties: VariantProperties,
    /// All properties for this label in the metadata used to select the wheel.
    #[serde(skip)]
    declared_properties: VariantProperties,
}

/// The target-specific part of the selection stored in `uv_variant.json`.
#[derive(Deserialize)]
pub(crate) struct RecordedVariant {
    label: VariantLabel,
    properties: VariantProperties,
}

impl SelectedVariant {
    /// Return the selected wheel as seen by dependency markers.
    pub fn marker_environment(selected: Option<&Self>) -> MarkerVariant<'_> {
        match selected {
            Some(selected) => MarkerVariant::Selected {
                label: selected.label.as_str(),
                properties: &selected.properties,
            },
            None => MarkerVariant::Ordinary,
        }
    }
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
    #[serde(deserialize_with = "deserialize_unique_map")]
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
                validate_feature_values(namespace, feature, values)?;
            }
        }
        Ok(())
    }
}

/// User policy for wheel variant providers.
#[derive(Debug, Clone, Default)]
pub struct VariantPolicy {
    target: Option<SupportedVariantProperties>,
    explicit_platform: bool,
    enabled: BTreeMap<VariantNamespace, VariantProviderOptIn>,
    provider_index: Option<IndexUrl>,
}

impl VariantPolicy {
    pub fn new(
        target: Option<SupportedVariantProperties>,
        enabled: impl IntoIterator<Item = VariantProviderOptIn>,
        provider_index: Option<IndexUrl>,
    ) -> Self {
        Self {
            target,
            explicit_platform: false,
            enabled: enabled
                .into_iter()
                .map(|opt_in| (opt_in.namespace.clone(), opt_in))
                .collect(),
            provider_index,
        }
    }

    /// An explicit target replaces all detected and metadata-provided properties.
    pub fn target(&self) -> Option<&SupportedVariantProperties> {
        self.target.as_ref()
    }

    /// Whether the user explicitly enabled this provider namespace.
    pub fn enables(&self, namespace: &VariantNamespace) -> bool {
        self.enabled.contains_key(namespace)
    }

    pub fn opt_in(&self, namespace: &VariantNamespace) -> Option<&VariantProviderOptIn> {
        self.enabled.get(namespace)
    }

    /// The user-authorized index for external provider packages and their dependencies.
    pub fn provider_index(&self) -> Option<&IndexUrl> {
        self.provider_index.as_ref()
    }

    /// Whether provider selection should use only platform defaults.
    pub fn explicit_platform(&self) -> bool {
        self.explicit_platform
    }

    /// Suppress host detection for a foreign target.
    #[must_use]
    pub fn with_explicit_platform(mut self, explicit: bool) -> Self {
        self.explicit_platform = explicit;
        self
    }
}

/// Explicit authorization for a variant provider namespace and, optionally, plugin identity.
#[derive(Debug, Clone)]
pub struct VariantProviderOptIn {
    namespace: VariantNamespace,
    packages: Vec<PackageName>,
    endpoint: Option<String>,
}

impl VariantProviderOptIn {
    pub fn authorizes<'a>(
        &self,
        packages: impl IntoIterator<Item = &'a PackageName>,
        endpoint: &str,
    ) -> bool {
        !self.packages.is_empty()
            && self.packages.iter().eq(packages)
            && self.endpoint.as_deref() == Some(endpoint)
    }

    pub fn permits_plugin(&self) -> bool {
        !self.packages.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "Invalid variant provider opt-in `{0}`; expected NAMESPACE[=PACKAGE[,PACKAGE]...[@MODULE[:OBJECT]]]"
)]
pub struct VariantProviderOptInError(String);

impl FromStr for VariantProviderOptIn {
    type Err = VariantProviderOptInError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || VariantProviderOptInError(value.to_string());
        let (namespace, identity) = value
            .split_once('=')
            .map_or((value, None), |(namespace, identity)| {
                (namespace, Some(identity))
            });
        let namespace = VariantNamespace::try_from(namespace.to_string()).map_err(|_| invalid())?;
        let Some(identity) = identity else {
            return Ok(Self {
                namespace,
                packages: Vec::new(),
                endpoint: None,
            });
        };
        let (packages, endpoint) = identity
            .split_once('@')
            .map_or((identity, None), |(packages, endpoint)| {
                (packages, Some(endpoint))
            });
        let packages = packages
            .split(',')
            .map(|package| PackageName::from_str(package).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(first) = packages.first() else {
            return Err(invalid());
        };
        if !packages.iter().all_unique() {
            return Err(invalid());
        }
        let endpoint =
            endpoint.map_or_else(|| first.as_dist_info_name().into_owned(), str::to_string);
        if !valid_plugin_endpoint(&endpoint) {
            return Err(invalid());
        }
        Ok(Self {
            namespace,
            packages,
            endpoint: Some(endpoint),
        })
    }
}

/// Whether a plugin endpoint is a dotted Python module with an optional dotted object path.
pub fn valid_plugin_endpoint(endpoint: &str) -> bool {
    fn valid_path(path: &str) -> bool {
        path.split('.').all(|part| {
            let mut bytes = part.bytes();
            bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    }
    let (module, object) = endpoint
        .split_once(':')
        .map_or((endpoint, None), |(module, object)| (module, Some(object)));
    valid_path(module) && object.is_none_or(valid_path)
}

/// A supported feature and its compatible values, ordered from most to least preferred.
pub type SupportedVariantFeature = (VariantFeature, Vec<VariantValue>);

/// A feature returned by a PEP 850 provider plugin.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginVariantFeature {
    pub name: VariantFeature,
    pub values: Vec<VariantValue>,
    pub multi_value: bool,
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
    variants: BTreeMap<VariantLabel, VariantProperties>,
    // Keep `providers` last in both structs so compact MessagePack can omit it.
    // TODO(konsti): Remove this constraint once `providers` is required in all supported schemas.
    #[serde(skip_serializing_if = "Option::is_none")]
    providers: Option<BTreeMap<VariantNamespace, VariantProvider>>,
}

impl VariantsJsonContent {
    /// The label when this metadata describes exactly one wheel variant.
    pub fn wheel_label(&self) -> Option<&VariantLabel> {
        self.variants.keys().exactly_one().ok()
    }

    /// Return whether the metadata describes the given variant label.
    pub fn contains_variant(&self, label: &VariantLabel) -> bool {
        self.variants.contains_key(label)
    }

    /// Required providers and optional providers enabled by the user.
    pub fn enabled_providers(
        &self,
        policy: Option<&VariantPolicy>,
    ) -> impl Iterator<Item = (&VariantNamespace, &VariantProvider)> {
        self.providers
            .iter()
            .flatten()
            .filter(move |(namespace, provider)| {
                !provider.optional || policy.is_some_and(|policy| policy.enables(namespace))
            })
    }

    /// Validate the properties returned by a provider plugin.
    pub fn validate_plugin_features(
        &self,
        namespace: &VariantNamespace,
        features: &[PluginVariantFeature],
    ) -> Result<(), VariantMetadataError> {
        if !features.iter().map(|feature| &feature.name).all_unique() {
            return Err(VariantMetadataError::DuplicateFeatures(namespace.clone()));
        }
        for feature in features {
            validate_feature_values(namespace, &feature.name, &feature.values)?;
            if !feature.multi_value
                && self.variants.values().any(|properties| {
                    properties
                        .get(namespace)
                        .and_then(|features| features.get(&feature.name))
                        .is_some_and(|values| values.len() > 1)
                })
            {
                return Err(VariantMetadataError::SingleValueFeature(
                    namespace.clone(),
                    feature.name.clone(),
                ));
            }
        }
        Ok(())
    }

    /// Return the schema URL declared by the metadata.
    pub fn schema(&self) -> &str {
        &self.schema
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
        if metadata.variants.keys().eq(std::iter::once(label)) {
            Ok(metadata)
        } else {
            Err(VariantsJsonParseError::WheelLabels {
                expected: label.clone(),
                labels: metadata.variants.into_keys().collect(),
            })
        }
    }

    fn is_abi_dependency(&self, namespace: &VariantNamespace) -> bool {
        let Some(VariantProvider {
            kind: VariantProviderKind::Builtin { builtin },
            ..
        }) = self
            .providers
            .as_ref()
            .and_then(|providers| providers.get(namespace))
        else {
            return false;
        };
        *builtin == BuiltinProvider::AbiDependency
    }

    /// Collect the ABI requirements for a variant.
    ///
    /// Returns an error if the variant is missing or any of its ABI requirements is invalid.
    pub fn abi_dependencies(
        &self,
        label: &VariantLabel,
    ) -> Result<Vec<(PackageName, Ranges<Version>)>, AbiDependencyError> {
        let properties = self
            .variants
            .get(label)
            .ok_or_else(|| AbiDependencyError::MissingVariant(label.clone()))?;
        let mut dependencies = Vec::new();
        for (namespace, features) in properties {
            if self.is_abi_dependency(namespace) {
                for (feature, values) in features {
                    dependencies.push(abi_dependency(feature, values)?);
                }
            }
        }
        Ok(dependencies)
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
        let declared_properties = self.declared_properties(label)?;
        let mut properties = BTreeMap::new();
        for (namespace, features) in &declared_properties {
            let supported_features = supported.properties.get(namespace)?;
            let mut selected_features = BTreeMap::new();
            for (feature, values) in features {
                let (_, supported_values) = supported_features
                    .iter()
                    .find(|(supported_feature, _)| supported_feature == feature)?;
                let selected_values = values
                    .iter()
                    .filter(|value| supported_values.contains(value))
                    .cloned()
                    .collect::<Vec<_>>();
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
            declared_properties,
        })
    }

    /// Sort each feature's alternative values for comparison across metadata sources.
    fn declared_properties(&self, label: &VariantLabel) -> Option<VariantProperties> {
        let mut properties = self.variants.get(label)?.clone();
        for features in properties.values_mut() {
            for values in features.values_mut() {
                values.sort_unstable();
            }
        }
        Some(properties)
    }

    /// Check that the wheel declares the same properties used during resolution.
    pub fn matches_selected_variant(&self, selected: &SelectedVariant) -> bool {
        self.declared_properties(&selected.label).as_ref() == Some(&selected.declared_properties)
    }

    /// Recover the full selection from an installed wheel and its recorded target properties.
    pub(crate) fn restore_selected_variant(
        &self,
        recorded: RecordedVariant,
    ) -> Option<SelectedVariant> {
        let label = self.wheel_label()?;
        let properties = self.declared_properties(label)?;
        if label != &recorded.label || !properties.keys().eq(recorded.properties.keys()) {
            return None;
        }
        let valid = properties.iter().all(|(namespace, features)| {
            let selected_features = &recorded.properties[namespace];
            features.keys().eq(selected_features.keys())
                && features.iter().all(|(feature, values)| {
                    let selected_values = &selected_features[feature];
                    !selected_values.is_empty()
                        && selected_values.windows(2).all(|pair| pair[0] < pair[1])
                        && selected_values.iter().all(|value| values.contains(value))
                })
        });
        valid.then_some(SelectedVariant {
            label: recorded.label,
            properties: recorded.properties,
            declared_properties: properties,
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
                .enabled_providers(None)
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
        #[serde(deserialize_with = "deserialize_unique_map")]
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
        builtin: BuiltinProvider,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", rename_all = "snake_case")]
pub enum BuiltinProvider {
    AbiDependency,
    #[serde(untagged)]
    Unknown(String),
}

impl From<String> for BuiltinProvider {
    fn from(name: String) -> Self {
        match name.as_str() {
            "abi_dependency" => Self::AbiDependency,
            _ => Self::Unknown(name),
        }
    }
}

impl fmt::Display for BuiltinProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbiDependency => f.write_str("abi_dependency"),
            Self::Unknown(name) => f.write_str(name),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct VariantsJsonWire {
    #[serde(rename = "$schema")]
    schema: String,
    default_priorities: DefaultPriorities,
    variants: UniqueMap<
        VariantLabel,
        UniqueMap<VariantNamespace, UniqueMap<VariantFeature, Vec<VariantValue>>>,
    >,
    #[serde(default)]
    providers: Option<UniqueMap<VariantNamespace, VariantProvider>>,
}

/// A metadata map must not silently replace an earlier value for the same key.
#[derive(Deserialize)]
#[serde(
    transparent,
    bound(deserialize = "K: Ord + Deserialize<'de>, V: Deserialize<'de>")
)]
struct UniqueMap<K, V>(#[serde(deserialize_with = "deserialize_unique_map")] BTreeMap<K, V>);

fn deserialize_unique_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct UniqueMapVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for UniqueMapVisitor<K, V>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object with unique keys")
        }

        fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = access.next_entry()? {
                if values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate key in variant metadata",
                    ));
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(UniqueMapVisitor(PhantomData))
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
    #[error("Variant feature `{0} :: {1}` does not allow multiple values")]
    SingleValueFeature(VariantNamespace, VariantFeature),
    #[error("The null variant must not have properties")]
    NullProperties,
}

impl TryFrom<VariantsJsonWire> for VariantsJsonContent {
    type Error = VariantMetadataError;

    fn try_from(wire: VariantsJsonWire) -> Result<Self, Self::Error> {
        let mut metadata = Self {
            schema: wire.schema,
            default_priorities: wire.default_priorities,
            providers: wire.providers.map(|providers| providers.0),
            variants: wire
                .variants
                .0
                .into_iter()
                .map(|(label, namespaces)| {
                    let properties = namespaces
                        .0
                        .into_iter()
                        .map(|(namespace, features)| (namespace, features.0))
                        .collect();
                    (label, properties)
                })
                .collect(),
        };
        let namespaces = &metadata.default_priorities.namespace;
        let namespace_set: BTreeSet<_> = namespaces.iter().collect();
        if namespaces.is_empty() || namespace_set.len() != namespaces.len() {
            return Err(VariantMetadataError::NamespacePriorities);
        }

        // The provider checks below validate metadata only. Supported properties are collected
        // separately and passed to `score_variant` to determine compatibility.
        if metadata.providers.is_some() || metadata.schema == PEP_850_SCHEMA {
            let provider_namespaces = metadata
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
        if let Some(providers) = &metadata.providers {
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
                    // A static provider may declare that it supports no values for a feature.
                    if !values.is_empty() {
                        validate_feature_values(namespace, feature, values)?;
                    }
                }
            }
        }

        for (label, properties) in &mut metadata.variants {
            for (namespace, features) in properties.iter_mut() {
                if !features.is_empty() && !namespace_set.contains(namespace) {
                    return Err(VariantMetadataError::MissingNamespace(namespace.clone()));
                }
                for (feature, values) in features {
                    validate_feature_values(namespace, feature, values)?;
                    if label == &NULL_VARIANT_LABEL {
                        return Err(VariantMetadataError::NullProperties);
                    }
                }
            }
            properties.retain(|_, features| !features.is_empty());
        }

        // Parse leniently for forward compatibility: accept unknown schema versions when their
        // contents pass validation.
        if !matches!(metadata.schema.as_str(), PEP_825_SCHEMA | PEP_850_SCHEMA) {
            debug!("Unknown variant metadata schema `{}`", metadata.schema);
        }

        Ok(metadata)
    }
}

/// An error reading a variant's ABI dependency requirements.
#[derive(Debug, thiserror::Error)]
pub enum AbiDependencyError {
    #[error("Variant `{0}` is missing from the variant metadata")]
    MissingVariant(VariantLabel),
    #[error("Invalid ABI dependency feature name: {0}")]
    InvalidName(#[from] InvalidNameError),
    #[error(
        "Invalid ABI dependency feature name: `{feature}` must use the normalized \
         distribution name `{expected}`"
    )]
    NonNormalizedName {
        feature: VariantFeature,
        expected: String,
    },
    #[error("Unsupported ABI dependency value `{value}` for `{name}`: {source}")]
    InvalidVersion {
        name: PackageName,
        value: VariantValue,
        #[source]
        source: VersionParseError,
    },
    #[error(
        "Unsupported ABI dependency for `{name}`: \
         expected a canonical release prefix with one to three components: {value}"
    )]
    InvalidReleasePrefix {
        name: PackageName,
        value: VariantValue,
    },
}

/// ABI features name a distribution and list the release prefixes it can provide.
fn abi_dependency(
    feature: &VariantFeature,
    values: &[VariantValue],
) -> Result<(PackageName, Ranges<Version>), AbiDependencyError> {
    let name = PackageName::from_str(feature.as_str())?;
    let dist_info_name = name.as_dist_info_name();
    if dist_info_name != feature.as_str() {
        return Err(AbiDependencyError::NonNormalizedName {
            feature: feature.clone(),
            expected: dist_info_name.into_owned(),
        });
    }
    // The allowed version range is the union of the specified versions, as any matching
    // version is sufficient.
    let mut versions = Ranges::empty();
    for value in values {
        let version = Version::from_str(value.as_str()).map_err(|source| {
            AbiDependencyError::InvalidVersion {
                name: name.clone(),
                value: value.clone(),
                source,
            }
        })?;
        let release = version.release();
        if release.len() > 3 || version.only_release().to_string() != value.as_str() {
            return Err(AbiDependencyError::InvalidReleasePrefix {
                name,
                value: value.clone(),
            });
        }
        versions = versions.union(&VersionSpecifier::equals_star_version(version).into());
    }
    Ok((name, versions))
}

/// Validate that a feature declares at least one value and that its values are unique.
fn validate_feature_values(
    namespace: &VariantNamespace,
    feature: &VariantFeature,
    values: &[VariantValue],
) -> Result<(), VariantMetadataError> {
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
            values: values.to_vec(),
        });
    }
    Ok(())
}

/// The schema field alone, used to identify unknown schemas when parsing fails.
#[derive(Deserialize)]
struct VariantsJsonSchema {
    #[serde(rename = "$schema")]
    schema: String,
}
