use std::str::FromStr;

use indexmap::IndexMap;
use serde::de::IntoDeserializer;
use serde::Deserialize;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;

use crate::MetadataError;

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    pub project: Option<Project>,
    pub(super) tool: Option<Tool>,
}

impl PyProjectToml {
    pub fn from_toml(toml: &str) -> Result<Self, MetadataError> {
        let pyproject_toml: toml_edit::ImDocument<_> = toml_edit::ImDocument::from_str(toml)
            .map_err(MetadataError::InvalidPyprojectTomlSyntax)?;
        let pyproject_toml: Self = PyProjectToml::deserialize(pyproject_toml.into_deserializer())
            .map_err(MetadataError::InvalidPyprojectTomlSchema)?;
        Ok(pyproject_toml)
    }
}

/// PEP 621 project metadata.
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for dependency resolution.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Debug)]
#[serde(try_from = "PyprojectTomlWire")]
pub struct Project {
    /// The name of the project
    pub name: PackageName,
    /// The version of the project as supported by PEP 440
    pub version: Option<Version>,
    /// The Python version requirements of the project
    pub requires_python: Option<String>,
    /// Project dependencies
    pub dependencies: Option<Vec<String>>,
    /// Optional dependencies
    pub optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    pub dynamic: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyprojectTomlWire {
    name: Option<PackageName>,
    version: Option<Version>,
    requires_python: Option<String>,
    dependencies: Option<Vec<String>>,
    optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    dynamic: Option<Vec<String>>,
}

impl TryFrom<PyprojectTomlWire> for Project {
    type Error = MetadataError;

    fn try_from(wire: PyprojectTomlWire) -> Result<Self, Self::Error> {
        let name = wire.name.ok_or(MetadataError::MissingName)?;
        Ok(Project {
            name,
            version: wire.version,
            requires_python: wire.requires_python,
            dependencies: wire.dependencies,
            optional_dependencies: wire.optional_dependencies,
            dynamic: wire.dynamic,
        })
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(super) struct Tool {
    pub(super) poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::empty_structs_with_brackets)]
pub(super) struct ToolPoetry {}
