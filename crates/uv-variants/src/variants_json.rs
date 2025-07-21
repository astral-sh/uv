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
pub struct Variant(FxHashMap<VariantNamespace, FxHashMap<VariantFeature, Vec<VariantValue>>>);

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
    type Target = FxHashMap<VariantNamespace, FxHashMap<VariantFeature, Vec<VariantValue>>>;

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
    /// Default provider priorities
    pub default_priorities: DefaultPriorities,
    /// Mapping of namespaces to provider information
    pub providers: FxHashMap<VariantNamespace, Provider>,
    /// Mapping of variant labels to properties
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
    pub feature: FxHashMap<VariantNamespace, Vec<VariantFeature>>,
    /// Default property priorities
    #[serde(default)]
    pub property: FxHashMap<VariantNamespace, FxHashMap<VariantFeature, Vec<VariantValue>>>,
}

/// A `namespace :: feature :: property` entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantPropertyType {
    pub namespace: VariantNamespace,
    pub feature: VariantFeature,
    pub value: VariantValue,
}

/// The stages at which a plugin is run.
///
/// Specifically captures whether it needs to be run at install time.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginUse {
    /// The plugin is never run, it is only static.
    None,
    /// The plugin is run at build time, the install time evaluation is static.
    Build,
    /// The plugin is run both at build time and at install time.
    #[default]
    All,
}

impl PluginUse {
    /// Whether to run this plugin on installation, `false` for plugins evaluated from
    /// default priorities.
    pub fn run_on_install(self) -> bool {
        match self {
            Self::All => true,
            Self::None | Self::Build => false,
        }
    }
}

/// Provider information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Provider {
    /// Object reference to plugin class
    pub plugin_api: Option<String>,
    /// Environment marker specifying when to enable the plugin
    #[serde(
        skip_serializing_if = "uv_pep508::marker::ser::is_empty",
        serialize_with = "uv_pep508::marker::ser::serialize",
        default
    )]
    pub enable_if: MarkerTree,
    /// Dependency specifiers for how to install the plugin
    pub requires: Option<Vec<Requirement<VerbatimParsedUrl>>>,
    /// Whether this plugin is run at install time.
    pub plugin_use: Option<PluginUse>,
}
