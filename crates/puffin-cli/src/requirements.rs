//! A standard interface for working with heterogeneous sources of requirements.

use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use fs_err as fs;

use pep508_rs::Requirement;
use puffin_normalize::{ExtraName, PackageName};
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
pub(crate) enum ExtrasSpecification<'a> {
    #[default]
    None,
    All,
    Some(&'a [ExtraName]),
}

impl ExtrasSpecification<'_> {
    /// Returns true if a name is included in the extra specification.
    fn contains(&self, name: &ExtraName) -> bool {
        match self {
            ExtrasSpecification::All => true,
            ExtrasSpecification::None => false,
            ExtrasSpecification::Some(extras) => extras.contains(name),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RequirementsSpecification {
    /// The name of the project specifying requirements.
    pub(crate) project: Option<PackageName>,
    /// The requirements for the project.
    pub(crate) requirements: Vec<Requirement>,
    /// The constraints for the project.
    pub(crate) constraints: Vec<Requirement>,
    /// The extras used to collect requirements.
    pub(crate) extras: HashSet<ExtraName>,
}

impl RequirementsSpecification {
    /// Read the requirements and constraints from a source.
    pub(crate) fn try_from_source(
        source: &RequirementsSource,
        extras: &ExtrasSpecification,
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Name(name) => {
                let requirement = Requirement::from_str(name)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    project: None,
                    requirements: vec![requirement],
                    constraints: vec![],
                    extras: HashSet::new(),
                }
            }
            RequirementsSource::RequirementsTxt(path) => {
                let requirements_txt = RequirementsTxt::parse(path, std::env::current_dir()?)?;
                Self {
                    project: None,
                    requirements: requirements_txt
                        .requirements
                        .into_iter()
                        .map(|entry| entry.requirement)
                        .collect(),
                    constraints: requirements_txt.constraints.into_iter().collect(),
                    extras: HashSet::new(),
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = fs::read_to_string(path)?;
                let pyproject_toml = toml::from_str::<pyproject_toml::PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to read `{}`", path.display()))?;
                let mut used_extras = HashSet::new();
                let mut requirements = Vec::new();
                let mut project_name = None;
                if let Some(project) = pyproject_toml.project {
                    requirements.extend(project.dependencies.unwrap_or_default());
                    // Include any optional dependencies specified in `extras`
                    if !matches!(extras, ExtrasSpecification::None) {
                        for (name, optional_requirements) in
                            project.optional_dependencies.unwrap_or_default()
                        {
                            let normalized_name = ExtraName::new(name);
                            if extras.contains(&normalized_name) {
                                used_extras.insert(normalized_name);
                                requirements.extend(optional_requirements);
                            }
                        }
                    }
                    // Parse the project name
                    project_name = Some(PackageName::new(project.name));
                }

                Self {
                    project: project_name,
                    requirements,
                    constraints: vec![],
                    extras: used_extras,
                }
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub(crate) fn try_from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        extras: &ExtrasSpecification,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source = Self::try_from_source(source, extras)?;
            spec.requirements.extend(source.requirements);
            spec.constraints.extend(source.constraints);
            spec.extras.extend(source.extras);

            // Use the first project name discovered
            if spec.project.is_none() {
                spec.project = source.project;
            }
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
