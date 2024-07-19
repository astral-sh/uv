use std::path::PathBuf;

use path_slash::PathBufExt;
use pypi_types::VerbatimParsedUrl;
use serde::Deserialize;
use toml_edit::value;
use toml_edit::Array;
use toml_edit::Table;
use toml_edit::Value;

/// A tool entry.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    /// The requirements requested by the user during installation.
    requirements: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    /// The Python requested by the user during installation.
    python: Option<String>,
    /// A mapping of entry point names to their metadata.
    entrypoints: Vec<ToolEntrypoint>,
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

    /// Returns the TOML table for this tool.
    pub(crate) fn to_toml(&self) -> Table {
        let mut table = Table::new();

        table.insert("requirements", {
            let requirements = match self.requirements.as_slice() {
                [] => Array::new(),
                [requirement] => Array::from_iter([Value::from(requirement.to_string())]),
                requirements => each_element_on_its_line_array(
                    requirements
                        .iter()
                        .map(|requirement| Value::from(requirement.to_string())),
                ),
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
                    .map(toml_edit::Table::into_inline_table),
            );
            value(entrypoints)
        });

        table
    }

    pub fn entrypoints(&self) -> &[ToolEntrypoint] {
        &self.entrypoints
    }

    pub fn requirements(&self) -> &[pep508_rs::Requirement<VerbatimParsedUrl>] {
        &self.requirements
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
            value(self.install_path.to_slash_lossy().to_string()),
        );
        table
    }
}
