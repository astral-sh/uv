//! Failures from lockfile policy, parsing, and resolution.

use std::fmt;
use std::io;
use std::path::PathBuf;

use uv_client::{ClientBuildError, FlatIndexError};
use uv_command_support::UvError;
use uv_distribution::{LoweringError, MetadataError};
use uv_distribution_types::{ExtraBuildRequiresError, IndexCredentialsError, IndexUrlError};
use uv_errors::{Hinted, Hints};
use uv_lock::{Lock, LockError as LockDataError, LockParseError};
use uv_normalize::{GroupName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTreeContents;
use uv_platform_tags::TagsError;
use uv_python_discovery::PythonSelectionError;
use uv_requirements::ScriptRequirementsError;
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

/// A failure while reading, validating, or resolving a lockfile.
#[derive(thiserror::Error, Debug)]
pub enum LockError {
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
        "Supported environments must be disjoint, but the following markers overlap: `{0}` and `{1}`"
    )]
    OverlappingMarkers(String, String, String),

    #[error("Environment markers `{0}` don't overlap with Python requirement `{1}`")]
    DisjointEnvironment(MarkerTreeContents, VersionSpecifiers),

    #[error("Environment marker is empty")]
    EmptyEnvironment,

    #[error("Failed to parse `uv.lock`")]
    UvLockParse(#[source] toml::de::Error),

    #[error("Group `{0}` is not defined in the project's `dependency-groups` table")]
    MissingGroupProject(GroupName),

    #[error("Group `{0}` is not defined in any project's `dependency-groups` table")]
    MissingGroupProjects(GroupName),

    #[error("PEP 723 scripts do not support dependency groups, but group `{0}` was specified")]
    MissingGroupScript(GroupName),

    #[error(transparent)]
    ClientBuild(#[from] ClientBuildError),

    #[error(transparent)]
    FlatIndex(#[from] Box<FlatIndexError>),

    #[error(transparent)]
    Lowering(#[from] Box<LoweringError>),

    #[error(transparent)]
    Metadata(#[from] MetadataError),

    #[error(transparent)]
    ExtraBuildRequires(#[from] ExtraBuildRequiresError),

    #[error(transparent)]
    IndexCredentials(#[from] IndexCredentialsError),

    #[error(transparent)]
    IndexUrl(#[from] IndexUrlError),

    #[error(transparent)]
    Lock(#[from] LockDataError),

    #[error(transparent)]
    Tags(#[from] TagsError),

    #[error(transparent)]
    PythonSelection(#[from] Box<PythonSelectionError>),

    #[error(transparent)]
    Resolve(#[from] Box<ResolveError>),

    #[error(transparent)]
    HashStrategy(#[from] HashStrategyError),

    #[error(transparent)]
    DependencyGroup(#[from] DependencyGroupError),

    #[error(transparent)]
    DefaultGroups(#[from] DefaultGroupsError),

    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error(transparent)]
    Fmt(#[from] fmt::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl From<LockParseError> for LockError {
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

impl From<LockError> for UvError {
    fn from(error: LockError) -> Self {
        match error {
            error @ (LockError::LockMismatch(..)
            | LockError::LockFormat(..)
            | LockError::MissingLockfile(..)
            | LockError::LockWorkspaceMismatch(..)) => Self::user(error),
            LockError::Resolve(error) => Self::from(*error),
            error @ (LockError::UnsupportedLockVersion(..)
            | LockError::UnparsableLockVersion(..)
            | LockError::LockSerialization(_)
            | LockError::OverlappingMarkers(..)
            | LockError::DisjointEnvironment(..)
            | LockError::EmptyEnvironment
            | LockError::UvLockParse(_)
            | LockError::MissingGroupProject(_)
            | LockError::MissingGroupProjects(_)
            | LockError::MissingGroupScript(_)
            | LockError::ClientBuild(_)
            | LockError::FlatIndex(_)
            | LockError::Lowering(_)
            | LockError::Metadata(_)
            | LockError::ExtraBuildRequires(_)
            | LockError::IndexCredentials(_)
            | LockError::IndexUrl(_)
            | LockError::Lock(_)
            | LockError::Tags(_)
            | LockError::PythonSelection(_)
            | LockError::HashStrategy(_)
            | LockError::DependencyGroup(_)
            | LockError::DefaultGroups(_)
            | LockError::Workspace(_)
            | LockError::Fmt(_)
            | LockError::Io(_)
            | LockError::Anyhow(_)) => Self::unexpected(error.into()),
        }
    }
}

impl Hinted for LockError {
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
            Self::Resolve(error) => error.hints(),
            Self::Lock(error) => error.hints(),
            Self::PythonSelection(error) => error.hints(),
            Self::MissingLockfile(..)
            | Self::UnsupportedLockVersion(..)
            | Self::UnparsableLockVersion(..)
            | Self::LockSerialization(_)
            | Self::DisjointEnvironment(..)
            | Self::EmptyEnvironment
            | Self::UvLockParse(_)
            | Self::MissingGroupProject(_)
            | Self::MissingGroupProjects(_)
            | Self::MissingGroupScript(_)
            | Self::ClientBuild(_)
            | Self::FlatIndex(_)
            | Self::Lowering(_)
            | Self::Metadata(_)
            | Self::ExtraBuildRequires(_)
            | Self::IndexCredentials(_)
            | Self::IndexUrl(_)
            | Self::Tags(_)
            | Self::HashStrategy(_)
            | Self::DependencyGroup(_)
            | Self::DefaultGroups(_)
            | Self::Workspace(_)
            | Self::Fmt(_)
            | Self::Io(_)
            | Self::Anyhow(_) => Hints::none(),
        }
    }
}

impl From<FlatIndexError> for LockError {
    fn from(error: FlatIndexError) -> Self {
        Self::FlatIndex(Box::new(error))
    }
}

impl From<LoweringError> for LockError {
    fn from(error: LoweringError) -> Self {
        Self::Lowering(Box::new(error))
    }
}

impl From<PythonSelectionError> for LockError {
    fn from(error: PythonSelectionError) -> Self {
        Self::PythonSelection(Box::new(error))
    }
}

impl From<ResolveError> for LockError {
    fn from(error: ResolveError) -> Self {
        Self::Resolve(Box::new(error))
    }
}

impl From<ScriptRequirementsError> for LockError {
    fn from(error: ScriptRequirementsError) -> Self {
        match error {
            ScriptRequirementsError::Io(error) => Self::Io(error),
            ScriptRequirementsError::IndexUrl(error) => Self::IndexUrl(error),
            ScriptRequirementsError::Lowering(error) => Self::Lowering(error),
        }
    }
}

impl From<LockValidationError> for LockError {
    fn from(error: LockValidationError) -> Self {
        match error {
            LockValidationError::Lock(error) => Self::Lock(error),
            LockValidationError::Tags(error) => Self::Tags(error),
        }
    }
}

/// A failure while validating an existing lockfile against its requirements.
#[derive(Debug, thiserror::Error)]
pub enum LockValidationError {
    #[error(transparent)]
    Lock(#[from] LockDataError),

    #[error(transparent)]
    Tags(#[from] TagsError),
}
