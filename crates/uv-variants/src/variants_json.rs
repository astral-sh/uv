use std::collections::BTreeMap;
use std::ops::Deref;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use uv_distribution_filename::VariantLabel;
use uv_pep508::{
    MarkerTree, MarkerVariantsEnvironment, Requirement, VariantFeature, VariantNamespace,
    VariantValue,
};
use uv_pypi_types::VerbatimParsedUrl;

/// Mapping of namespaces in a variant
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Variant(BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>);

impl MarkerVariantsEnvironment for Variant {
    fn contains_namespace(&self, namespace: &VariantNamespace) -> bool {
        self.0.contains_key(namespace)
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

/// Combined index metadata for wheel variants.
///
/// See <https://wheelnext.dev/variants.json>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantsJsonContent {
    /// Default provider priorities.
    pub default_priorities: DefaultPriorities,
    /// Mapping of namespaces to provider information.
    pub providers: FxHashMap<VariantNamespace, Provider>,
    /// The supported, ordered properties for `AoT` providers.
    pub static_properties: Option<Variant>,
    /// Mapping of variant labels to properties.
    pub variants: FxHashMap<VariantLabel, Variant>,
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
    /// Default feature priorities
    #[serde(default)]
    pub feature: BTreeMap<VariantNamespace, Vec<VariantFeature>>,
    /// Default property priorities
    #[serde(default)]
    pub property: BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>,
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
