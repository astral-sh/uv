//! Failures specific to project metadata and editing.

use std::{fmt, io};

use owo_colors::OwoColorize;
use uv_command_support::UvError;
use uv_environment_operations::EnvironmentError;
use uv_errors::{Hinted, Hints};
use uv_workspace::WorkspaceError;

/// A failure from project metadata, editing, or a shared workflow.
#[derive(thiserror::Error, Debug)]
pub enum ProjectError {
    #[error("Failed to parse `pyproject.toml`")]
    PyprojectTomlParse(#[source] uv_workspace::pyproject::PyprojectTomlError),

    #[error("Failed to update `pyproject.toml`")]
    PyprojectTomlUpdate,

    #[error("Failed to parse PEP 723 script metadata")]
    Pep723ScriptTomlParse(#[source] toml::de::Error),

    #[error(transparent)]
    PyprojectMut(#[from] uv_project_edit::Error),

    #[error(transparent)]
    Lock(#[from] uv_lock_operations::LockError),

    #[error(transparent)]
    Environment(#[from] EnvironmentError),

    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error(transparent)]
    MissingProjectVersion(WorkspaceError),

    #[error(transparent)]
    Fmt(#[from] fmt::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<ProjectError> for UvError {
    fn from(error: ProjectError) -> Self {
        match error {
            ProjectError::Lock(error) => Self::from(error),
            ProjectError::Environment(error) => Self::from(error),
            error @ (ProjectError::PyprojectTomlParse(_)
            | ProjectError::PyprojectTomlUpdate
            | ProjectError::Pep723ScriptTomlParse(_)
            | ProjectError::PyprojectMut(_)
            | ProjectError::Workspace(_)
            | ProjectError::MissingProjectVersion(_)
            | ProjectError::Fmt(_)
            | ProjectError::Io(_)
            | ProjectError::Anyhow(_)) => Self::unexpected(error.into()),
        }
    }
}

impl Hinted for ProjectError {
    fn hints(&self) -> Hints<'_> {
        match self {
            Self::Lock(error) => error.hints(),
            Self::Environment(error) => error.hints(),
            Self::MissingProjectVersion(_) => Hints::from(format!(
                "If you meant to view uv's version, use `{}` instead",
                "uv self version".green()
            )),
            Self::PyprojectTomlParse(_)
            | Self::PyprojectTomlUpdate
            | Self::Pep723ScriptTomlParse(_)
            | Self::PyprojectMut(_)
            | Self::Workspace(_)
            | Self::Fmt(_)
            | Self::Io(_)
            | Self::Anyhow(_) => Hints::none(),
        }
    }
}
