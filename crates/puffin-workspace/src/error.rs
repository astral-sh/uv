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
}
