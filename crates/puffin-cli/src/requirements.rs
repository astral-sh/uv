use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{bail, Result};
use fs_err as fs;

use pep508_rs::Requirement;
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

impl RequirementsSource {
    /// Return an iterator over the requirements in this source.
    pub(crate) fn requirements(&self) -> Result<impl Iterator<Item = Requirement>> {
        let iter_name = if let Self::Name(name) = self {
            let requirement = Requirement::from_str(name)?;
            Some(std::iter::once(requirement))
        } else {
            None
        };

        let iter_requirements_txt = if let Self::RequirementsTxt(path) = self {
            let requirements_txt = RequirementsTxt::parse(path, std::env::current_dir()?)?;
            if !requirements_txt.constraints.is_empty() {
                bail!("Constraints in requirements files are not supported");
            }
            Some(
                requirements_txt
                    .requirements
                    .into_iter()
                    .map(|entry| entry.requirement),
            )
        } else {
            None
        };

        let iter_pyproject_toml = if let Self::PyprojectToml(path) = self {
            let pyproject_toml =
                toml::from_str::<pyproject_toml::PyProjectToml>(&fs::read_to_string(path)?)?;
            Some(
                pyproject_toml
                    .project
                    .into_iter()
                    .flat_map(|project| project.dependencies.into_iter().flatten()),
            )
        } else {
            None
        };

        Ok(iter_name
            .into_iter()
            .flatten()
            .chain(iter_requirements_txt.into_iter().flatten())
            .chain(iter_pyproject_toml.into_iter().flatten()))
    }
}
