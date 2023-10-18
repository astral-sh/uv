use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorkspaceError {
    #[error(transparent)]
    IO(#[from] io::Error),

    #[error(transparent)]
    InvalidToml(#[from] toml_edit::TomlError),

    #[error(transparent)]
    InvalidPyproject(#[from] toml_edit::de::Error),

    #[error(transparent)]
    InvalidRequirement(#[from] pep508_rs::Pep508Error),

    #[error("no `[project]` table found in `pyproject.toml`")]
    MissingProjectTable,

    #[error("no `[project.dependencies]` array found in `pyproject.toml`")]
    MissingProjectDependenciesArray,

    #[error("unable to find package: `{0}`")]
    MissingPackage(String),
}
