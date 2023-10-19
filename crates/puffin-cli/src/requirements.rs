use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use itertools::Either;

use pep508_rs::Requirement;
use puffin_package::requirements_txt::RequirementsTxt;

#[derive(Debug)]
pub(crate) enum RequirementsSource {
    /// A dependency was provided on the command line (e.g., `pip install flask`).
    Name(String),
    /// Dependencies were provided via a `requirements.txt` file (e.g., `pip install -r requirements.txt`).
    Path(PathBuf),
}

impl From<String> for RequirementsSource {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

impl From<PathBuf> for RequirementsSource {
    fn from(path: PathBuf) -> Self {
        Self::Path(path)
    }
}

impl RequirementsSource {
    /// Return an iterator over the requirements in this source.
    pub(crate) fn requirements(&self) -> Result<impl Iterator<Item = Requirement>> {
        match self {
            Self::Name(name) => {
                let requirement = Requirement::from_str(name)?;
                Ok(Either::Left(std::iter::once(requirement)))
            }
            Self::Path(path) => {
                let requirements_txt = RequirementsTxt::parse(path, std::env::current_dir()?)?;
                Ok(Either::Right(
                    requirements_txt
                        .requirements
                        .into_iter()
                        .map(|entry| entry.requirement),
                ))
            }
        }
    }
}
