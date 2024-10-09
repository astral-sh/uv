use crate::{
    LenientRequirement, LenientVersionSpecifiers, MetadataError, ResolutionMetadata,
    VerbatimParsedUrl,
};
use indexmap::IndexMap;
use itertools::Itertools;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::Requirement;

/// Extract the metadata from a `pyproject.toml` file, as specified in PEP 621.
pub(crate) fn parse_pyproject_toml(contents: &str) -> Result<ResolutionMetadata, MetadataError> {
    let pyproject_toml = PyProjectToml::from_toml(contents)?;

    let project = pyproject_toml
        .project
        .ok_or(MetadataError::FieldNotFound("project"))?;

    // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml` file.
    let dynamic = project.dynamic.unwrap_or_default();
    for field in dynamic {
        match field.as_str() {
            "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
            "optional-dependencies" => {
                return Err(MetadataError::DynamicField("optional-dependencies"))
            }
            "requires-python" => return Err(MetadataError::DynamicField("requires-python")),
            "version" => return Err(MetadataError::DynamicField("version")),
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
    })
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<Project>,
    tool: Option<Tool>,
}

impl PyProjectToml {
    pub(crate) fn from_toml(toml: &str) -> Result<Self, MetadataError> {
        let pyproject_toml: toml_edit::ImDocument<_> = toml_edit::ImDocument::from_str(toml)
            .map_err(MetadataError::InvalidPyprojectTomlSyntax)?;
        let pyproject_toml: Self = PyProjectToml::deserialize(pyproject_toml.into_deserializer())
            .map_err(|err| {
            // TODO(konsti): A typed error would be nicer, this can break on toml upgrades.
            if err.message().contains("missing field `name`") {
                MetadataError::InvalidPyprojectTomlMissingName(err)
            } else {
                MetadataError::InvalidPyprojectTomlSchema(err)
            }
        })?;
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
#[serde(rename_all = "kebab-case")]
struct Project {
    /// The name of the project
    name: PackageName,
    /// The version of the project as supported by PEP 440
    version: Option<Version>,
    /// The Python version requirements of the project
    requires_python: Option<String>,
    /// Project dependencies
    dependencies: Option<Vec<String>>,
    /// Optional dependencies
    optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    dynamic: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Tool {
    poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::empty_structs_with_brackets)]
struct ToolPoetry {}

/// Python Package Metadata 2.3 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
///
/// This is a subset of [`ResolutionMetadata`]; specifically, it omits the `version` and `requires-python`
/// fields, which aren't necessary when extracting the requirements of a package without installing
/// the package itself.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub provides_extras: Vec<ExtraName>,
}

impl RequiresDist {
    /// Extract the [`RequiresDist`] from a `pyproject.toml` file, as specified in PEP 621.
    pub fn parse_pyproject_toml(contents: &str) -> Result<Self, MetadataError> {
        let pyproject_toml = PyProjectToml::from_toml(contents)?;

        let project = pyproject_toml
            .project
            .ok_or(MetadataError::FieldNotFound("project"))?;

        // If any of the fields we need were declared as dynamic, we can't use the `pyproject.toml`
        // file.
        let dynamic = project.dynamic.unwrap_or_default();
        for field in dynamic {
            match field.as_str() {
                "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
                "optional-dependencies" => {
                    return Err(MetadataError::DynamicField("optional-dependencies"))
                }
                _ => (),
            }
        }

        // If dependencies are declared with Poetry, and `project.dependencies` is omitted, treat
        // the dependencies as dynamic. The inclusion of a `project` table without defining
        // `project.dependencies` is almost certainly an error.
        if project.dependencies.is_none()
            && pyproject_toml.tool.and_then(|tool| tool.poetry).is_some()
        {
            return Err(MetadataError::PoetrySyntax);
        }

        let name = project.name;

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

        Ok(Self {
            name,
            requires_dist,
            provides_extras,
        })
    }
}

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
        let meta = parse_pyproject_toml(s);
        assert!(matches!(meta, Err(MetadataError::FieldNotFound("version"))));

        let s = r#"
            [project]
            name = "asdf"
            dynamic = ["version"]
        "#;
        let meta = parse_pyproject_toml(s);
        assert!(matches!(meta, Err(MetadataError::DynamicField("version"))));

        let s = r#"
            [project]
            name = "asdf"
            version = "1.0"
        "#;
        let meta = parse_pyproject_toml(s).unwrap();
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
        let meta = parse_pyproject_toml(s).unwrap();
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
        let meta = parse_pyproject_toml(s).unwrap();
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
        let meta = parse_pyproject_toml(s).unwrap();
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
