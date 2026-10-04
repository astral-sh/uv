//! Failures specific to project metadata and editing.

use std::{fmt, io};

use uv_client::{ClientBuildError, FlatIndexError};
use uv_command_support::UvError;
use uv_distribution::{LoweringError, MetadataError};
use uv_distribution_types::{ExtraBuildRequiresError, IndexCredentialsError};
use uv_environment_operations::EnvironmentError;
use uv_errors::{Hinted, Hints};
use uv_install_operations::Error as InstallError;
use uv_lock::LockError;
use uv_python_context::PythonContextError;
use uv_resolve_operations::Error as ResolveError;
use uv_types::HashStrategyError;
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::{DefaultGroupsError, WorkspaceError};

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
    PyprojectMut(#[from] uv_workspace::pyproject_mut::Error),

    #[error(transparent)]
    Lock(#[from] uv_lock_operations::LockError),

    #[error(transparent)]
    Environment(#[from] EnvironmentError),
}

impl From<ProjectError> for UvError {
    fn from(error: ProjectError) -> Self {
        match error {
            ProjectError::Lock(error) => Self::from(error),
            ProjectError::Environment(error) => Self::from(error),
            error @ (ProjectError::PyprojectTomlParse(_)
            | ProjectError::PyprojectTomlUpdate
            | ProjectError::Pep723ScriptTomlParse(_)
            | ProjectError::PyprojectMut(_)) => Self::unexpected(error.into()),
        }
    }
}

impl Hinted for ProjectError {
    fn hints(&self) -> Hints<'_> {
        match self {
            Self::Lock(error) => error.hints(),
            Self::Environment(error) => error.hints(),
            Self::PyprojectTomlParse(_)
            | Self::PyprojectTomlUpdate
            | Self::Pep723ScriptTomlParse(_)
            | Self::PyprojectMut(_) => Hints::none(),
        }
    }
}

impl From<ClientBuildError> for ProjectError {
    fn from(error: ClientBuildError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<FlatIndexError> for ProjectError {
    fn from(error: FlatIndexError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<LoweringError> for ProjectError {
    fn from(error: LoweringError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<MetadataError> for ProjectError {
    fn from(error: MetadataError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<ExtraBuildRequiresError> for ProjectError {
    fn from(error: ExtraBuildRequiresError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<IndexCredentialsError> for ProjectError {
    fn from(error: IndexCredentialsError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<InstallError> for ProjectError {
    fn from(error: InstallError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<LockError> for ProjectError {
    fn from(error: LockError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<PythonContextError> for ProjectError {
    fn from(error: PythonContextError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<ResolveError> for ProjectError {
    fn from(error: ResolveError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<HashStrategyError> for ProjectError {
    fn from(error: HashStrategyError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<DependencyGroupError> for ProjectError {
    fn from(error: DependencyGroupError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<DefaultGroupsError> for ProjectError {
    fn from(error: DefaultGroupsError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<WorkspaceError> for ProjectError {
    fn from(error: WorkspaceError) -> Self {
        Self::Environment(error.into())
    }
}

impl From<fmt::Error> for ProjectError {
    fn from(error: fmt::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<io::Error> for ProjectError {
    fn from(error: io::Error) -> Self {
        Self::Environment(error.into())
    }
}

impl From<anyhow::Error> for ProjectError {
    fn from(error: anyhow::Error) -> Self {
        Self::Environment(error.into())
    }
}
