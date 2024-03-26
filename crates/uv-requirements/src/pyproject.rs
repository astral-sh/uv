use indexmap::IndexMap;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use pep508_rs::Requirement;
use pypi_types::LenientRequirement;
use uv_normalize::{ExtraName, PackageName};

use crate::ExtrasSpecification;

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PyProjectToml {
    /// Project metadata
    pub(crate) project: Option<Project>,
}

/// PEP 621 project metadata.
///
/// This is a subset of the full metadata specification, and only includes the fields that are
/// relevant for extracting static requirements.
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Project {
    /// The name of the project
    pub(crate) name: PackageName,
    /// Project dependencies
    pub(crate) dependencies: Option<Vec<String>>,
    /// Optional dependencies
    pub(crate) optional_dependencies: Option<IndexMap<ExtraName, Vec<String>>>,
    /// Specifies which fields listed by PEP 621 were intentionally unspecified
    /// so another tool can/will provide such metadata dynamically.
    pub(crate) dynamic: Option<Vec<String>>,
}

/// The PEP 621 project metadata, with static requirements extracted in advance.
#[derive(Debug)]
pub(crate) struct Pep621Metadata {
    /// The name of the project.
    pub(crate) name: PackageName,
    /// The requirements extracted from the project.
    pub(crate) requirements: Vec<Requirement>,
    /// The extras used to collect requirements.
    pub(crate) used_extras: FxHashSet<ExtraName>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Pep621Error {
    #[error(transparent)]
    Pep508(#[from] pep508_rs::Pep508Error),
}

impl Pep621Metadata {
    /// Extract the static [`Pep621Metadata`] from a [`Project`] and [`ExtrasSpecification`], if
    /// possible.
    ///
    /// If the project specifies dynamic dependencies, or if the project specifies dynamic optional
    /// dependencies and the extras are requested, the requirements cannot be extracted.
    ///
    /// Returns an error if the requirements are not valid PEP 508 requirements.
    pub(crate) fn try_from(
        project: Project,
        extras: &ExtrasSpecification,
    ) -> Result<Option<Self>, Pep621Error> {
        if let Some(dynamic) = project.dynamic.as_ref() {
            // If the project specifies dynamic dependencies, we can't extract the requirements.
            if dynamic.iter().any(|field| field == "dependencies") {
                return Ok(None);
            }
            // If we requested extras, and the project specifies dynamic optional dependencies, we can't
            // extract the requirements.
            if !extras.is_empty() && dynamic.iter().any(|field| field == "optional-dependencies") {
                return Ok(None);
            }
        }

        let name = project.name;

        // Parse out the project requirements.
        let mut requirements = project
            .dependencies
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .map(|s| LenientRequirement::from_str(s).map(Requirement::from))
            .collect::<Result<Vec<_>, _>>()?;

        // Include any optional dependencies specified in `extras`.
        let mut used_extras = FxHashSet::default();
        if !extras.is_empty() {
            if let Some(optional_dependencies) = project.optional_dependencies {
                // Parse out the optional dependencies.
                let optional_dependencies = optional_dependencies
                    .into_iter()
                    .map(|(extra, requirements)| {
                        let requirements = requirements
                            .iter()
                            .map(String::as_str)
                            .map(|s| LenientRequirement::from_str(s).map(Requirement::from))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok::<(ExtraName, Vec<Requirement>), Pep621Error>((extra, requirements))
                    })
                    .collect::<Result<IndexMap<_, _>, _>>()?;

                // Include the optional dependencies if the extras are requested.
                for (extra, optional_requirements) in &optional_dependencies {
                    if extras.contains(extra) {
                        used_extras.insert(extra.clone());
                        requirements.extend(flatten_extra(
                            &name,
                            optional_requirements,
                            &optional_dependencies,
                        ));
                    }
                }
            }
        }

        Ok(Some(Self {
            name,
            requirements,
            used_extras,
        }))
    }
}

/// Given an extra in a project that may contain references to the project
/// itself, flatten it into a list of requirements.
///
/// For example:
/// ```toml
/// [project]
/// name = "my-project"
/// version = "0.0.1"
/// dependencies = [
///     "tomli",
/// ]
///
/// [project.optional-dependencies]
/// test = [
///     "pep517",
/// ]
/// dev = [
///     "my-project[test]",
/// ]
/// ```
fn flatten_extra(
    project_name: &PackageName,
    requirements: &[Requirement],
    extras: &IndexMap<ExtraName, Vec<Requirement>>,
) -> Vec<Requirement> {
    fn inner(
        project_name: &PackageName,
        requirements: &[Requirement],
        extras: &IndexMap<ExtraName, Vec<Requirement>>,
        seen: &mut FxHashSet<ExtraName>,
    ) -> Vec<Requirement> {
        let mut flattened = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            if requirement.name == *project_name {
                for extra in &requirement.extras {
                    // Avoid infinite recursion on mutually recursive extras.
                    if !seen.insert(extra.clone()) {
                        continue;
                    }

                    // Flatten the extra requirements.
                    for (other_extra, extra_requirements) in extras {
                        if other_extra == extra {
                            flattened.extend(inner(project_name, extra_requirements, extras, seen));
                        }
                    }
                }
            } else {
                flattened.push(requirement.clone());
            }
        }
        flattened
    }

    inner(
        project_name,
        requirements,
        extras,
        &mut FxHashSet::default(),
    )
}
