use pypi_types::VerbatimParsedUrl;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fmt, mem};
use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, TomlError, Value};

/// A `tools.toml` with an (optional) `[tools]` section.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolsToml {
    pub(crate) tools: Option<BTreeMap<String, Tool>>,

    /// The raw unserialized document.
    #[serde(skip)]
    pub(crate) raw: String,
}

impl ToolsToml {
    /// Parse a `ToolsToml` from a raw TOML string.
    pub(crate) fn from_string(raw: String) -> Result<Self, toml::de::Error> {
        let tools = toml::from_str(&raw)?;
        Ok(ToolsToml { raw, ..tools })
    }
}

// Ignore raw document in comparison.
impl PartialEq for ToolsToml {
    fn eq(&self, other: &Self) -> bool {
        self.tools.eq(&other.tools)
    }
}

impl Eq for ToolsToml {}

/// A `[[tools]]` entry.
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

/// Raw and mutable representation of a `tools.toml`.
///
/// This is useful for operations that require editing an existing `tools.toml` while
/// preserving comments and other structure.
pub struct ToolsTomlMut {
    doc: DocumentMut,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to parse `tools.toml`")]
    Parse(#[from] Box<TomlError>),
    #[error("Failed to serialize `tools.toml`")]
    Serialize(#[from] Box<toml::ser::Error>),
    #[error("`tools.toml` is malformed")]
    MalformedTools,
}

impl ToolsTomlMut {
    /// Initialize a `ToolsTomlMut` from a `ToolsToml`.
    pub fn from_toml(tools: &ToolsToml) -> Result<Self, Error> {
        Ok(Self {
            doc: tools.raw.parse().map_err(Box::new)?,
        })
    }

    /// Adds a tool to `tools`.
    pub fn add_tool(&mut self, name: &str, tool: &Tool) -> Result<(), Error> {
        // Get or create `tools`.
        let tools = self
            .doc
            .entry("tools")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedTools)?;

        add_tool(name, tool, tools)?;

        Ok(())
    }
}

/// Adds a tool to the given `tools` table.
pub(crate) fn add_tool(name: &str, tool: &Tool, tools: &mut Table) -> Result<(), Error> {
    // Serialize as an inline table.
    let mut doc = toml::to_string(tool)
        .map_err(Box::new)?
        .parse::<DocumentMut>()
        .unwrap();
    let table = mem::take(doc.as_table_mut()).into_inline_table();

    tools.insert(name, Item::Value(Value::InlineTable(table)));

    Ok(())
}

impl fmt::Display for ToolsTomlMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}
