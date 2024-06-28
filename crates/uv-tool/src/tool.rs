use std::path::PathBuf;

use pypi_types::VerbatimParsedUrl;
use serde::{Deserialize, Serialize};

/// A tool entry.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    // The requirements requested by the user during installation.
    requirements: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    /// The Python requested by the user during installation.
    python: Option<String>,
    // A mapping of entry point names to their metadata.
    entrypoints: Vec<ToolEntrypoint>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ToolEntrypoint {
    name: String,
    install_path: PathBuf,
}

impl Tool {
    /// Create a new `Tool`.
    pub fn new(
        requirements: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
        python: Option<String>,
        entrypoints: impl Iterator<Item = ToolEntrypoint>,
    ) -> Self {
        let mut entrypoints: Vec<_> = entrypoints.collect();
        entrypoints.sort();
        Self {
            requirements,
            python,
            entrypoints,
        }
    }
}

impl ToolEntrypoint {
    /// Create a new [`ToolEntrypoint`].
    pub fn new(name: String, install_path: PathBuf) -> Self {
        Self { name, install_path }
    }
}
