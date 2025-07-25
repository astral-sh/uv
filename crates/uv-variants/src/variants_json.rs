use std::ops::Deref;

use indoc::formatdoc;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use uv_pep508::{MarkerVariantsEnvironment, Requirement};
use uv_pypi_types::VerbatimParsedUrl;

// TODO(konsti): Validate the string contents
pub type VariantNamespace = String;
pub type VariantFeature = String;
pub type VariantProperty = String;

/// Mapping of namespaces in a variant
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Variant(FxHashMap<String, FxHashMap<String, Vec<String>>>);

impl MarkerVariantsEnvironment for Variant {
    fn contains_namespace(&self, namespace: &str) -> bool {
        self.0.contains_key(namespace)
    }

    fn contains_feature(&self, namespace: &str, feature: &str) -> bool {
        let Some(features) = self.0.get(namespace) else {
            return false;
        };

        let Some(properties) = features.get(feature) else {
            return false;
        };

        !properties.is_empty()
    }

    fn contains_property(&self, namespace: &str, feature: &str, property: &str) -> bool {
        let Some(features) = self.0.get(namespace) else {
            return false;
        };

        let Some(properties) = features.get(feature) else {
            return false;
        };

        properties.iter().any(|p| p == property)
    }
}

impl Deref for Variant {
    type Target = FxHashMap<String, FxHashMap<String, Vec<String>>>;

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
    pub providers: FxHashMap<String, Provider>,
    /// Mapping of variant labels to properties
    pub variants: FxHashMap<String, Variant>,
}

/// Default provider priorities
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DefaultPriorities {
    /// Default namespace priorities
    pub namespace: Vec<String>,
    /// Default feature priorities
    #[serde(default)]
    pub feature: FxHashMap<VariantNamespace, Vec<VariantFeature>>,
    /// Default property priorities
    #[serde(default)]
    pub property: FxHashMap<VariantNamespace, FxHashMap<VariantFeature, Vec<VariantProperty>>>,
}

/// A `namespace :: feature :: property` entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantPropertyType {
    pub namespace: String,
    pub feature: String,
    pub value: String,
}

/// Provider information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Provider {
    /// Object reference to plugin class
    pub plugin_api: Option<String>,
    /// Environment marker specifying when to enable the plugin
    // TODO(konsti): Why does this break caching
    /*#[serde(
        skip_serializing_if = "uv_pep508::marker::ser::is_empty",
        serialize_with = "uv_pep508::marker::ser::serialize",
        default
    )]
    pub enable_if: MarkerTree,*/
    /// Dependency specifiers for how to install the plugin
    pub requires: Vec<Requirement<VerbatimParsedUrl>>,
}

impl Provider {
    pub fn import(&self, name: &str) -> String {
        let import = if let Some(plugin_api) = &self.plugin_api {
            if let Some((path, object)) = plugin_api.split_once(':') {
                format!("from {path} import {object} as backend")
            } else {
                format!("import {plugin_api} as backend")
            }
        } else {
            // TODO(konsti): Normalize the name to a valid python identifier
            format!("import {name} as backend")
        };

        formatdoc! {r#"
            import sys

            if sys.path[0] == "":
                sys.path.pop(0)

            {import}

            if callable(backend):
                backend = backend()
        "#}
    }
}
