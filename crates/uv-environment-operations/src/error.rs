use std::path::PathBuf;

use uv_audit::osv;
use uv_auth::CredentialsFromUrlError;
use uv_command_support::UvError;
use uv_distribution_types::{IndexCredentialsError, IndexUrlError, RequiresPython};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_python_discovery::format_requires_python_sources;
use uv_requirements::ScriptRequirementsError;
use uv_workspace::RequiresPythonSources;
use uv_workspace::dependency_groups::DependencyGroupError;

use crate::ConflictError;

/// A failure while resolving, creating, or updating a Python environment.
#[derive(thiserror::Error, Debug)]
pub enum EnvironmentError {
    #[error(transparent)]
    Conflict(#[from] ConflictError),

    #[error("Group `{0}` is not defined in the project's `dependency-groups` table")]
    MissingGroupProject(GroupName),

    #[error("Group `{0}` is not defined in any project's `dependency-groups` table")]
    MissingGroupProjects(GroupName),

    #[error("PEP 723 scripts do not support dependency groups, but group `{0}` was specified")]
    MissingGroupScript(GroupName),

    #[error("Extra `{0}` is not defined in the `optional-dependencies` table for `{1}`")]
    MissingExtraProject(ExtraName, PackageName),

    #[error("Extra `{0}` is not defined in any project's `optional-dependencies` table")]
    MissingExtraProjects(ExtraName),

    #[error("PEP 723 scripts do not support optional dependencies, but extra `{0}` was specified")]
    MissingExtraScript(ExtraName),

    #[error(
        "Found conflicting Python requirements:\n- lockfile: {locked}\n{groups}",
        groups = format_requires_python_sources(.groups)
    )]
    DisjointLockedRequiresPython {
        locked: RequiresPython,
        groups: RequiresPythonSources,
    },

    #[error(
        "The current Python version ({0}) is not compatible with the locked Python requirement: `{1}`"
    )]
    LockedPythonIncompatibility(Version, RequiresPython),

    #[error(
        "The current Python platform is not compatible with the lockfile's supported environments: {0}"
    )]
    LockedPlatformIncompatibility(String),

    #[error("Project virtual environment directory `{0}` cannot be used because {1}")]
    InvalidProjectEnvironmentDir(PathBuf, String),

    #[error(
        "Malware detected in one or more dependencies that would be installed; aborting sync. Set `UV_MALWARE_CHECK=0` to bypass this check."
    )]
    MalwareFound,

    #[error("Malware check failed due to an error from OSV")]
    Osv(#[from] osv::Error),

    #[error("Attempted to drop a temporary virtual environment while still in-use")]
    DroppedEnvironment,

    #[error(transparent)]
    DependencyGroup(#[from] DependencyGroupError),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    ClientBuild(#[from] uv_client::ClientBuildError),

    #[error(transparent)]
    Credentials(#[from] CredentialsFromUrlError),

    #[error(transparent)]
    IndexCredentials(#[from] IndexCredentialsError),

    #[error(transparent)]
    IndexUrl(#[from] IndexUrlError),

    #[error(transparent)]
    Python(#[from] Box<uv_python_discovery::Error>),

    #[error(transparent)]
    PythonSelection(#[from] Box<uv_python_discovery::PythonSelectionError>),

    #[error(transparent)]
    Virtualenv(#[from] uv_virtualenv::Error),

    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Tags(#[from] uv_platform_tags::TagsError),

    #[error(transparent)]
    FlatIndex(#[from] Box<uv_client::FlatIndexError>),

    #[error(transparent)]
    Lock(#[from] uv_lock::LockError),

    #[error(transparent)]
    Resolve(#[from] Box<uv_resolve_operations::Error>),

    #[error(transparent)]
    Install(#[from] Box<uv_install_operations::Error>),

    #[error(transparent)]
    Interpreter(#[from] uv_python_interpreter::InterpreterError),

    #[error(transparent)]
    Name(#[from] uv_normalize::InvalidNameError),

    #[error(transparent)]
    Requirements(#[from] uv_requirements::Error),

    #[error(transparent)]
    Metadata(#[from] uv_distribution::MetadataError),

    #[error(transparent)]
    Lowering(#[from] Box<uv_distribution::LoweringError>),

    #[error(transparent)]
    Workspace(#[from] uv_workspace::WorkspaceError),

    #[error(transparent)]
    DefaultGroups(#[from] uv_workspace::DefaultGroupsError),

    #[error(transparent)]
    ExtraBuildRequires(#[from] uv_distribution_types::ExtraBuildRequiresError),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    CacheInfo(#[from] uv_cache_info::CacheInfoError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    RetryParsing(#[from] uv_client::RetryParsingError),

    #[error(transparent)]
    Accelerator(#[from] uv_torch::AcceleratorError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<uv_python_discovery::Error> for EnvironmentError {
    fn from(error: uv_python_discovery::Error) -> Self {
        Self::Python(Box::new(error))
    }
}

impl From<uv_python_interpreter::PythonEnvironmentError> for EnvironmentError {
    fn from(error: uv_python_interpreter::PythonEnvironmentError) -> Self {
        Self::Python(Box::new(error.into()))
    }
}

impl From<uv_python_discovery::PythonSelectionError> for EnvironmentError {
    fn from(error: uv_python_discovery::PythonSelectionError) -> Self {
        Self::PythonSelection(Box::new(error))
    }
}

impl From<uv_client::FlatIndexError> for EnvironmentError {
    fn from(error: uv_client::FlatIndexError) -> Self {
        Self::FlatIndex(Box::new(error))
    }
}

impl From<uv_distribution::LoweringError> for EnvironmentError {
    fn from(error: uv_distribution::LoweringError) -> Self {
        Self::Lowering(Box::new(error))
    }
}

impl From<uv_resolve_operations::Error> for EnvironmentError {
    fn from(error: uv_resolve_operations::Error) -> Self {
        Self::Resolve(Box::new(error))
    }
}

impl From<uv_install_operations::Error> for EnvironmentError {
    fn from(error: uv_install_operations::Error) -> Self {
        Self::Install(Box::new(error))
    }
}

impl From<EnvironmentError> for UvError {
    fn from(error: EnvironmentError) -> Self {
        match error {
            EnvironmentError::Resolve(error) => Self::from(*error),
            EnvironmentError::Install(error) => Self::from(*error),
            EnvironmentError::Requirements(error) => {
                Self::from(uv_resolve_operations::Error::Requirements(error))
            }
            error @ (EnvironmentError::Conflict(..)
            | EnvironmentError::MissingGroupProject(..)
            | EnvironmentError::MissingGroupProjects(..)
            | EnvironmentError::MissingGroupScript(..)
            | EnvironmentError::MissingExtraProject(..)
            | EnvironmentError::MissingExtraProjects(..)
            | EnvironmentError::MissingExtraScript(..)
            | EnvironmentError::DisjointLockedRequiresPython { .. }
            | EnvironmentError::LockedPythonIncompatibility(..)
            | EnvironmentError::LockedPlatformIncompatibility(..)
            | EnvironmentError::InvalidProjectEnvironmentDir(..)
            | EnvironmentError::MalwareFound
            | EnvironmentError::Osv(..)
            | EnvironmentError::DroppedEnvironment
            | EnvironmentError::DependencyGroup(..)
            | EnvironmentError::Client(..)
            | EnvironmentError::ClientBuild(..)
            | EnvironmentError::Credentials(..)
            | EnvironmentError::IndexCredentials(..)
            | EnvironmentError::IndexUrl(..)
            | EnvironmentError::Python(..)
            | EnvironmentError::PythonSelection(..)
            | EnvironmentError::Virtualenv(..)
            | EnvironmentError::HashStrategy(..)
            | EnvironmentError::Tags(..)
            | EnvironmentError::FlatIndex(..)
            | EnvironmentError::Lock(..)
            | EnvironmentError::Interpreter(..)
            | EnvironmentError::Name(..)
            | EnvironmentError::Metadata(..)
            | EnvironmentError::Lowering(..)
            | EnvironmentError::Workspace(..)
            | EnvironmentError::DefaultGroups(..)
            | EnvironmentError::ExtraBuildRequires(..)
            | EnvironmentError::Fmt(..)
            | EnvironmentError::CacheInfo(..)
            | EnvironmentError::Io(..)
            | EnvironmentError::RetryParsing(..)
            | EnvironmentError::Accelerator(..)
            | EnvironmentError::Anyhow(..)) => Self::unexpected(error.into()),
        }
    }
}

impl uv_errors::Hinted for EnvironmentError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::Lock(error) => error.hints(),
            Self::Python(error) => error.hints(),
            Self::PythonSelection(error) => error.hints(),
            Self::Resolve(error) => error.hints(),
            Self::Install(error) => error.hints(),
            Self::Client(error) => error.hints(),
            Self::Conflict(..)
            | Self::MissingGroupProject(..)
            | Self::MissingGroupProjects(..)
            | Self::MissingGroupScript(..)
            | Self::MissingExtraProject(..)
            | Self::MissingExtraProjects(..)
            | Self::MissingExtraScript(..)
            | Self::DisjointLockedRequiresPython { .. }
            | Self::LockedPythonIncompatibility(..)
            | Self::LockedPlatformIncompatibility(..)
            | Self::InvalidProjectEnvironmentDir(..)
            | Self::MalwareFound
            | Self::Osv(..)
            | Self::DroppedEnvironment
            | Self::DependencyGroup(..)
            | Self::ClientBuild(..)
            | Self::Credentials(..)
            | Self::IndexCredentials(..)
            | Self::IndexUrl(..)
            | Self::Virtualenv(..)
            | Self::HashStrategy(..)
            | Self::Tags(..)
            | Self::FlatIndex(..)
            | Self::Interpreter(..)
            | Self::Name(..)
            | Self::Requirements(..)
            | Self::Metadata(..)
            | Self::Lowering(..)
            | Self::Workspace(..)
            | Self::DefaultGroups(..)
            | Self::ExtraBuildRequires(..)
            | Self::Fmt(..)
            | Self::CacheInfo(..)
            | Self::Io(..)
            | Self::RetryParsing(..)
            | Self::Accelerator(..)
            | Self::Anyhow(..) => uv_errors::Hints::none(),
        }
    }
}

impl From<ScriptRequirementsError> for EnvironmentError {
    fn from(error: ScriptRequirementsError) -> Self {
        match error {
            ScriptRequirementsError::Io(error) => Self::Io(error),
            ScriptRequirementsError::IndexUrl(error) => Self::IndexUrl(error),
            ScriptRequirementsError::Lowering(error) => Self::Lowering(error),
        }
    }
}
