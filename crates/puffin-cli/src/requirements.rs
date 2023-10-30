//! A standard interface for working with heterogeneous sources of requirements.

use std::clone::Clone;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use fs_err as fs;

use pep508_rs::Requirement;
use puffin_package::extra_name::ExtraName;
use puffin_package::requirements_txt::RequirementsTxt;

#[derive(Debug)]
pub(crate) enum RequirementsSource {
    /// A dependency was provided on the command line (e.g., `pip install flask`).
    Name(String),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    RequirementsTxt(PathBuf),
    /// Dependencies were provided via a `pyproject.toml` file (e.g., `pip-compile pyproject.toml`).
    PyprojectToml(PathBuf),
}

impl From<String> for RequirementsSource {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

impl From<PathBuf> for RequirementsSource {
    fn from(path: PathBuf) -> Self {
        if path.ends_with("pyproject.toml") {
            Self::PyprojectToml(path)
        } else {
            Self::RequirementsTxt(path)
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RequirementsSpecification {
    /// The requirements for the project.
    pub(crate) requirements: Vec<Requirement>,
    /// The constraints for the project.
    pub(crate) constraints: Vec<Requirement>,
}

impl RequirementsSpecification {
    /// Read the requirements and constraints from a source.
    pub(crate) fn try_from_source(
        source: &RequirementsSource,
        extras: &[ExtraName],
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Name(name) => {
                let requirement = Requirement::from_str(name)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    requirements: vec![requirement],
                    constraints: vec![],
                }
            }
            RequirementsSource::RequirementsTxt(path) => {
                let requirements_txt = RequirementsTxt::parse(path, std::env::current_dir()?)?;
                Self {
                    requirements: requirements_txt
                        .requirements
                        .into_iter()
                        .map(|entry| entry.requirement)
                        .collect(),
                    constraints: requirements_txt.constraints.into_iter().collect(),
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = fs::read_to_string(path)?;
                let pyproject_toml = toml::from_str::<pyproject_toml::PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to read `{}`", path.display()))?;
                let requirements: Vec<Requirement> = pyproject_toml
                    .project
                    .into_iter()
                    .flat_map(|project| {
                        project.dependencies.into_iter().flatten().chain(
                            // Include any optional dependencies specified in `extras`
                            project.optional_dependencies.into_iter().flat_map(
                                |optional_dependencies| {
                                    extras.iter().flat_map(move |extra| {
                                        optional_dependencies
                                            .get(extra.as_ref())
                                            .cloned()
                                            // undefined extra requests are ignored silently
                                            .unwrap_or_default()
                                    })
                                },
                            ),
                        )
                    })
                    .collect();

                Self {
                    requirements,
                    constraints: vec![],
                }
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub(crate) fn try_from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        extras: &[ExtraName],
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source = Self::try_from_source(source, extras)?;
            spec.requirements.extend(source.requirements);
            spec.constraints.extend(source.constraints);
        }

        // Read all constraints, treating both requirements _and_ constraints as constraints.
        for source in constraints {
            let source = Self::try_from_source(source, extras)?;
            spec.constraints.extend(source.requirements);
            spec.constraints.extend(source.constraints);
        }

        Ok(spec)
    }
}
