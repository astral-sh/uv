use std::fmt::{Debug, Display};
use std::str::FromStr;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::IntoDeserializer;
use tracing::instrument;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};

use crate::{LenientVersionSpecifiers, MetadataError};

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    pub project: Option<Project>,
    pub tool: Option<Tool>,
}

impl PyProjectToml {
    #[instrument(name = "toml::from_str uv pypi types", skip_all, fields(source = % _source))]
    pub fn from_toml(toml: &str, _source: impl Display) -> Result<Self, MetadataError> {
        let pyproject_toml = toml_edit::Document::from_str(toml)
            .map_err(MetadataError::InvalidPyprojectTomlSyntax)?;
        let pyproject_toml = Self::deserialize(pyproject_toml.into_deserializer())
            .map_err(MetadataError::InvalidPyprojectTomlSchema)?;
        Ok(pyproject_toml)
    }

    /// Extract static `requires-python` metadata from a PEP 621 project.
    ///
    /// Unlike [`crate::ResolutionMetadata::parse_pyproject_toml`], this does not require static
    /// project version or dependency metadata.
    pub fn requires_python(&self) -> Result<Option<VersionSpecifiers>, MetadataError> {
        let project = self
            .project
            .as_ref()
            .ok_or(MetadataError::FieldNotFound("project"))?;

        if project
            .dynamic
            .as_ref()
            .is_some_and(|dynamic| dynamic.iter().any(|field| field == "requires-python"))
        {
            return Err(MetadataError::DynamicField("requires-python"));
        }

        Ok(project
            .requires_python
            .as_ref()
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?)
    }
}

/// PEP 621 project metadata.
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for dependency resolution.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Debug, Clone)]
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

#[derive(Deserialize)]
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
        Ok(Self {
            name,
            version: wire.version,
            requires_python: wire.requires_python,
            dependencies: wire.dependencies,
            optional_dependencies: wire.optional_dependencies,
            dynamic: wire.dynamic,
        })
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    pub poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ToolPoetry {
    pub name: Option<PackageName>,
}

#[cfg(test)]
mod tests {
    use super::PyProjectToml;
    use crate::MetadataError;

    #[test]
    fn requires_python_allows_unrelated_dynamic_metadata() {
        let pyproject_toml = PyProjectToml::from_toml(
            r#"
            [project]
            name = "example"
            requires-python = ">=3.11,<3.13"
            dynamic = ["version", "dependencies"]
            "#,
            "pyproject.toml",
        )
        .unwrap();

        assert_eq!(
            pyproject_toml.requires_python().unwrap(),
            Some(">=3.11,<3.13".parse().unwrap())
        );
    }

    #[test]
    fn requires_python_rejects_dynamic_field() {
        let pyproject_toml = PyProjectToml::from_toml(
            r#"
            [project]
            name = "example"
            dynamic = ["requires-python"]
            "#,
            "pyproject.toml",
        )
        .unwrap();

        assert!(matches!(
            pyproject_toml.requires_python(),
            Err(MetadataError::DynamicField("requires-python"))
        ));
    }
}
