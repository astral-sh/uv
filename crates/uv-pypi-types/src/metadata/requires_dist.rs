use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use uv_normalize::{ExtraName, PackageName};
use uv_pep508::Requirement;

use crate::metadata::pyproject_toml::PyProjectToml;
use crate::{LenientRequirement, MetadataError, VerbatimParsedUrl};

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
    #[serde(default)]
    pub dynamic: bool,
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
        let mut dynamic = false;
        for field in project.dynamic.unwrap_or_default() {
            match field.as_str() {
                "dependencies" => return Err(MetadataError::DynamicField("dependencies")),
                "optional-dependencies" => {
                    return Err(MetadataError::DynamicField("optional-dependencies"))
                }
                "version" => {
                    dynamic = true;
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
            dynamic,
        })
    }
}
