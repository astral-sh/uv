use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionPolicy {
    /// Resolve only for a single, exact Python version (e.g. 3.12.3)
    Only(String),
    /// Resolve for a PEP 440-compatible version spec (e.g. ">=3.12,<3.13")
    Range(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")] 
pub struct PythonResolutionConfig {
    pub resolution: Option<ResolutionPolicy>,
}
