use pypi_types::VerbatimParsedUrl;
use serde::{Deserialize, Serialize};

/// A tool entry.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    requirements: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    python: Option<String>,
}

impl Tool {
    /// Create a new `Tool`.
    pub fn new(
        requirements: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
        python: Option<String>,
    ) -> Self {
        Self {
            requirements,
            python,
        }
    }
}
