use std::path::PathBuf;

use serde::Deserialize;
use toml_edit::value;
use toml_edit::Table;
use toml_edit::Value;
use toml_edit::{Array, Item};

use pypi_types::{Requirement, VerbatimParsedUrl};
use uv_fs::PortablePath;
use uv_settings::ToolOptions;

/// A tool entry.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "ToolWire", into = "ToolWire")]
pub struct Tool {
    /// The requirements requested by the user during installation.
    requirements: Vec<Requirement>,
    /// The Python requested by the user during installation.
    python: Option<String>,
    /// A mapping of entry point names to their metadata.
    entrypoints: Vec<ToolEntrypoint>,
    /// The [`ToolOptions`] used to install this tool.
    options: ToolOptions,
}

#[derive(Clone, Debug, Deserialize)]
struct ToolWire {
    requirements: Vec<RequirementWire>,
    python: Option<String>,
    entrypoints: Vec<ToolEntrypoint>,
    #[serde(default)]
    options: ToolOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum RequirementWire {
    /// A [`Requirement`] following our uv-specific schema.
    Requirement(Requirement),
    /// A PEP 508-compatible requirement. We no longer write these, but there might be receipts out
    /// there that still use them.
    Deprecated(pep508_rs::Requirement<VerbatimParsedUrl>),
}

impl From<Tool> for ToolWire {
    fn from(tool: Tool) -> Self {
        Self {
            requirements: tool
                .requirements
                .into_iter()
                .map(RequirementWire::Requirement)
                .collect(),
            python: tool.python,
            entrypoints: tool.entrypoints,
            options: tool.options,
        }
    }
}

impl TryFrom<ToolWire> for Tool {
    type Error = serde::de::value::Error;

    fn try_from(tool: ToolWire) -> Result<Self, Self::Error> {
        Ok(Self {
            requirements: tool
                .requirements
                .into_iter()
                .map(|req| match req {
                    RequirementWire::Requirement(requirements) => requirements,
                    RequirementWire::Deprecated(requirement) => Requirement::from(requirement),
                })
                .collect(),
            python: tool.python,
            entrypoints: tool.entrypoints,
            options: tool.options,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolEntrypoint {
    pub name: String,
    pub install_path: PathBuf,
}

/// Format an array so that each element is on its own line and has a trailing comma.
///
/// Example:
///
/// ```toml
/// requirements = [
///     "foo",
///     "bar",
/// ]
/// ```
fn each_element_on_its_line_array(elements: impl Iterator<Item = impl Into<Value>>) -> Array {
    let mut array = elements
        .map(Into::into)
        .map(|mut value| {
            // Each dependency is on its own line and indented.
            value.decor_mut().set_prefix("\n    ");
            value
        })
        .collect::<Array>();
    // With a trailing comma, inserting another entry doesn't change the preceding line,
    // reducing the diff noise.
    array.set_trailing_comma(true);
    // The line break between the last element's comma and the closing square bracket.
    array.set_trailing("\n");
    array
}

impl Tool {
    /// Create a new `Tool`.
    pub fn new(
        requirements: Vec<Requirement>,
        python: Option<String>,
        entrypoints: impl Iterator<Item = ToolEntrypoint>,
        options: ToolOptions,
    ) -> Self {
        let mut entrypoints: Vec<_> = entrypoints.collect();
        entrypoints.sort();
        Self {
            requirements,
            python,
            entrypoints,
            options,
        }
    }

    /// Create a new [`Tool`] with the given [`ToolOptions`].
    #[must_use]
    pub fn with_options(self, options: ToolOptions) -> Self {
        Self { options, ..self }
    }

    /// Returns the TOML table for this tool.
    pub(crate) fn to_toml(&self) -> Result<Table, toml_edit::ser::Error> {
        let mut table = Table::new();

        table.insert("requirements", {
            let requirements = self
                .requirements
                .iter()
                .map(|requirement| {
                    serde::Serialize::serialize(
                        &requirement,
                        toml_edit::ser::ValueSerializer::new(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;

            let requirements = match requirements.as_slice() {
                [] => Array::new(),
                [requirement] => Array::from_iter([requirement]),
                requirements => each_element_on_its_line_array(requirements.iter()),
            };
            value(requirements)
        });

        if let Some(ref python) = self.python {
            table.insert("python", value(python));
        }

        table.insert("entrypoints", {
            let entrypoints = each_element_on_its_line_array(
                self.entrypoints
                    .iter()
                    .map(ToolEntrypoint::to_toml)
                    .map(Table::into_inline_table),
            );
            value(entrypoints)
        });

        if self.options != ToolOptions::default() {
            let serialized =
                serde::Serialize::serialize(&self.options, toml_edit::ser::ValueSerializer::new())?;
            let Value::InlineTable(serialized) = serialized else {
                return Err(toml_edit::ser::Error::Custom(
                    "Expected an inline table".to_string(),
                ));
            };
            table.insert("options", Item::Table(serialized.into_table()));
        }

        Ok(table)
    }

    pub fn entrypoints(&self) -> &[ToolEntrypoint] {
        &self.entrypoints
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    pub fn python(&self) -> &Option<String> {
        &self.python
    }

    pub fn options(&self) -> &ToolOptions {
        &self.options
    }
}

impl ToolEntrypoint {
    /// Create a new [`ToolEntrypoint`].
    pub fn new(name: String, install_path: PathBuf) -> Self {
        Self { name, install_path }
    }

    /// Returns the TOML table for this entrypoint.
    pub(crate) fn to_toml(&self) -> Table {
        let mut table = Table::new();
        table.insert("name", value(&self.name));
        table.insert(
            "install-path",
            // Use cross-platform slashes so the toml string type does not change
            value(PortablePath::from(&self.install_path).to_string()),
        );
        table
    }
}
