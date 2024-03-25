use indexmap::IndexMap;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use pep508_rs::Requirement;
use uv_normalize::{ExtraName, PackageName};

use crate::ExtrasSpecification;

/// A pyproject.toml as specified in PEP 517
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct PyProjectToml {
    /// Project metadata
    pub(crate) project: Option<Project>,
}

/// PEP 621 project metadata
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Project {
    /// The name of the project
    pub(crate) name: PackageName,
    /// Project dependencies
    pub(crate) dependencies: Option<Vec<Requirement>>,
    /// Optional dependencies
    pub(crate) optional_dependencies: Option<IndexMap<ExtraName, Vec<Requirement>>>,
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

impl Pep621Metadata {
    pub(crate) fn try_from(project: Project, extras: &ExtrasSpecification) -> Option<Self> {
        if let Some(dynamic) = project.dynamic.as_ref() {
            // If the project specifies dynamic dependencies, we can't extract the requirements.
            if dynamic.iter().any(|field| field == "dependencies") {
                return None;
            }
            // If we requested extras, and the project specifies dynamic optional dependencies, we can't
            // extract the requirements.
            if !extras.is_empty() && dynamic.iter().any(|field| field == "optional-dependencies") {
                return None;
            }
        }

        let mut requirements = Vec::new();
        let mut used_extras = FxHashSet::default();

        // Include the default dependencies.
        requirements.extend(project.dependencies.unwrap_or_default());

        // Include any optional dependencies specified in `extras`.
        let name = project.name;
        if !extras.is_empty() {
            if let Some(optional_dependencies) = project.optional_dependencies {
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

        Some(Self {
            name,
            requirements,
            used_extras,
        })
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
