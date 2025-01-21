use std::str::FromStr;

use indexmap::IndexMap;
use itertools::Itertools;
use serde::de::IntoDeserializer;
use serde::Deserialize;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;

use crate::{LenientRequirement, LenientVersionSpecifiers, MetadataError, ResolutionMetadata};

/// Extract the metadata from a `pyproject.toml` file, as specified in PEP 621.
///
/// If we're coming from a source distribution, we may already know the version (unlike for a source
/// tree), so we can tolerate dynamic versions.
pub(super) fn parse_pyproject_toml(
    contents: &str,
    sdist_version: Option<&Version>,
) -> Result<ResolutionMetadata, MetadataError> {
    let pyproject_toml = PyProjectToml::from_toml(contents)?;

    let project = pyproject_toml
        .project
        .ok_or(MetadataError::FieldNotFound("project"))?;

    // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml` file.
    let mut dynamic = false;
    for field in project.dynamic.unwrap_or_default() {
        match field.as_str() {
            "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
            "optional-dependencies" => {
                return Err(MetadataError::DynamicField("optional-dependencies"))
            }
            "requires-python" => return Err(MetadataError::DynamicField("requires-python")),
            // When building from a source distribution, the version is known from the filename and
            // fixed by it, so we can pretend it's static.
            "version" => {
                if sdist_version.is_none() {
                    return Err(MetadataError::DynamicField("version"));
                }
                dynamic = true;
            }
            _ => (),
        }
    }

    // If dependencies are declared with Poetry, and `project.dependencies` is omitted, treat
    // the dependencies as dynamic. The inclusion of a `project` table without defining
    // `project.dependencies` is almost certainly an error.
    if project.dependencies.is_none() && pyproject_toml.tool.and_then(|tool| tool.poetry).is_some()
    {
        return Err(MetadataError::PoetrySyntax);
    }

    let name = project.name;
    let version = project
        .version
        // When building from a source distribution, the version is known from the filename and
        // fixed by it, so we can pretend it's static.
        .or_else(|| sdist_version.cloned())
        .ok_or(MetadataError::FieldNotFound("version"))?;

    // Parse the Python version requirements.
    let requires_python = project
        .requires_python
        .map(|requires_python| {
            LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
        })
        .transpose()?;

    // Extract the requirements.
    let mut requires_dist = project
        .dependencies
        .unwrap_or_default()
        .into_iter()
        .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
        .map_ok(Requirement::from)
        .collect::<Result<Vec<_>, _>>()?;

    // Extract the optional dependencies.
    let mut provides_extras: Vec<ExtraName> = Vec::new();
    for (extra, requirements) in project.optional_dependencies.unwrap_or_default() {
        requires_dist.extend(
            requirements
                .into_iter()
                .map(|requires_dist| LenientRequirement::from_str(&requires_dist))
                .map_ok(Requirement::from)
                .map_ok(|requirement| requirement.with_extra_marker(&extra))
                .collect::<Result<Vec<_>, _>>()?,
        );
        provides_extras.push(extra);
    }

    Ok(ResolutionMetadata {
        name,
        version,
        requires_dist,
        requires_python,
        provides_extras,
        dynamic,
    })
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(super) struct PyProjectToml {
    pub(super) project: Option<Project>,
    pub(super) tool: Option<Tool>,
}

impl PyProjectToml {
    pub(super) fn from_toml(toml: &str) -> Result<Self, MetadataError> {
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
pub(super) struct Project {
    /// The name of the project
    pub(super) name: PackageName,
    /// The version of the project as supported by PEP 440
    pub(super) version: Option<Version>,
    /// The Python version requirements of the project
    pub(super) requires_python: Option<String>,
    /// Project dependencies
    pub(super) dependencies: Option<Vec<String>>,
    /// Optional dependencies
    pub(super) optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    pub(super) dynamic: Option<Vec<String>>,
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

#[cfg(test)]
mod tests {
    use crate::metadata::pyproject_toml::parse_pyproject_toml;
    use crate::MetadataError;
    use std::str::FromStr;
    use uv_normalize::PackageName;
    use uv_pep440::Version;

    #[test]
    fn test_parse_pyproject_toml() {
        let s = r#"
        [project]
        name = "asdf"
    "#;
        let meta = parse_pyproject_toml(s, None);
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("version"))));

        let s = r#"
        [project]
        name = "asdf"
        dynamic = ["version"]
    "#;
        let meta = parse_pyproject_toml(s, None);
        assert!(matches!(meta, Err(MetadataError::DynamicField("version"))));

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
    "#;
        let meta = parse_pyproject_toml(s, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert!(meta.requires_python.is_none());
        assert!(meta.requires_dist.is_empty());
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
    "#;
        let meta = parse_pyproject_toml(s, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert!(meta.requires_dist.is_empty());
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]
    "#;
        let meta = parse_pyproject_toml(s, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
        assert!(meta.provides_extras.is_empty());

        let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]

        [project.optional-dependencies]
        dotenv = ["bar"]
    "#;
        let meta = parse_pyproject_toml(s, None).unwrap();
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
        assert_eq!(
            meta.requires_dist,
            vec![
                "foo".parse().unwrap(),
                "bar; extra == \"dotenv\"".parse().unwrap()
            ]
        );
        assert_eq!(meta.provides_extras, vec!["dotenv".parse().unwrap()]);
    }
}
