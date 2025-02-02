use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DevGroupsManifest, DevGroupsSpecification, ExtrasSpecification,
    GroupsSpecification, LowerBound, PreviewMode, Reinstall, TrustedHost, Upgrade,
};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    Index, Resolution, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use uv_fs::{Simplified, CWD};
use uv_git::ResolvedRepositoryReference;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::{GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::MarkerTreeContents;
use uv_pypi_types::{ConflictPackage, ConflictSet, Conflicts, Requirement};
use uv_python::{
    EnvironmentPreference, Interpreter, InvalidEnvironmentKind, PythonDownloads, PythonEnvironment,
    PythonInstallation, PythonPreference, PythonRequest, PythonVariant, PythonVersionFile,
    VersionFileDiscoveryOptions, VersionRequest,
};
use uv_requirements::upgrade::{read_lock_requirements, LockedRequirements};
use uv_requirements::{NamedRequirementsResolver, RequirementsSpecification};
use uv_resolver::{
    FlatIndex, Lock, OptionsBuilder, PythonRequirement, RequiresPython, ResolverEnvironment,
    ResolverOutput,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::pyproject::PyProjectToml;
use uv_workspace::{ProjectWorkspace, Workspace};

use crate::commands::pip::loggers::{InstallLogger, ResolveLogger};
use crate::commands::pip::operations::{Changelog, Modifications};
use crate::commands::reporters::{PythonDownloadReporter, ResolverReporter};
use crate::commands::{capitalize, conjunction, pip};
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverInstallerSettings, ResolverSettingsRef};

pub(crate) mod add;
pub(crate) mod environment;
pub(crate) mod export;
pub(crate) mod init;
mod install_target;
pub(crate) mod lock;
mod lock_target;
pub(crate) mod remove;
pub(crate) mod run;
pub(crate) mod sync;
pub(crate) mod tree;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ProjectError {
    #[error("The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.")]
    LockMismatch,

    #[error(
        "Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`."
    )]
    MissingLockfile,

    #[error("The lockfile at `uv.lock` uses an unsupported schema version (v{1}, but only v{0} is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.")]
    UnsupportedLockVersion(u32, u32),

    #[error("Failed to parse `uv.lock`, which uses an unsupported schema version (v{1}, but only v{0} is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.")]
    UnparsableLockVersion(u32, u32, #[source] toml::de::Error),

    #[error("Failed to serialize `uv.lock`")]
    LockSerialization(#[from] toml_edit::ser::Error),

    #[error("The current Python version ({0}) is not compatible with the locked Python requirement: `{1}`")]
    LockedPythonIncompatibility(Version, RequiresPython),

    #[error("The current Python platform is not compatible with the lockfile's supported environments: {0}")]
    LockedPlatformIncompatibility(String),

    #[error(transparent)]
    Conflict(#[from] ConflictError),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the project's Python requirement: `{1}`")]
    RequestedPythonProjectIncompatibility(Version, RequiresPython),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the project's Python requirement: `{2}`. Use `uv python pin` to update the `.python-version` file to a compatible version.")]
    DotPythonVersionProjectIncompatibility(String, Version, RequiresPython),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the project's Python requirement: `{1}`")]
    RequiresPythonProjectIncompatibility(Version, RequiresPython),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the script's Python requirement: `{1}`")]
    RequestedPythonScriptIncompatibility(Version, RequiresPython),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the script's Python requirement: `{2}`")]
    DotPythonVersionScriptIncompatibility(String, Version, RequiresPython),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the script's Python requirement: `{1}`")]
    RequiresPythonScriptIncompatibility(Version, RequiresPython),

    #[error("The requested interpreter resolved to Python {0}, which is incompatible with the project's Python requirement: `{1}`. However, a workspace member (`{member}`) supports Python {3}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _2.cyan(), venv = format!("uv venv --python {_0}").green(), install = "uv pip install -e .".green(), path = _4.user_display().cyan() )]
    RequestedMemberIncompatibility(
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("The Python request from `{0}` resolved to Python {1}, which is incompatible with the project's Python requirement: `{2}`. However, a workspace member (`{member}`) supports Python {4}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _3.cyan(), venv = format!("uv venv --python {_1}").green(), install = "uv pip install -e .".green(), path = _5.user_display().cyan() )]
    DotPythonVersionMemberIncompatibility(
        String,
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("The resolved Python interpreter (Python {0}) is incompatible with the project's Python requirement: `{1}`. However, a workspace member (`{member}`) supports Python {3}. To install the workspace member on its own, navigate to `{path}`, then run `{venv}` followed by `{install}`.", member = _2.cyan(), venv = format!("uv venv --python {_0}").green(), install = "uv pip install -e .".green(), path = _4.user_display().cyan() )]
    RequiresPythonMemberIncompatibility(
        Version,
        RequiresPython,
        PackageName,
        VersionSpecifiers,
        PathBuf,
    ),

    #[error("Group `{0}` is not defined in the project's `dependency-group` table")]
    MissingGroupProject(GroupName),

    #[error("Group `{0}` is not defined in any project's `dependency-group` table")]
    MissingGroupWorkspace(GroupName),

    #[error("PEP 723 scripts do not support dependency groups, but group `{0}` was specified")]
    MissingGroupScript(GroupName),

    #[error("Default group `{0}` (from `tool.uv.default-groups`) is not defined in the project's `dependency-group` table")]
    MissingDefaultGroup(GroupName),

    #[error("Supported environments must be disjoint, but the following markers overlap: `{0}` and `{1}`.\n\n{hint}{colon} replace `{1}` with `{2}`.", hint = "hint".bold().cyan(), colon = ":".bold())]
    OverlappingMarkers(String, String, String),

    #[error("Environment markers `{0}` don't overlap with Python requirement `{1}`")]
    DisjointEnvironment(MarkerTreeContents, VersionSpecifiers),

    #[error("The workspace contains conflicting Python requirements:\n{}", _0.iter().map(|(name, specifiers)| format!("- `{name}`: `{specifiers}`")).join("\n"))]
    DisjointRequiresPython(BTreeMap<PackageName, VersionSpecifiers>),

    #[error("Environment marker is empty")]
    EmptyEnvironment,

    #[error("Project virtual environment directory `{0}` cannot be used because {1}")]
    InvalidProjectEnvironmentDir(PathBuf, String),

    #[error("Failed to parse `uv.lock`")]
    UvLockParse(#[source] toml::de::Error),

    #[error("Failed to parse `pyproject.toml`")]
    PyprojectTomlParse(#[source] toml::de::Error),

    #[error("Failed to update `pyproject.toml`")]
    PyprojectTomlUpdate,

    #[error("Failed to parse PEP 723 script metadata")]
    Pep723ScriptTomlParse(#[source] toml::de::Error),

    #[error(transparent)]
    DependencyGroup(#[from] DependencyGroupError),

    #[error(transparent)]
    Python(#[from] uv_python::Error),

    #[error(transparent)]
    Virtualenv(#[from] uv_virtualenv::Error),

    #[error(transparent)]
    HashStrategy(#[from] uv_types::HashStrategyError),

    #[error(transparent)]
    Tags(#[from] uv_platform_tags::TagsError),

    #[error(transparent)]
    FlatIndex(#[from] uv_client::FlatIndexError),

    #[error(transparent)]
    Lock(#[from] uv_resolver::LockError),

    #[error(transparent)]
    Operation(#[from] pip::operations::Error),

    #[error(transparent)]
    Interpreter(#[from] uv_python::InterpreterError),

    #[error(transparent)]
    Tool(#[from] uv_tool::Error),

    #[error(transparent)]
    Name(#[from] uv_normalize::InvalidNameError),

    #[error(transparent)]
    Requirements(#[from] uv_requirements::Error),

    #[error(transparent)]
    Metadata(#[from] uv_distribution::MetadataError),

    #[error(transparent)]
    PyprojectMut(#[from] uv_workspace::pyproject_mut::Error),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

#[derive(Debug)]
pub(crate) struct ConflictError {
    /// The set from which the conflict was derived.
    pub(crate) set: ConflictSet,
    /// The items from the set that were enabled, and thus create the conflict.
    pub(crate) conflicts: Vec<ConflictPackage>,
    /// The manifest of enabled dependency groups.
    pub(crate) dev: DevGroupsManifest,
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format the set itself.
        let set = self
            .set
            .iter()
            .map(|item| match item.conflict() {
                ConflictPackage::Extra(ref extra) => format!("`{}[{}]`", item.package(), extra),
                ConflictPackage::Group(ref group) => format!("`{}:{}`", item.package(), group),
            })
            .join(", ");

        // If all the conflicts are of the same kind, show a more succinct error.
        if self
            .conflicts
            .iter()
            .all(|conflict| matches!(conflict, ConflictPackage::Extra(..)))
        {
            write!(
                f,
                "Extras {} are incompatible with the declared conflicts: {{{set}}}",
                conjunction(
                    self.conflicts
                        .iter()
                        .map(|conflict| match conflict {
                            ConflictPackage::Extra(ref extra) => format!("`{extra}`"),
                            ConflictPackage::Group(..) => unreachable!(),
                        })
                        .collect()
                )
            )
        } else if self
            .conflicts
            .iter()
            .all(|conflict| matches!(conflict, ConflictPackage::Group(..)))
        {
            write!(
                f,
                "Groups {} are incompatible with the declared conflicts: {{{set}}}",
                conjunction(
                    self.conflicts
                        .iter()
                        .map(|conflict| match conflict {
                            ConflictPackage::Group(ref group) if self.dev.is_default(group) =>
                                format!("`{group}` (enabled by default)"),
                            ConflictPackage::Group(ref group) => format!("`{group}`"),
                            ConflictPackage::Extra(..) => unreachable!(),
                        })
                        .collect()
                )
            )
        } else {
            write!(
                f,
                "{} are incompatible with the declared conflicts: {{{set}}}",
                conjunction(
                    self.conflicts
                        .iter()
                        .enumerate()
                        .map(|(i, conflict)| {
                            let conflict = match conflict {
                                ConflictPackage::Extra(ref extra) => format!("extra `{extra}`"),
                                ConflictPackage::Group(ref group) if self.dev.is_default(group) => {
                                    format!("group `{group}` (enabled by default)")
                                }
                                ConflictPackage::Group(ref group) => format!("group `{group}`"),
                            };
                            (i == 0).then(|| capitalize(&conflict)).unwrap_or(conflict)
                        })
                        .collect()
                )
            )
        }
    }
}

impl std::error::Error for ConflictError {}

/// A [`SharedState`] instance to use for universal resolution.
#[derive(Default, Clone)]
pub(crate) struct UniversalState(SharedState);

impl std::ops::Deref for UniversalState {
    type Target = SharedState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl UniversalState {
    /// Fork the [`UniversalState`] to create a [`PlatformState`].
    pub(crate) fn fork(&self) -> PlatformState {
        PlatformState(self.0.fork())
    }
}

/// A [`SharedState`] instance to use for platform-specific resolution.
#[derive(Default, Clone)]
pub(crate) struct PlatformState(SharedState);

impl std::ops::Deref for PlatformState {
    type Target = SharedState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PlatformState {
    /// Fork the [`PlatformState`] to create a [`UniversalState`].
    pub(crate) fn fork(&self) -> UniversalState {
        UniversalState(self.0.fork())
    }

    /// Create a [`SharedState`] from the [`PlatformState`].
    pub(crate) fn into_inner(self) -> SharedState {
        self.0
    }
}

/// Compute the `Requires-Python` bound for the [`Workspace`].
///
/// For a [`Workspace`] with multiple packages, the `Requires-Python` bound is the union of the
/// `Requires-Python` bounds of all the packages.
#[allow(clippy::result_large_err)]
pub(crate) fn find_requires_python(
    workspace: &Workspace,
) -> Result<Option<RequiresPython>, ProjectError> {
    // If there are no `Requires-Python` specifiers in the workspace, return `None`.
    if workspace.requires_python().next().is_none() {
        return Ok(None);
    }
    match RequiresPython::intersection(
        workspace
            .requires_python()
            .map(|(.., specifiers)| specifiers),
    ) {
        Some(requires_python) => Ok(Some(requires_python)),
        None => Err(ProjectError::DisjointRequiresPython(
            workspace
                .requires_python()
                .map(|(name, specifiers)| (name.clone(), specifiers.clone()))
                .collect(),
        )),
    }
}

/// Returns an error if the [`Interpreter`] does not satisfy the [`Workspace`] `requires-python`.
///
/// If no [`Workspace`] is provided, the `requires-python` will be validated against the originating
/// source (e.g., a `.python-version` file or a `--python` command-line argument).
#[allow(clippy::result_large_err)]
pub(crate) fn validate_project_requires_python(
    interpreter: &Interpreter,
    workspace: Option<&Workspace>,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), ProjectError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }

    // If the Python version is compatible with one of the workspace _members_, raise
    // a dedicated error. For example, if the workspace root requires Python >=3.12, but
    // a library in the workspace is compatible with Python >=3.8, the user may attempt
    // to sync on Python 3.8. This will fail, but we should provide a more helpful error
    // message.
    for (name, member) in workspace.into_iter().flat_map(Workspace::packages) {
        let Some(project) = member.pyproject_toml().project.as_ref() else {
            continue;
        };
        let Some(specifiers) = project.requires_python.as_ref() else {
            continue;
        };
        if specifiers.contains(interpreter.python_version()) {
            return match source {
                PythonRequestSource::UserRequest => {
                    Err(ProjectError::RequestedMemberIncompatibility(
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
                PythonRequestSource::DotPythonVersion(file) => {
                    Err(ProjectError::DotPythonVersionMemberIncompatibility(
                        file.path().user_display().to_string(),
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
                PythonRequestSource::RequiresPython => {
                    Err(ProjectError::RequiresPythonMemberIncompatibility(
                        interpreter.python_version().clone(),
                        requires_python.clone(),
                        name.clone(),
                        specifiers.clone(),
                        member.root().clone(),
                    ))
                }
            };
        }
    }

    match source {
        PythonRequestSource::UserRequest => {
            Err(ProjectError::RequestedPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(ProjectError::DotPythonVersionProjectIncompatibility(
                file.path().user_display().to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(ProjectError::RequiresPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
    }
}

/// Returns an error if the [`Interpreter`] does not satisfy script or workspace `requires-python`.
#[allow(clippy::result_large_err)]
fn validate_script_requires_python(
    interpreter: &Interpreter,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
) -> Result<(), ProjectError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }
    match source {
        PythonRequestSource::UserRequest => {
            Err(ProjectError::RequestedPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(ProjectError::DotPythonVersionScriptIncompatibility(
                file.file_name().to_string(),
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
        PythonRequestSource::RequiresPython => {
            Err(ProjectError::RequiresPythonScriptIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
            ))
        }
    }
}

/// An interpreter suitable for a PEP 723 script.
#[derive(Debug, Clone)]
pub(crate) struct ScriptInterpreter(Interpreter);

impl ScriptInterpreter {
    /// Discover the interpreter to use for the current [`Pep723Item`].
    pub(crate) async fn discover(
        script: Pep723ItemRef<'_>,
        python_request: Option<PythonRequest>,
        python_preference: PythonPreference,
        python_downloads: PythonDownloads,
        connectivity: Connectivity,
        native_tls: bool,
        allow_insecure_host: &[TrustedHost],
        install_mirrors: &PythonInstallMirrors,
        no_config: bool,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        // For now, we assume that scripts are never evaluated in the context of a workspace.
        let workspace = None;

        let ScriptPython {
            source,
            python_request,
            requires_python,
        } = ScriptPython::from_request(python_request, workspace, script, no_config).await?;

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());

        let reporter = PythonDownloadReporter::single(printer);

        let interpreter = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
        )
        .await?
        .into_interpreter();

        if let Err(err) = match requires_python {
            Some((requires_python, RequiresPythonSource::Project)) => {
                validate_project_requires_python(&interpreter, workspace, &requires_python, &source)
            }
            Some((requires_python, RequiresPythonSource::Script)) => {
                validate_script_requires_python(&interpreter, &requires_python, &source)
            }
            None => Ok(()),
        } {
            warn_user!("{err}");
        }

        Ok(Self(interpreter))
    }

    /// Consume the [`PythonInstallation`] and return the [`Interpreter`].
    pub(crate) fn into_interpreter(self) -> Interpreter {
        self.0
    }
}

/// An interpreter suitable for the project.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ProjectInterpreter {
    /// An interpreter from outside the project, to create a new project virtual environment.
    Interpreter(Interpreter),
    /// An interpreter from an existing project virtual environment.
    Environment(PythonEnvironment),
}

impl ProjectInterpreter {
    /// Discover the interpreter to use in the current [`Workspace`].
    pub(crate) async fn discover(
        workspace: &Workspace,
        project_dir: &Path,
        python_request: Option<PythonRequest>,
        python_preference: PythonPreference,
        python_downloads: PythonDownloads,
        connectivity: Connectivity,
        native_tls: bool,
        allow_insecure_host: &[TrustedHost],
        install_mirrors: &PythonInstallMirrors,
        no_config: bool,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, ProjectError> {
        // Resolve the Python request and requirement for the workspace.
        let WorkspacePython {
            source,
            python_request,
            requires_python,
        } = WorkspacePython::from_request(python_request, Some(workspace), project_dir, no_config)
            .await?;

        // Read from the virtual environment first.
        let venv = workspace.venv();
        match PythonEnvironment::from_root(&venv, cache) {
            Ok(venv) => {
                if python_request.as_ref().map_or(true, |request| {
                    if request.satisfied(venv.interpreter(), cache) {
                        debug!(
                            "The virtual environment's Python version satisfies `{}`",
                            request.to_canonical_string()
                        );
                        true
                    } else {
                        debug!(
                            "The virtual environment's Python version does not satisfy `{}`",
                            request.to_canonical_string()
                        );
                        false
                    }
                }) {
                    if let Some(requires_python) = requires_python.as_ref() {
                        if requires_python.contains(venv.interpreter().python_version()) {
                            return Ok(Self::Environment(venv));
                        }
                        debug!(
                            "The virtual environment's Python version does not meet the project's Python requirement: `{requires_python}`"
                        );
                    } else {
                        return Ok(Self::Environment(venv));
                    }
                }
            }
            Err(uv_python::Error::MissingEnvironment(_)) => {}
            Err(uv_python::Error::InvalidEnvironment(inner)) => {
                // If there's an invalid environment with existing content, we error instead of
                // deleting it later on
                match inner.kind {
                    InvalidEnvironmentKind::NotDirectory => {
                        return Err(ProjectError::InvalidProjectEnvironmentDir(
                            venv,
                            inner.kind.to_string(),
                        ))
                    }
                    InvalidEnvironmentKind::MissingExecutable(_) => {
                        if fs_err::read_dir(&venv).is_ok_and(|mut dir| dir.next().is_some()) {
                            return Err(ProjectError::InvalidProjectEnvironmentDir(
                                venv,
                                "it is not a valid Python environment (no Python executable was found)"
                                    .to_string(),
                            ));
                        }
                    }
                    // If the environment is an empty directory, it's fine to use
                    InvalidEnvironmentKind::Empty => {}
                };
            }
            Err(uv_python::Error::Query(uv_python::InterpreterError::NotFound(path))) => {
                if path.is_symlink() {
                    let target_path = fs_err::read_link(&path)?;
                    warn_user!(
                        "Ignoring existing virtual environment linked to non-existent Python interpreter: {} -> {}",
                        path.user_display().cyan(),
                        target_path.user_display().cyan(),
                    );
                }
            }
            Err(err) => return Err(err.into()),
        };

        let client_builder = BaseClientBuilder::default()
            .connectivity(connectivity)
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());

        let reporter = PythonDownloadReporter::single(printer);

        // Locate the Python interpreter to use in the environment.
        let python = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
        )
        .await?;

        let managed = python.source().is_managed();
        let implementation = python.implementation();
        let interpreter = python.into_interpreter();

        if managed {
            writeln!(
                printer.stderr(),
                "Using {} {}",
                implementation.pretty(),
                interpreter.python_version().cyan()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "Using {} {} interpreter at: {}",
                implementation.pretty(),
                interpreter.python_version(),
                interpreter.sys_executable().user_display().cyan()
            )?;
        }

        if let Some(requires_python) = requires_python.as_ref() {
            validate_project_requires_python(
                &interpreter,
                Some(workspace),
                requires_python,
                &source,
            )?;
        }

        Ok(Self::Interpreter(interpreter))
    }

    /// Convert the [`ProjectInterpreter`] into an [`Interpreter`].
    pub(crate) fn into_interpreter(self) -> Interpreter {
        match self {
            ProjectInterpreter::Interpreter(interpreter) => interpreter,
            ProjectInterpreter::Environment(venv) => venv.into_interpreter(),
        }
    }
}

/// The source of a `Requires-Python` specifier.
#[derive(Debug, Clone)]
pub(crate) enum RequiresPythonSource {
    /// From the PEP 723 inline script metadata.
    Script,
    /// From a `pyproject.toml` in a workspace.
    Project,
}

#[derive(Debug, Clone)]
pub(crate) enum PythonRequestSource {
    /// The request was provided by the user.
    UserRequest,
    /// The request was inferred from a `.python-version` or `.python-versions` file.
    DotPythonVersion(PythonVersionFile),
    /// The request was inferred from a `pyproject.toml` file.
    RequiresPython,
}

impl std::fmt::Display for PythonRequestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PythonRequestSource::UserRequest => write!(f, "explicit request"),
            PythonRequestSource::DotPythonVersion(file) => {
                write!(f, "version file at `{}`", file.path().user_display())
            }
            PythonRequestSource::RequiresPython => write!(f, "`requires-python` metadata"),
        }
    }
}

/// The resolved Python request and requirement for a [`Workspace`].
#[derive(Debug, Clone)]
pub(crate) struct WorkspacePython {
    /// The source of the Python request.
    pub(crate) source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, and (3) any
    /// `Requires-Python` specifier in the `pyproject.toml`.
    pub(crate) python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the project, computed by taking the intersection of all
    /// `Requires-Python` specifiers in the workspace.
    pub(crate) requires_python: Option<RequiresPython>,
}

impl WorkspacePython {
    /// Determine the [`WorkspacePython`] for the current [`Workspace`].
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: Option<&Workspace>,
        project_dir: &Path,
        no_config: bool,
    ) -> Result<Self, ProjectError> {
        let requires_python = workspace.map(find_requires_python).transpose()?.flatten();

        let workspace_root = workspace.map(Workspace::install_path);

        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            let source = PythonRequestSource::UserRequest;
            let request = Some(request);
            (source, request)
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default()
                .with_stop_discovery_at(workspace_root.map(PathBuf::as_ref))
                .with_no_config(no_config),
        )
        .await?
        {
            // (2) Request from `.python-version`
            let source = PythonRequestSource::DotPythonVersion(file.clone());
            let request = file.into_version();
            (source, request)
        } else {
            // (3) `requires-python` in `pyproject.toml`
            let request = requires_python
                .as_ref()
                .map(RequiresPython::specifiers)
                .map(|specifiers| {
                    PythonRequest::Version(VersionRequest::Range(
                        specifiers.clone(),
                        PythonVariant::Default,
                    ))
                });
            let source = PythonRequestSource::RequiresPython;
            (source, request)
        };

        if let Some(python_request) = python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        };

        Ok(Self {
            source,
            python_request,
            requires_python,
        })
    }
}

/// The resolved Python request and requirement for a [`Pep723Script`]
#[derive(Debug, Clone)]
pub(crate) struct ScriptPython {
    /// The source of the Python request.
    pub(crate) source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, (3) any
    /// `Requires-Python` specifier in the script metadata, and (4) any `Requires-Python` specifier
    /// in the `pyproject.toml`.
    pub(crate) python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the script and its source.
    pub(crate) requires_python: Option<(RequiresPython, RequiresPythonSource)>,
}

impl ScriptPython {
    /// Determine the [`ScriptPython`] for the current [`Workspace`].
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: Option<&Workspace>,
        script: Pep723ItemRef<'_>,
        no_config: bool,
    ) -> Result<Self, ProjectError> {
        // First, discover a requirement from the workspace
        let WorkspacePython {
            mut source,
            mut python_request,
            requires_python,
        } = WorkspacePython::from_request(
            python_request,
            workspace,
            script.path().and_then(Path::parent).unwrap_or(&**CWD),
            no_config,
        )
        .await?;

        // If the script has a `requires-python` specifier, prefer that over one from the workspace.
        let requires_python =
            if let Some(requires_python_specifiers) = script.metadata().requires_python.as_ref() {
                if python_request.is_none() {
                    python_request = Some(PythonRequest::Version(VersionRequest::Range(
                        requires_python_specifiers.clone(),
                        PythonVariant::Default,
                    )));
                    source = PythonRequestSource::RequiresPython;
                }
                Some((
                    RequiresPython::from_specifiers(requires_python_specifiers),
                    RequiresPythonSource::Script,
                ))
            } else {
                requires_python.map(|requirement| (requirement, RequiresPythonSource::Project))
            };

        if let Some(python_request) = python_request.as_ref() {
            debug!("Using Python request {python_request} from {source}");
        };

        Ok(Self {
            source,
            python_request,
            requires_python,
        })
    }
}

/// Initialize a virtual environment for the current project.
pub(crate) async fn get_or_init_environment(
    workspace: &Workspace,
    python: Option<PythonRequest>,
    install_mirrors: &PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment, ProjectError> {
    match ProjectInterpreter::discover(
        workspace,
        workspace.install_path().as_ref(),
        python,
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        install_mirrors,
        no_config,
        cache,
        printer,
    )
    .await?
    {
        // If we found an existing, compatible environment, use it.
        ProjectInterpreter::Environment(environment) => Ok(environment),

        // Otherwise, create a virtual environment with the discovered interpreter.
        ProjectInterpreter::Interpreter(interpreter) => {
            let venv = workspace.venv();

            // Avoid removing things that are not virtual environments
            let should_remove = match (venv.try_exists(), venv.join("pyvenv.cfg").try_exists()) {
                // It's a virtual environment we can remove it
                (_, Ok(true)) => true,
                // It doesn't exist at all, we should use it without deleting it to avoid TOCTOU bugs
                (Ok(false), Ok(false)) => false,
                // If it's not a virtual environment, bail
                (Ok(true), Ok(false)) => {
                    // Unless it's empty, in which case we just ignore it
                    if venv.read_dir().is_ok_and(|mut dir| dir.next().is_none()) {
                        false
                    } else {
                        return Err(ProjectError::InvalidProjectEnvironmentDir(
                            venv,
                            "it is not a compatible environment but cannot be recreated because it is not a virtual environment".to_string(),
                        ));
                    }
                }
                // Similarly, if we can't _tell_ if it exists we should bail
                (_, Err(err)) | (Err(err), _) => {
                    return Err(ProjectError::InvalidProjectEnvironmentDir(
                        venv,
                        format!("it is not a compatible environment but cannot be recreated because uv cannot determine if it is a virtual environment: {err}"),
                    ));
                }
            };

            // Remove the existing virtual environment if it doesn't meet the requirements.
            if should_remove {
                match fs_err::remove_dir_all(&venv) {
                    Ok(()) => {
                        writeln!(
                            printer.stderr(),
                            "Removed virtual environment at: {}",
                            venv.user_display().cyan()
                        )?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }

            writeln!(
                printer.stderr(),
                "Creating virtual environment at: {}",
                venv.user_display().cyan()
            )?;

            // Determine a prompt for the environment, in order of preference:
            //
            // 1) The name of the project
            // 2) The name of the directory at the root of the workspace
            // 3) No prompt
            let prompt = workspace
                .pyproject_toml()
                .project
                .as_ref()
                .map(|p| p.name.to_string())
                .or_else(|| {
                    workspace
                        .install_path()
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                })
                .map(uv_virtualenv::Prompt::Static)
                .unwrap_or(uv_virtualenv::Prompt::None);

            Ok(uv_virtualenv::create_venv(
                &venv,
                interpreter,
                prompt,
                false,
                false,
                false,
                false,
            )?)
        }
    }
}

/// Resolve any [`UnresolvedRequirementSpecification`] into a fully-qualified [`Requirement`].
pub(crate) async fn resolve_names(
    requirements: Vec<UnresolvedRequirementSpecification>,
    interpreter: &Interpreter,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<Vec<Requirement>, uv_requirements::Error> {
    // Partition the requirements into named and unnamed requirements.
    let (mut requirements, unnamed): (Vec<_>, Vec<_>) =
        requirements
            .into_iter()
            .partition_map(|spec| match spec.requirement {
                UnresolvedRequirement::Named(requirement) => itertools::Either::Left(requirement),
                UnresolvedRequirement::Unnamed(requirement) => {
                    itertools::Either::Right(requirement)
                }
            });

    // Short-circuit if there are no unnamed requirements.
    if unnamed.is_empty() {
        return Ok(requirements);
    }

    // Extract the project settings.
    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution: _,
        prerelease: _,
        fork_strategy: _,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode: _,
        sources,
        upgrade: _,
        reinstall: _,
        build_options,
    } = settings;

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if *no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package)
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let hasher = HashStrategy::default();
    let flat_index = FlatIndex::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone(),
        *index_strategy,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        &build_hasher,
        *exclude_newer,
        LowerBound::Allow,
        *sources,
        concurrency,
        preview,
    );

    // Resolve the unnamed requirements.
    requirements.extend(
        NamedRequirementsResolver::new(
            &hasher,
            state.index(),
            DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
        )
        .with_reporter(Arc::new(ResolverReporter::from(printer)))
        .resolve(unnamed.into_iter())
        .await?,
    );

    Ok(requirements)
}

#[derive(Debug, Clone)]
pub(crate) struct EnvironmentSpecification<'lock> {
    /// The requirements to include in the environment.
    requirements: RequirementsSpecification,
    /// The lockfile from which to extract preferences, along with the install path.
    lock: Option<(&'lock Lock, &'lock Path)>,
}

impl From<RequirementsSpecification> for EnvironmentSpecification<'_> {
    fn from(requirements: RequirementsSpecification) -> Self {
        Self {
            requirements,
            lock: None,
        }
    }
}

impl<'lock> EnvironmentSpecification<'lock> {
    #[must_use]
    pub(crate) fn with_lock(self, lock: Option<(&'lock Lock, &'lock Path)>) -> Self {
        Self { lock, ..self }
    }
}

/// Run dependency resolution for an interpreter, returning the [`ResolverOutput`].
pub(crate) async fn resolve_environment(
    spec: EnvironmentSpecification<'_>,
    interpreter: &Interpreter,
    settings: ResolverSettingsRef<'_>,
    state: &PlatformState,
    logger: Box<dyn ResolveLogger>,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ResolverOutput, ProjectError> {
    warn_on_requirements_txt_setting(&spec.requirements, settings);

    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        fork_strategy,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        upgrade: _,
        build_options,
        sources,
    } = settings;

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        source_trees,
        ..
    } = spec.requirements;

    // Determine the tags, markers, and interpreter to use for resolution.
    let tags = interpreter.tags()?;
    let marker_env = interpreter.resolver_marker_environment();
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package)
    };

    let options = OptionsBuilder::new()
        .resolution_mode(resolution)
        .prerelease_mode(prerelease)
        .fork_strategy(fork_strategy)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build_options(build_options.clone())
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let extras = ExtrasSpecification::default();
    let groups = DevGroupsSpecification::default();
    let hasher = HashStrategy::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();

    // When resolving from an interpreter, we assume an empty environment, so reinstalls and
    // upgrades aren't relevant.
    let reinstall = Reinstall::default();
    let upgrade = Upgrade::default();

    // If an existing lockfile exists, build up a set of preferences.
    let LockedRequirements { preferences, git } = spec
        .lock
        .map(|(lock, install_path)| read_lock_requirements(lock, install_path, &upgrade))
        .transpose()?
        .unwrap_or_default();

    // Populate the Git resolver.
    for ResolvedRepositoryReference { reference, sha } in git {
        debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
        state.git().insert(reference, sha);
    }

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let resolve_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer,
        LowerBound::Allow,
        sources,
        concurrency,
        preview,
    );

    // Resolve the requirements.
    Ok(pip::operations::resolve(
        requirements,
        constraints,
        overrides,
        source_trees,
        project,
        BTreeSet::default(),
        &extras,
        &groups,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &reinstall,
        &upgrade,
        Some(tags),
        ResolverEnvironment::specific(marker_env),
        python_requirement,
        Conflicts::empty(),
        &client,
        &flat_index,
        state.index(),
        &resolve_dispatch,
        concurrency,
        options,
        logger,
        printer,
    )
    .await?)
}

/// Sync a [`PythonEnvironment`] with a set of resolved requirements.
pub(crate) async fn sync_environment(
    venv: PythonEnvironment,
    resolution: &Resolution,
    settings: InstallerSettingsRef<'_>,
    state: &PlatformState,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<PythonEnvironment, ProjectError> {
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
        sources,
    } = settings;

    let site_packages = SitePackages::from_environment(&venv)?;

    // Determine the markers tags to use for resolution.
    let interpreter = venv.interpreter();
    let tags = venv.interpreter().tags()?;

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(&venv, no_build_isolation_package)
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let dry_run = false;
    let hasher = HashStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        index_strategy,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer,
        LowerBound::Allow,
        sources,
        concurrency,
        preview,
    );

    // Sync the environment.
    pip::operations::install(
        resolution,
        site_packages,
        Modifications::Exact,
        reinstall,
        build_options,
        link_mode,
        compile_bytecode,
        index_locations,
        config_setting,
        &hasher,
        tags,
        &client,
        state.in_flight(),
        concurrency,
        &build_dispatch,
        cache,
        &venv,
        logger,
        installer_metadata,
        dry_run,
        printer,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(venv)
}

/// The result of updating a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
#[derive(Debug)]
pub(crate) struct EnvironmentUpdate {
    /// The updated [`PythonEnvironment`].
    pub(crate) environment: PythonEnvironment,
    /// The [`Changelog`] of changes made to the environment.
    pub(crate) changelog: Changelog,
}

impl EnvironmentUpdate {
    /// Convert the [`EnvironmentUpdate`] into a [`PythonEnvironment`].
    pub(crate) fn into_environment(self) -> PythonEnvironment {
        self.environment
    }
}

/// Update a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
pub(crate) async fn update_environment(
    venv: PythonEnvironment,
    spec: RequirementsSpecification,
    settings: &ResolverInstallerSettings,
    state: &SharedState,
    resolve: Box<dyn ResolveLogger>,
    install: Box<dyn InstallLogger>,
    installer_metadata: bool,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<EnvironmentUpdate, ProjectError> {
    warn_on_requirements_txt_setting(&spec, settings.as_ref().into());

    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        fork_strategy,
        dependency_metadata,
        config_setting,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode,
        sources,
        upgrade,
        reinstall,
        build_options,
    } = settings;

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        source_trees,
        ..
    } = spec;

    // Determine markers to use for resolution.
    let interpreter = venv.interpreter();
    let marker_env = venv.interpreter().resolver_marker_environment();

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_environment(&venv)?;
    if source_trees.is_empty() && reinstall.is_none() && upgrade.is_none() && overrides.is_empty() {
        match site_packages.satisfies(&requirements, &constraints, &marker_env)? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                debug!(
                    "All requirements satisfied: {}",
                    recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                        .join(" | ")
                );
                return Ok(EnvironmentUpdate {
                    environment: venv,
                    changelog: Changelog::default(),
                });
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Determine whether to enable build isolation.
    let build_isolation = if *no_build_isolation {
        BuildIsolation::Shared(&venv)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(&venv, no_build_isolation_package)
    };

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease_mode(*prerelease)
        .fork_strategy(*fork_strategy)
        .exclude_newer(*exclude_newer)
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let dry_run = false;
    let extras = ExtrasSpecification::default();
    let groups = DevGroupsSpecification::default();
    let hasher = HashStrategy::default();
    let preferences = Vec::default();

    // Determine the tags to use for resolution.
    let tags = venv.interpreter().tags()?;
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone(),
        *index_strategy,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        &build_hasher,
        *exclude_newer,
        LowerBound::Allow,
        *sources,
        concurrency,
        preview,
    );

    // Resolve the requirements.
    let resolution = match pip::operations::resolve(
        requirements,
        constraints,
        overrides,
        source_trees,
        project,
        BTreeSet::default(),
        &extras,
        &groups,
        preferences,
        site_packages.clone(),
        &hasher,
        reinstall,
        upgrade,
        Some(tags),
        ResolverEnvironment::specific(marker_env.clone()),
        python_requirement,
        Conflicts::empty(),
        &client,
        &flat_index,
        state.index(),
        &build_dispatch,
        concurrency,
        options,
        resolve,
        printer,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(err) => return Err(err.into()),
    };

    // Sync the environment.
    let changelog = pip::operations::install(
        &resolution,
        site_packages,
        Modifications::Exact,
        reinstall,
        build_options,
        *link_mode,
        *compile_bytecode,
        index_locations,
        config_setting,
        &hasher,
        tags,
        &client,
        state.in_flight(),
        concurrency,
        &build_dispatch,
        cache,
        &venv,
        install,
        installer_metadata,
        dry_run,
        printer,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(EnvironmentUpdate {
        environment: venv,
        changelog,
    })
}

/// Determine the [`RequiresPython`] requirement for a new PEP 723 script.
pub(crate) async fn init_script_python_requirement(
    python: Option<&str>,
    install_mirrors: &PythonInstallMirrors,
    directory: &Path,
    no_pin_python: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    no_config: bool,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    reporter: &PythonDownloadReporter,
) -> anyhow::Result<RequiresPython> {
    let python_request = if let Some(request) = python {
        // (1) Explicit request from user
        PythonRequest::parse(request)
    } else if let (false, Some(request)) = (
        no_pin_python,
        PythonVersionFile::discover(
            directory,
            &VersionFileDiscoveryOptions::default().with_no_config(no_config),
        )
        .await?
        .and_then(PythonVersionFile::into_version),
    ) {
        // (2) Request from `.python-version`
        request
    } else {
        // (3) Assume any Python version
        PythonRequest::Any
    };

    let interpreter = PythonInstallation::find_or_download(
        Some(&python_request),
        EnvironmentPreference::Any,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
    )
    .await?
    .into_interpreter();

    Ok(RequiresPython::greater_than_equal_version(
        &interpreter.python_minor_version(),
    ))
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum DependencyGroupsTarget<'env> {
    /// The dependency groups can be defined in any workspace member.
    Workspace(&'env Workspace),
    /// The dependency groups must be defined in the target project.
    Project(&'env ProjectWorkspace),
    /// The dependency groups must be defined in the target script.
    Script,
}

impl DependencyGroupsTarget<'_> {
    /// Validate the dependency groups requested by the [`DevGroupsSpecification`].
    #[allow(clippy::result_large_err)]
    pub(crate) fn validate(self, dev: &DevGroupsSpecification) -> Result<(), ProjectError> {
        for group in dev
            .groups()
            .into_iter()
            .flat_map(GroupsSpecification::names)
        {
            match self {
                Self::Workspace(workspace) => {
                    // The group must be defined in the workspace.
                    if !workspace.groups().contains(group) {
                        return Err(ProjectError::MissingGroupWorkspace(group.clone()));
                    }
                }
                Self::Project(project) => {
                    // The group must be defined in the target project.
                    if !project
                        .current_project()
                        .pyproject_toml()
                        .dependency_groups
                        .as_ref()
                        .is_some_and(|groups| groups.contains_key(group))
                    {
                        return Err(ProjectError::MissingGroupProject(group.clone()));
                    }
                }
                Self::Script => {
                    return Err(ProjectError::MissingGroupScript(group.clone()));
                }
            }
        }
        Ok(())
    }
}

/// Returns the default dependency groups from the [`PyProjectToml`].
#[allow(clippy::result_large_err)]
pub(crate) fn default_dependency_groups(
    pyproject_toml: &PyProjectToml,
) -> Result<Vec<GroupName>, ProjectError> {
    if let Some(defaults) = pyproject_toml
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref().and_then(|uv| uv.default_groups.as_ref()))
    {
        for group in defaults {
            if !pyproject_toml
                .dependency_groups
                .as_ref()
                .is_some_and(|groups| groups.contains_key(group))
            {
                return Err(ProjectError::MissingDefaultGroup(group.clone()));
            }
        }
        Ok(defaults.clone())
    } else {
        Ok(vec![DEV_DEPENDENCIES.clone()])
    }
}

/// Validate that we aren't trying to install extras or groups that
/// are declared as conflicting.
#[allow(clippy::result_large_err)]
pub(crate) fn detect_conflicts(
    lock: &Lock,
    extras: &ExtrasSpecification,
    dev: &DevGroupsManifest,
) -> Result<(), ProjectError> {
    // Note that we need to collect all extras and groups that match in
    // a particular set, since extras can be declared as conflicting with
    // groups. So if extra `x` and group `g` are declared as conflicting,
    // then enabling both of those should result in an error.
    let conflicts = lock.conflicts();
    for set in conflicts.iter() {
        let mut conflicts: Vec<ConflictPackage> = vec![];
        for item in set.iter() {
            if item
                .extra()
                .map(|extra| extras.contains(extra))
                .unwrap_or(false)
            {
                conflicts.push(item.conflict().clone());
            }
            if item
                .group()
                .map(|group| dev.contains(group))
                .unwrap_or(false)
            {
                conflicts.push(item.conflict().clone());
            }
        }
        if conflicts.len() >= 2 {
            return Err(ProjectError::Conflict(ConflictError {
                set: set.clone(),
                conflicts,
                dev: dev.clone(),
            }));
        }
    }
    Ok(())
}

/// Warn if the user provides (e.g.) an `--index-url` in a requirements file.
fn warn_on_requirements_txt_setting(
    spec: &RequirementsSpecification,
    settings: ResolverSettingsRef<'_>,
) {
    let RequirementsSpecification {
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        no_binary,
        no_build,
        ..
    } = spec;

    if settings.index_locations.no_index() {
        // Nothing to do, we're ignoring the URLs anyway.
    } else if *no_index {
        warn_user_once!("Ignoring `--no-index` from requirements file. Instead, use the `--no-index` command-line argument, or set `no-index` in a `uv.toml` or `pyproject.toml` file.");
    } else {
        if let Some(index_url) = index_url {
            if settings.index_locations.default_index().map(Index::url) != Some(index_url) {
                warn_user_once!(
                    "Ignoring `--index-url` from requirements file: `{index_url}`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file."
                );
            }
        }
        for extra_index_url in extra_index_urls {
            if !settings
                .index_locations
                .implicit_indexes()
                .any(|index| index.url() == extra_index_url)
            {
                warn_user_once!(
                    "Ignoring `--extra-index-url` from requirements file: `{extra_index_url}`. Instead, use the `--extra-index-url` command-line argument, or set `extra-index-url` in a `uv.toml` or `pyproject.toml` file.`"

                );
            }
        }
        for find_link in find_links {
            if !settings
                .index_locations
                .flat_indexes()
                .any(|index| index.url() == find_link)
            {
                warn_user_once!(
                    "Ignoring `--find-links` from requirements file: `{find_link}`. Instead, use the `--find-links` command-line argument, or set `find-links` in a `uv.toml` or `pyproject.toml` file.`"
                );
            }
        }
    }

    if !no_binary.is_none() && settings.build_options.no_binary() != no_binary {
        warn_user_once!("Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-binary` command-line argument, or set `no-binary` in a `uv.toml` or `pyproject.toml` file.");
    }

    if !no_build.is_none() && settings.build_options.no_build() != no_build {
        warn_user_once!("Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-build` command-line argument, or set `no-build` in a `uv.toml` or `pyproject.toml` file.");
    }
}
