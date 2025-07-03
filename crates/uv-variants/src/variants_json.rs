use std::collections::HashMap;

use serde::Deserialize;

use uv_pep508::{MarkerTree, Requirement};

/// Mapping of namespaces in a variant
pub type Variant = HashMap<String, VariantNamespace>;

/// Mapping of features to their possible values in a namespace
pub type VariantNamespace = HashMap<String, Vec<String>>;

// TODO(konsti): Validate the string contents
pub type Namespace = String;
pub type Feature = String;
pub type Property = String;

/// Combined index metadata for wheel variants.
///
/// See <https://wheelnext.dev/variants.json>
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantsJsonContent {
    /// Default provider priorities
    pub default_priorities: DefaultPriorities,
    /// Mapping of namespaces to provider information
    pub providers: HashMap<String, Provider>,
    /// Mapping of variant labels to properties
    pub variants: HashMap<String, Variant>,
}

/// Default provider priorities
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DefaultPriorities {
    /// Default namespace priorities
    pub namespace: Vec<String>,
    /// Default feature priorities
    #[serde(default)]
    pub feature: HashMap<Namespace, Vec<Feature>>,
    /// Default property priorities
    #[serde(default)]
    pub property: HashMap<Namespace, HashMap<Feature, Vec<Property>>>,
}

/// Provider information
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Provider {
    /// Object reference to plugin class
    pub plugin_api: Option<String>,
    /// Environment marker specifying when to enable the plugin
    pub enable_if: Option<MarkerTree>,
    /// Dependency specifiers for how to install the plugin
    pub requires: Vec<Requirement>,
}
