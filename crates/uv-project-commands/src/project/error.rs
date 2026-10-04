//! Failures specific to project commands and lockfile policy.

use std::fmt;
use std::io;
use std::path::PathBuf;

use uv_client::{ClientBuildError, FlatIndexError};
use uv_command_support::UvError;
use uv_distribution::{LoweringError, MetadataError};
use uv_distribution_types::{ExtraBuildRequiresError, IndexCredentialsError, RequiresPython};
use uv_environment_operations::EnvironmentError;
use uv_errors::{Hinted, Hints};
use uv_install_operations::Error as InstallError;
use uv_lock::{Lock, LockError, LockParseError};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::MarkerTreeContents;
use uv_python_context::PythonContextError;
use uv_resolve_operations::Error as ResolveError;
use uv_settings::{FrozenSource, LockedSource};
use uv_types::HashStrategyError;
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::{DefaultGroupsError, WorkspaceError};

/// The source of a missing lockfile error.
#[derive(Debug, Clone, Copy)]
pub enum MissingLockfileSource {
    /// Frozen mode required an existing lockfile.
    Frozen(FrozenSource),
    /// A lock check required an existing lockfile.
    Locked(LockedSource),
}

impl std::fmt::Display for MissingLockfileSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frozen(source) => write!(f, "`{source}`"),
            Self::Locked(source) => write!(f, "`{source}`"),
        }
    }
}

impl From<LockedSource> for MissingLockfileSource {
    fn from(source: LockedSource) -> Self {
        Self::Locked(source)
    }
}

impl From<FrozenSource> for MissingLockfileSource {
    fn from(source: FrozenSource) -> Self {
        Self::Frozen(source)
    }
}

/// A failure from project metadata, lockfile policy, or environment workflows.
#[derive(thiserror::Error, Debug)]
pub enum ProjectError {
    #[error("The lockfile at `uv.lock` needs to be updated, but `{2}` was provided.")]
    LockMismatch(Option<Box<Lock>>, Box<Lock>, LockedSource),

    #[error(
        "The lockfile at `{0}` has non-canonical formatting at line {1}, but `{2}` was provided."
    )]
    LockFormat(PathBuf, usize, LockedSource),

    #[error(
        "Unable to find lockfile at `{1}`, but {0} was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag."
    )]
    MissingLockfile(MissingLockfileSource, PathBuf),

    #[error(
        "The lockfile at `uv.lock` needs to be updated, but {1} was provided: Missing workspace member `{0}`."
    )]
    LockWorkspaceMismatch(PackageName, MissingLockfileSource),

    #[error(
        "The lockfile at `uv.lock` uses an unsupported schema version (v{1}, but only v{0} is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`."
    )]
    UnsupportedLockVersion(u32, u32),

    #[error(
        "Failed to parse `uv.lock`, which uses an unsupported schema version (v{1}, but only v{0} is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`."
    )]
    UnparsableLockVersion(u32, u32, #[source] toml::de::Error),

    #[error("Failed to serialize `uv.lock`")]
    LockSerialization(#[from] toml_edit::ser::Error),

    #[error(
        "The current Python version ({0}) is not compatible with the locked Python requirement: `{1}`"
    )]
    LockedPythonIncompatibility(Version, RequiresPython),

    #[error(
        "The current Python platform is not compatible with the lockfile's supported environments: {0}"
    )]
    LockedPlatformIncompatibility(String),

    #[error(
        "Supported environments must be disjoint, but the following markers overlap: `{0}` and `{1}`"
    )]
    OverlappingMarkers(String, String, String),

    #[error("Environment markers `{0}` don't overlap with Python requirement `{1}`")]
    DisjointEnvironment(MarkerTreeContents, VersionSpecifiers),

    #[error("Environment marker is empty")]
    EmptyEnvironment,

    #[error("Failed to parse `uv.lock`")]
    UvLockParse(#[source] toml::de::Error),

    #[error("Failed to parse `pyproject.toml`")]
    PyprojectTomlParse(#[source] uv_workspace::pyproject::PyprojectTomlError),

    #[error("Failed to update `pyproject.toml`")]
    PyprojectTomlUpdate,

    #[error("Failed to parse PEP 723 script metadata")]
    Pep723ScriptTomlParse(#[source] toml::de::Error),

    #[error(transparent)]
    PyprojectMut(#[from] uv_workspace::pyproject_mut::Error),

    #[error(transparent)]
    Environment(#[from] EnvironmentError),
}

impl From<ProjectError> for UvError {
    fn from(error: ProjectError) -> Self {
        match error {
            error @ (ProjectError::LockMismatch(..)
            | ProjectError::LockFormat(..)
            | ProjectError::MissingLockfile(..)
            | ProjectError::LockWorkspaceMismatch(..)) => Self::user(error),
            ProjectError::Environment(error) => Self::from(error),
            error @ (ProjectError::UnsupportedLockVersion(..)
            | ProjectError::UnparsableLockVersion(..)
            | ProjectError::LockSerialization(_)
            | ProjectError::LockedPythonIncompatibility(..)
            | ProjectError::LockedPlatformIncompatibility(_)
            | ProjectError::OverlappingMarkers(..)
            | ProjectError::DisjointEnvironment(..)
            | ProjectError::EmptyEnvironment
            | ProjectError::UvLockParse(_)
            | ProjectError::PyprojectTomlParse(_)
            | ProjectError::PyprojectTomlUpdate
            | ProjectError::Pep723ScriptTomlParse(_)
            | ProjectError::PyprojectMut(_)) => Self::unexpected(error.into()),
        }
    }
}

impl From<LockParseError> for ProjectError {
    fn from(error: LockParseError) -> Self {
        match error {
            LockParseError::UnsupportedVersion { supported, version } => {
                Self::UnsupportedLockVersion(supported, version)
            }
            LockParseError::UnparsableVersion {
                supported,
                version,
                source,
            } => Self::UnparsableLockVersion(supported, version, source),
            LockParseError::Toml(source) => Self::UvLockParse(source),
        }
    }
}

impl Hinted for ProjectError {
    fn hints(&self) -> Hints<'_> {
        match self {
            Self::LockMismatch(..) | Self::LockWorkspaceMismatch(..) => {
                Hints::from("To update the lockfile, run `uv lock`.")
            }
            Self::LockFormat(..) => Hints::from(
                "To regenerate the lockfile, run `uv lock --refresh --preview-features lockfile-format-check`.",
            ),
            Self::OverlappingMarkers(_, rhs, replacement) => {
                Hints::from(format!("replace `{rhs}` with `{replacement}`"))
            }
            Self::Environment(error) => error.hints(),
            Self::MissingLockfile(..)
            | Self::UnsupportedLockVersion(..)
            | Self::UnparsableLockVersion(..)
            | Self::LockSerialization(_)
            | Self::LockedPythonIncompatibility(..)
            | Self::LockedPlatformIncompatibility(_)
            | Self::DisjointEnvironment(..)
            | Self::EmptyEnvironment
            | Self::UvLockParse(_)
            | Self::PyprojectTomlParse(_)
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
