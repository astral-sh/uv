use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use serde::Deserialize;
use toml_edit::{Array, Item, Table, Value, value};

use uv_distribution_types::Requirement;
use uv_fs::{PortablePath, Simplified};
use uv_pypi_types::VerbatimParsedUrl;
use uv_python::PythonRequest;
use uv_settings::ToolOptions;

/// A tool entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "ToolWire", into = "ToolWire")]
pub struct Tool {
    /// The requirements requested by the user during installation.
    requirements: Vec<Requirement>,
    /// The constraints requested by the user during installation.
    constraints: Vec<Requirement>,
    /// The overrides requested by the user during installation.
    overrides: Vec<Requirement>,
    /// The build constraints requested by the user during installation.
    build_constraints: Vec<Requirement>,
    /// The Python requested by the user during installation.
    python: Option<PythonRequest>,
    /// A mapping of entry point names to their metadata.
    entrypoints: Vec<ToolEntrypoint>,
    /// The [`ToolOptions`] used to install this tool.
    options: ToolOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ToolWire {
    #[serde(default)]
    requirements: Vec<RequirementWire>,
    #[serde(default)]
    constraints: Vec<Requirement>,
    #[serde(default)]
    overrides: Vec<Requirement>,
    #[serde(default)]
    build_constraint_dependencies: Vec<Requirement>,
    python: Option<PythonRequest>,
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
    Deprecated(uv_pep508::Requirement<VerbatimParsedUrl>),
}

impl From<Tool> for ToolWire {
    fn from(tool: Tool) -> Self {
        Self {
            requirements: tool
                .requirements
                .into_iter()
                .map(RequirementWire::Requirement)
                .collect(),
            constraints: tool.constraints,
            overrides: tool.overrides,
            build_constraint_dependencies: tool.build_constraints,
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
            constraints: tool.constraints,
            overrides: tool.overrides,
            build_constraints: tool.build_constraint_dependencies,
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
    pub from: Option<String>,
}

impl Display for ToolEntrypoint {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        #[cfg(windows)]
        {
            write!(
                f,
                "{} ({})",
                self.name,
                self.install_path
                    .simplified_display()
                    .to_string()
                    .replace('/', "\\")
            )
        }
        #[cfg(unix)]
        {
            write!(
                f,
                "{} ({})",
                self.name,
                self.install_path.simplified_display()
            )
        }
    }
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
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
        build_constraints: Vec<Requirement>,
        python: Option<PythonRequest>,
        entrypoints: impl IntoIterator<Item = ToolEntrypoint>,
        options: ToolOptions,
    ) -> Self {
        let mut entrypoints: Vec<_> = entrypoints.into_iter().collect();
        entrypoints.sort();
        Self {
            requirements,
            constraints,
            overrides,
            build_constraints,
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

        if !self.requirements.is_empty() {
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
        }

        if !self.constraints.is_empty() {
            table.insert("constraints", {
                let constraints = self
                    .constraints
                    .iter()
                    .map(|constraint| {
                        serde::Serialize::serialize(
                            &constraint,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let constraints = match constraints.as_slice() {
                    [] => Array::new(),
                    [constraint] => Array::from_iter([constraint]),
                    constraints => each_element_on_its_line_array(constraints.iter()),
                };
                value(constraints)
            });
        }

        if !self.overrides.is_empty() {
            table.insert("overrides", {
                let overrides = self
                    .overrides
                    .iter()
                    .map(|r#override| {
                        serde::Serialize::serialize(
                            &r#override,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let overrides = match overrides.as_slice() {
                    [] => Array::new(),
                    [r#override] => Array::from_iter([r#override]),
                    overrides => each_element_on_its_line_array(overrides.iter()),
                };
                value(overrides)
            });
        }

        if !self.build_constraints.is_empty() {
            table.insert("build-constraint-dependencies", {
                let build_constraints = self
                    .build_constraints
                    .iter()
                    .map(|r#build_constraint| {
                        serde::Serialize::serialize(
                            &r#build_constraint,
                            toml_edit::ser::ValueSerializer::new(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let build_constraints = match build_constraints.as_slice() {
                    [] => Array::new(),
                    [r#build_constraint] => Array::from_iter([r#build_constraint]),
                    build_constraints => each_element_on_its_line_array(build_constraints.iter()),
                };
                value(build_constraints)
            });
        }

        if let Some(ref python) = self.python {
            table.insert(
                "python",
                value(serde::Serialize::serialize(
                    &python,
                    toml_edit::ser::ValueSerializer::new(),
                )?),
            );
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

    pub fn constraints(&self) -> &[Requirement] {
        &self.constraints
    }

    pub fn overrides(&self) -> &[Requirement] {
        &self.overrides
    }

    pub fn build_constraints(&self) -> &[Requirement] {
        &self.build_constraints
    }

    pub fn python(&self) -> &Option<PythonRequest> {
        &self.python
    }

    pub fn options(&self) -> &ToolOptions {
        &self.options
    }
}

impl ToolEntrypoint {
    /// Create a new [`ToolEntrypoint`].
    pub fn new(name: &str, install_path: PathBuf, from: String) -> Self {
        let name = name
            .trim_end_matches(std::env::consts::EXE_SUFFIX)
            .to_string();
        Self {
            name,
            install_path,
            from: Some(from),
        }
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
        if let Some(from) = &self.from {
            table.insert("from", value(from));
        }
        table
    }
}
