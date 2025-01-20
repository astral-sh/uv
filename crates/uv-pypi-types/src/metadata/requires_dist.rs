use std::collections::VecDeque;
use std::str::FromStr;

use itertools::Itertools;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use uv_normalize::{ExtraName, PackageName};
use uv_pep508::{MarkerTree, Requirement, VersionOrUrl};

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

/// Like [`RequiresDist`], but with any recursive (or self-referential) dependencies resolved.
///
/// For example, given:
/// ```toml
/// [project]
/// name = "example"
/// version = "0.1.0"
/// requires-python = ">=3.13.0"
/// dependencies = []
///
/// [project.optional-dependencies]
/// all = [
///     "example[async]",
/// ]
/// async = [
///     "fastapi",
/// ]
/// ```
///
/// A build backend could return:
/// ```txt
/// Metadata-Version: 2.2
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: example[async]; extra == "all"
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == "async"
/// ```
///
/// Or:
/// ```txt
/// Metadata-Version: 2.4
/// Name: example
/// Version: 0.1.0
/// Requires-Python: >=3.13.0
/// Provides-Extra: all
/// Requires-Dist: fastapi; extra == 'all'
/// Provides-Extra: async
/// Requires-Dist: fastapi; extra == 'async'
/// ```
///
/// The [`FlatRequiresDist`] struct is used to flatten out the recursive dependencies, i.e., convert
/// from the former to the latter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRequiresDist {
    pub name: PackageName,
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
    pub provides_extras: Vec<ExtraName>,
}

impl From<RequiresDist> for FlatRequiresDist {
    fn from(value: RequiresDist) -> Self {
        // If there are no self-references, we can return early.
        if value.requires_dist.iter().all(|req| req.name != value.name) {
            return Self {
                name: value.name,
                requires_dist: value.requires_dist,
                provides_extras: value.provides_extras,
            };
        }

        // Transitively process all extras that are recursively included.
        let mut requires_dist = value.requires_dist.clone();
        let mut seen = FxHashSet::<(ExtraName, MarkerTree)>::default();
        let mut queue: VecDeque<_> = value
            .requires_dist
            .iter()
            .filter(|req| req.name == value.name)
            .flat_map(|req| req.extras.iter().cloned().map(|extra| (extra, req.marker)))
            .collect();
        while let Some((extra, marker)) = queue.pop_front() {
            if !seen.insert((extra.clone(), marker)) {
                continue;
            }

            // Find the requirements for the extra.
            for requirement in &value.requires_dist {
                if requirement.marker.top_level_extra_name().as_ref() == Some(&extra) {
                    let requirement = {
                        let mut marker = marker;
                        marker.and(requirement.marker);
                        Requirement {
                            name: requirement.name.clone(),
                            extras: requirement.extras.clone(),
                            version_or_url: requirement.version_or_url.clone(),
                            marker: marker.simplify_extras(std::slice::from_ref(&extra)),
                            origin: requirement.origin.clone(),
                        }
                    };
                    if requirement.name == value.name {
                        // Add each transitively included extra.
                        queue.extend(
                            requirement
                                .extras
                                .iter()
                                .cloned()
                                .map(|extra| (extra, requirement.marker)),
                        );
                    } else {
                        // Add the requirements for that extra.
                        requires_dist.push(requirement);
                    }
                }
            }
        }

        // Drop all the self-requirements now that we flattened them out, but retain any that have
        // a version specifier.
        //
        // For example, if a project depends on itself, as in:
        // ```toml
        // [project]
        // name = "project"
        // version = "0.1.0"
        // requires-python = ">=3.12"
        // dependencies = ["typing-extensions", "project==0.1.0"]
        // ```
        //
        // We should retain `project==0.1.0`.
        requires_dist.retain(|req| {
            if req.name == value.name {
                if req
                    .version_or_url
                    .as_ref()
                    .is_none_or(|version_or_url| match version_or_url {
                        VersionOrUrl::Url(_) => false,
                        VersionOrUrl::VersionSpecifier(specifiers) => specifiers.is_empty(),
                    })
                {
                    return false;
                }
            }
            true
        });

        Self {
            name: value.name,
            requires_dist,
            provides_extras: value.provides_extras,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use uv_normalize::PackageName;

    #[test]
    fn test_flat_requires_dist_noop() {
        let requires_dist = RequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("black; extra == 'dev'").unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let expected = FlatRequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("black; extra == 'dev'").unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let actual = FlatRequiresDist::from(requires_dist);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_basic() {
        let requires_dist = RequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("pkg[dev]; extra == 'test'").unwrap(),
                Requirement::from_str("black; extra == 'dev'").unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let expected = FlatRequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("black; extra == 'dev'").unwrap(),
                Requirement::from_str("black; extra == 'test'").unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let actual = FlatRequiresDist::from(requires_dist);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_flat_requires_dist_with_markers() {
        let requires_dist = RequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("pkg[dev]; extra == 'test' and sys_platform == 'win32'")
                    .unwrap(),
                Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'").unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let expected = FlatRequiresDist {
            name: PackageName::from_str("pkg").unwrap(),
            requires_dist: vec![
                Requirement::from_str("requests>=2.0.0").unwrap(),
                Requirement::from_str("pytest; extra == 'test'").unwrap(),
                Requirement::from_str("black; extra == 'dev' and sys_platform == 'win32'").unwrap(),
                Requirement::from_str("black; extra == 'test' and sys_platform == 'win32'")
                    .unwrap(),
            ],
            provides_extras: vec![
                ExtraName::from_str("test").unwrap(),
                ExtraName::from_str("dev").unwrap(),
            ],
        };

        let actual = FlatRequiresDist::from(requires_dist);

        assert_eq!(actual, expected);
    }
}
