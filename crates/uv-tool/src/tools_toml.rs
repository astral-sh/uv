use std::path::Path;

use pypi_types::VerbatimParsedUrl;
use serde::{Deserialize, Serialize};

/// A `tool.toml` file with a `[tool]` entry.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolToml {
    pub(crate) tool: Tool,

    /// The raw unserialized document.
    #[serde(skip)]
    pub(crate) raw: String,
}

impl ToolToml {
    /// Parse a `ToolToml` from a raw TOML string.
    pub(crate) fn from_string(raw: String) -> Result<Self, toml::de::Error> {
        let tool = toml::from_str(&raw)?;
        Ok(ToolToml { raw, ..tool })
    }

    ///  Parse a `ToolToml` from the given path TOML string.
    pub(crate) fn from_path(path: &Path) -> Result<ToolToml, crate::Error> {
        match fs_err::read_to_string(path) {
            Ok(contents) => Ok(ToolToml::from_string(contents)
                .map_err(|err| crate::Error::TomlRead(path.to_owned(), Box::new(err)))?),
            Err(err) => Err(err.into()),
        }
    }
}

// Ignore raw document in comparison.
impl PartialEq for ToolToml {
    fn eq(&self, other: &Self) -> bool {
        self.tool.eq(&other.tool)
    }
}

impl Eq for ToolToml {}

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

impl From<Tool> for ToolToml {
    fn from(tool: Tool) -> Self {
        ToolToml {
            tool,
            raw: String::new(),
        }
    }
}
