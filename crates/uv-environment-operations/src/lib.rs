//! Shared project, script, and tool environment workflows.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};

use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::{debug, warn};
use uv_cache::{Cache, CacheBucket};
use uv_cache_key::{cache_digest, cache_name};
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    ActiveEnvironment, Concurrency, Constraints, DependencyGroupsWithDefaults, DryRun,
    ExtrasSpecification, HashCheckingMode, Modifications, Reinstall, TargetTriple, Upgrade,
};
use uv_dispatch::{BuildDispatch, PlatformState, SharedState};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ExtraBuildRequires, HashCollection, Index, RequiresPython, Resolution,
};
use uv_fs::{LockedFile, LockedFileError, LockedFileMode, Simplified, verbatim_path};
use uv_git::ResolvedRepositoryReference;
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_lock::{Installable, Lock};
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{ConflictItem, ConflictKind, ConflictSet, Conflicts};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::PythonInstallation;
use uv_python_interpreter::{BrokenLink, Interpreter, InvalidEnvironmentKind, PythonEnvironment};
use uv_python_managed::{PythonMinorVersionLink, UpgradePolicy};
use uv_python_types::{
    EnvironmentPreference, LenientImplementationName, PythonArchitecture, PythonDownloads,
    PythonPreference, PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_resolver::{
    DependencyMode, FlatIndex, OptionsBuilder, Preference, PythonRequirement, ResolverEnvironment,
    ResolverOutput,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_torch::TorchStrategy;
use uv_types::{BuildIsolation, HashStrategy, SourceTreeEditablePolicy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{ProjectEnvironmentSelection, Workspace, WorkspaceCache};

use crate::install_target::{InstallTarget, PackageSelection};
use uv_command_support::{Printer, conjunction};
use uv_install_operations::Changelog;
use uv_install_operations::loggers::InstallLogger;
use uv_python_discovery::CompatibleProjectPython;
use uv_python_discovery::EnvironmentIncompatibilityError;
use uv_python_discovery::EnvironmentKind;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::PythonDownloadReporter;
use uv_python_discovery::ScriptInterpreter;
use uv_python_discovery::check_environment_compatibility;
use uv_resolve_operations::locked_requirements::{LockedRequirements, read_lock_requirements};
use uv_resolve_operations::loggers::ResolveLogger;
use uv_settings::{InstallerSettingsRef, ResolverInstallerSettings, ResolverSettings};

pub mod environment;
mod error;
pub use error::EnvironmentError;
pub mod install_target;
pub mod malware;
mod sync;
pub use sync::{store_credentials_from_target, sync_from_lock};

#[derive(Debug)]
pub struct ConflictError {
    /// The set from which the conflict was derived.
    set: ConflictSet,
    /// The items from the set that were enabled, and thus create the conflict.
    conflicts: Vec<ConflictItem>,
    /// Enabled dependency groups with defaults applied.
    groups: DependencyGroupsWithDefaults,
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format the set itself.
        let set = self
            .set
            .iter()
            .map(|item| match item.kind() {
                ConflictKind::Project => format!("{}", item.package()),
                ConflictKind::Extra(extra) => format!("`{}[{}]`", item.package(), extra),
                ConflictKind::Group(group) => format!("`{}:{}`", item.package(), group),
            })
            .join(", ");

        // If all the conflicts are of the same kind, show a more succinct error.
        if self
            .conflicts
            .iter()
            .all(|conflict| matches!(conflict.kind(), ConflictKind::Extra(..)))
        {
            write!(
                f,
                "Extras {} are incompatible with the declared conflicts: {{{set}}}",
                conjunction(
                    self.conflicts
                        .iter()
                        .map(|conflict| match conflict.kind() {
                            ConflictKind::Extra(extra) => format!("`{extra}`"),
                            ConflictKind::Group(..) | ConflictKind::Project => unreachable!(),
                        })
                        .collect()
                )
            )
        } else if self
            .conflicts
            .iter()
            .all(|conflict| matches!(conflict.kind(), ConflictKind::Group(..)))
        {
            write!(
                f,
                "Groups {} are incompatible with the conflicts: {{{set}}}",
                conjunction(
                    self.conflicts
                        .iter()
                        .map(|conflict| match conflict.kind() {
                            ConflictKind::Group(group)
                                if self.groups.contains_because_default(group) =>
                                format!("`{group}` (enabled by default)"),
                            ConflictKind::Group(group) => format!("`{group}`"),
                            ConflictKind::Extra(..) | ConflictKind::Project => unreachable!(),
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
                            let conflict = match conflict.kind() {
                                ConflictKind::Project => {
                                    format!("package `{}`", conflict.package())
                                }
                                ConflictKind::Extra(extra) => format!("extra `{extra}`"),
                                ConflictKind::Group(group)
                                    if self.groups.contains_because_default(group) =>
                                {
                                    format!("group `{group}` (enabled by default)")
                                }
                                ConflictKind::Group(group) => format!("group `{group}`"),
                            };
                            if i == 0 {
                                capitalize(&conflict)
                            } else {
                                conflict
                            }
                        })
                        .collect()
                )
            )
        }
    }
}

impl std::error::Error for ConflictError {}

/// Capitalize the first letter of a string.
fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        None => String::new(),
        Some(character) => character.to_uppercase().collect::<String>() + characters.as_str(),
    }
}

/// The policy for discovering and initializing a project environment.
#[derive(Debug, Clone, Copy)]
pub enum ProjectEnvironmentPolicy {
    /// An environment is unnecessary; ignore it if invalid or incompatible.
    Optional,

    /// Require a valid environment compatible with the Python requirements.
    ///
    /// Replace an existing environment if it is incompatible.
    Compatible,

    /// Preserve a valid existing environment, even if incompatible.
    ///
    /// Create an environment if none exists, or replace an invalid virtual environment.
    Preserve,
}

/// Discover an existing project environment at `root` without validating its compatibility.
fn existing_project_environment(
    root: &Path,
    centralized: bool,
    policy: ProjectEnvironmentPolicy,
    cache: &Cache,
) -> Result<Option<PythonEnvironment>, EnvironmentError> {
    let environment = match PythonEnvironment::from_root(root, cache) {
        Ok(environment) => environment,
        Err(uv_python_interpreter::PythonEnvironmentError::MissingEnvironment(_)) => {
            return Ok(None);
        }
        Err(uv_python_interpreter::PythonEnvironmentError::InvalidEnvironment(inner)) => {
            match inner.kind {
                InvalidEnvironmentKind::NotDirectory => {
                    return Err(EnvironmentError::InvalidProjectEnvironmentDir(
                        root.to_path_buf(),
                        inner.kind.to_string(),
                    ));
                }
                InvalidEnvironmentKind::MissingExecutable(_) => {
                    if !matches!(policy, ProjectEnvironmentPolicy::Optional)
                        && !centralized
                        && fs_err::read_dir(root).is_ok_and(|mut dir| dir.next().is_some())
                    {
                        if !root.join("pyvenv.cfg").try_exists().unwrap_or_default() {
                            return Err(EnvironmentError::InvalidProjectEnvironmentDir(
                                root.to_path_buf(),
                                "it is not a valid Python environment (no Python executable was found)"
                                    .to_string(),
                            ));
                        }
                    }
                }
                InvalidEnvironmentKind::Empty => {}
            }
            return Ok(None);
        }
        Err(uv_python_interpreter::PythonEnvironmentError::Query(
            uv_python_interpreter::InterpreterError::NotFound(_),
        )) => {
            return Ok(None);
        }
        Err(uv_python_interpreter::PythonEnvironmentError::Query(
            uv_python_interpreter::InterpreterError::BrokenLink(BrokenLink {
                path,
                unix,
                venv: _,
            }),
        )) => {
            if unix {
                let target_path = fs_err::read_link(&path)?;
                warn_user!(
                    "Ignoring existing virtual environment linked to non-existent Python interpreter: `{}` -> `{}`",
                    path.user_display().cyan(),
                    target_path.user_display().cyan(),
                );
            } else {
                warn_user!(
                    "Ignoring existing virtual environment linked to non-existent Python interpreter: {}",
                    path.user_display().cyan(),
                );
            }
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    Ok(Some(environment))
}

/// Discover a compatible project environment at `root`.
fn discover_project_environment(
    root: &Path,
    python_request: Option<&PythonRequest>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    requires_python: Option<&RequiresPython>,
    policy: ProjectEnvironmentPolicy,
    centralized: bool,
    cache: &Cache,
) -> Result<Option<PythonEnvironment>, EnvironmentError> {
    let Some(environment) = existing_project_environment(root, centralized, policy, cache)? else {
        return Ok(None);
    };

    let compatibility = check_environment_compatibility(
        &environment,
        EnvironmentKind::Project,
        python_request,
        python_preference,
        python_arch,
        requires_python,
        cache,
    );

    // Conflicting versions for the same base interpreter indicate its cached metadata may be
    // corrupted. Clear the entry before interpreter discovery can select stale metadata.
    if matches!(
        &compatibility,
        Err(EnvironmentIncompatibilityError::PyenvVersionConflict(..))
    ) && let Ok(base_executable) = environment.interpreter().to_base_python()
        && let Ok(base_interpreter) = Interpreter::query(&base_executable, cache)
        && environment.uses(&base_interpreter)
        && environment.interpreter().python_version() != base_interpreter.python_version()
    {
        debug!(
            "Clearing cached interpreter info for `{}` after finding conflicting Python versions ({} and {})",
            base_executable.user_display(),
            base_interpreter.python_version(),
            environment.interpreter().python_version(),
        );
        Interpreter::clear_cache(&base_executable, cache)?;
    }

    match compatibility {
        Ok(()) => Ok(Some(environment)),
        Err(err) if matches!(policy, ProjectEnvironmentPolicy::Preserve) => {
            if centralized {
                let root = environment.root();
                warn_user!(
                    "Using incompatible environment (`{}`) due to `--no-sync` ({err})",
                    root.file_name()
                        .unwrap_or(root.as_os_str())
                        .to_string_lossy()
                        .cyan(),
                );
            } else {
                warn_user!(
                    "Using incompatible environment (`{}`) due to `--no-sync` ({err})",
                    environment.root().user_display().cyan(),
                );
            }
            Ok(Some(environment))
        }
        Err(err) => {
            debug!("{err}");
            Ok(None)
        }
    }
}

/// Return whether to use centralized project environments for this invocation.
pub fn centralized_environments_enabled(
    selection: &ProjectEnvironmentSelection,
    cache: &Cache,
) -> bool {
    if !selection.is_default() || !uv_preview::is_enabled(PreviewFeature::CentralizedProjectEnvs) {
        return false;
    }
    if cache.is_temporary() {
        warn_user_once!(
            "The `centralized-project-envs` feature has no effect when `--no-cache` is enabled"
        );
        return false;
    }
    true
}

/// Return whether `path` is lexically within `base`.
fn is_path_lexically_within(path: &Path, base: &Path) -> bool {
    // Normally only longer paths must be in the verbatim namespace, normalise both so the
    // comparison works correctly regardless.
    verbatim_path(path).starts_with(verbatim_path(base).as_ref())
}

/// Return whether `path` looks like a path we wrote and references our environment cache.
///
/// This isn't fully robust, and cannot be, as the path may not exist.
fn is_centralized_environment_path(path: &Path, cache: &Cache) -> bool {
    let Ok(environments) = std::path::absolute(cache.bucket(CacheBucket::Environments)) else {
        return false;
    };
    if is_path_lexically_within(path, &environments) {
        return true;
    }

    // Resolve existing relative or indirect paths; only the lexical check can handle dangling
    // paths.
    fs_err::canonicalize(path).is_ok_and(|path| {
        fs_err::canonicalize(&environments)
            .is_ok_and(|environments| is_path_lexically_within(&path, &environments))
    })
}

/// Return whether `path` appears to link into the current cache's environment bucket.
fn is_centralized_environment_link(path: &Path, cache: &Cache) -> bool {
    let Ok(target) = fs_err::read_link(path) else {
        return false;
    };
    is_centralized_environment_path(&target, cache) || is_centralized_environment_path(path, cache)
}

/// Read an environment path from a file.
fn read_environment_path_file(path: &Path) -> io::Result<PathBuf> {
    let target = PathBuf::from(fs_err::read_to_string(path)?);
    Ok(if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or(Path::new("")).join(target)
    })
}

/// Return whether `path` refers to an environment in the current cache's environment bucket.
pub fn is_centralized_environment_reference(path: &Path, cache: &Cache) -> bool {
    is_centralized_environment_link(path, cache)
        || read_environment_path_file(path)
            .is_ok_and(|target| is_centralized_environment_path(&target, cache))
}

/// Return the centralized environment path for a project and interpreter.
pub fn centralized_environment_root(
    target: ProjectEnvironmentTarget<'_>,
    interpreter: &Interpreter,
    upgrade_policy: UpgradePolicy,
    cache: &Cache,
) -> PathBuf {
    let install_path = target.install_path();
    let workspace_path =
        fs_err::canonicalize(install_path).unwrap_or_else(|_| install_path.to_path_buf());
    let interpreter_key = interpreter.key();
    // Use the workspace path to isolate projects and the interpreter key to maximize intra-project
    // environment re-use while avoiding clashes with incompatible environments. Ignoring the patch
    // version allows upgradeable managed environments to be re-used after an upgrade.
    let (digest, python_version) =
        if let Some(link) = PythonMinorVersionLink::from_interpreter(interpreter, upgrade_policy) {
            (
                cache_digest(&(&workspace_path, link.key())),
                interpreter.python_minor_version(),
            )
        } else {
            (
                cache_digest(&(&workspace_path, &interpreter_key)),
                interpreter.python_version().clone(),
            )
        };
    let name = target
        .project_name()
        .and_then(|name| cache_name(name.as_ref(), Some(100)))
        .or_else(|| {
            workspace_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| cache_name(name, Some(100)))
        });
    let implementation = interpreter_key.implementation();
    let implementation = match implementation.as_ref() {
        LenientImplementationName::Known(implementation) => implementation
            .short_name()
            .unwrap_or_else(|| implementation.long_name()),
        LenientImplementationName::Unknown(implementation) => implementation,
    };
    let entry = name.map_or_else(
        // A virtual workspace can be nameless if its directory has no cache-safe characters.
        || format!("{implementation}{python_version}-{digest}"),
        |name| format!("{name}-{implementation}{python_version}-{digest}"),
    );
    cache
        .shard(CacheBucket::Environments, entry)
        .into_path_buf()
}

/// How to report failures updating `.venv`.
#[derive(Clone, Copy)]
pub enum LinkErrorReporting {
    /// Report failures to the user.
    User,
    /// Log failures at warning level.
    Log,
}

/// Point the project's `.venv` to the centralized environment, returning whether the link was
/// successfully updated.
pub fn update_project_environment_link(
    environment: &PythonEnvironment,
    target: ProjectEnvironmentTarget<'_>,
    link_error_reporting: LinkErrorReporting,
) -> bool {
    let link = target.install_path().join(".venv");
    let report_error = |message: std::fmt::Arguments<'_>| match link_error_reporting {
        LinkErrorReporting::User => warn_user_once!("{message}"),
        LinkErrorReporting::Log => warn!("{message}"),
    };

    if fs_err::symlink_metadata(&link).is_ok_and(|metadata| metadata.is_dir()) {
        if uv_fs::is_virtualenv_base(&link) {
            if let Err(err) = uv_fs::remove_virtualenv(&link) {
                report_error(format_args!(
                    "Failed to remove existing local virtual environment: {err}"
                ));
                return false;
            }
        } else {
            // On Windows, copying a junction can produce an empty directory.
            #[cfg(windows)]
            if let Err(err) = fs_err::remove_dir(&link) {
                report_error(format_args!(
                    "Failed to create link to project environment: {err}"
                ));
                return false;
            }
        }
    }

    // On Windows replace_symlink won't replace a file, but we want to try to upgrade to a junction
    // if possible.
    if cfg!(windows) {
        let _ = fs_err::remove_file(&link);
    }

    let Err(link_error) = uv_fs::replace_symlink(environment.root(), &link) else {
        return true;
    };
    warn!("Failed to create link to project environment: {link_error}");

    let Some(target) = environment.root().to_str() else {
        report_error(format_args!(
            "Failed to write the environment path to `{}`: the path is not valid UTF-8",
            link.simplified_display()
        ));
        return false;
    };

    if let Err(err) = uv_fs::write_atomic_sync(&link, target.as_bytes()) {
        report_error(format_args!("Failed to write the environment path: {err}"));
        return false;
    }

    report_error(format_args!(
        "Failed to create link to project environment; wrote the environment path to `{}` instead",
        link.simplified_display()
    ));
    false
}

/// The project information needed to discover and create an environment.
#[derive(Clone, Copy)]
pub enum ProjectEnvironmentTarget<'a> {
    Workspace(&'a Workspace),
    Lockfile { root: &'a Path, lock: &'a Lock },
}

impl<'a> From<&'a Workspace> for ProjectEnvironmentTarget<'a> {
    fn from(workspace: &'a Workspace) -> Self {
        Self::Workspace(workspace)
    }
}

impl<'a> ProjectEnvironmentTarget<'a> {
    /// Return the directory where the environment is installed.
    fn install_path(self) -> &'a Path {
        match self {
            Self::Workspace(workspace) => workspace.install_path(),
            Self::Lockfile { root, .. } => root,
        }
    }

    /// Return the project associated with this environment, if any.
    fn project_name(self) -> Option<&'a PackageName> {
        match self {
            Self::Workspace(workspace) => workspace
                .pyproject_toml()
                .project
                .as_ref()
                .map(|project| &project.name),
            Self::Lockfile { lock, .. } => lock.root().map(uv_lock::Package::name),
        }
    }

    /// Return the discovered workspace, if this target has one.
    fn workspace(self) -> Option<&'a Workspace> {
        match self {
            Self::Workspace(workspace) => Some(workspace),
            Self::Lockfile { .. } => None,
        }
    }
}

/// An interpreter suitable for the project.
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
pub enum ProjectInterpreter {
    /// A compatible interpreter from outside the project, to create a new virtual environment.
    Interpreter(CompatibleProjectPython),
    /// An existing project environment, which may be incompatible under `--no-sync`.
    Environment(PythonEnvironment),
}

impl ProjectInterpreter {
    /// Discover an existing project environment without selecting or downloading an interpreter.
    pub fn discover_existing(
        install_path: &Path,
        active: ActiveEnvironment,
        cache: &Cache,
    ) -> Result<Option<PythonEnvironment>, EnvironmentError> {
        let selection = ProjectEnvironmentSelection::from_install_path(install_path, active);
        let root = selection
            .explicit_path()
            .map_or_else(|| install_path.join(".venv"), Path::to_path_buf);
        let root = read_environment_path_file(&root).unwrap_or(root);
        let centralized = centralized_environments_enabled(&selection, cache)
            || is_centralized_environment_reference(&root, cache);
        let root = if centralized {
            fs_err::canonicalize(&root).unwrap_or(root)
        } else {
            root
        };

        existing_project_environment(
            &root,
            centralized,
            ProjectEnvironmentPolicy::Optional,
            cache,
        )
    }

    /// Discover an interpreter for a workspace or frozen lockfile.
    pub async fn discover(
        target: ProjectEnvironmentTarget<'_>,
        project_python: ProjectPythonRequest,
        client_builder: &BaseClientBuilder<'_>,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
        policy: ProjectEnvironmentPolicy,
        active: ActiveEnvironment,
        cache: &Cache,
        printer: Printer,
    ) -> Result<Self, EnvironmentError> {
        let python_request = project_python.python_request.as_ref();
        let requires_python = project_python.requires_python();
        let upgrade_policy =
            UpgradePolicy::from_request(python_request.unwrap_or(&PythonRequest::Default));

        let environment_selection =
            ProjectEnvironmentSelection::from_install_path(target.install_path(), active);
        let centralized = centralized_environments_enabled(&environment_selection, cache);

        // Prefer `.venv`'s interpreter to keep its compatible cached environment selected; derive
        // the cache root instead of trusting the link target.
        if centralized {
            let project_environment_path = target.install_path().join(".venv");
            if let Ok(candidate) = PythonEnvironment::from_root(
                read_environment_path_file(&project_environment_path)
                    .ok()
                    .as_deref()
                    .unwrap_or(&project_environment_path),
                cache,
            ) {
                let root = centralized_environment_root(
                    target,
                    candidate.interpreter(),
                    upgrade_policy,
                    cache,
                );
                if let Some(environment) = discover_project_environment(
                    &root,
                    python_request,
                    python_preference,
                    python_arch,
                    requires_python,
                    policy,
                    centralized,
                    cache,
                )? {
                    return Ok(Self::Environment(environment));
                }
            }
        } else {
            let project_environment_path = environment_selection
                .explicit_path()
                .map_or_else(|| target.install_path().join(".venv"), Path::to_path_buf);
            // TODO(tk): Revisit after PEP 832.
            // A centralized path file is not a local environment; let initialization replace it.
            if !(environment_selection.is_default()
                && read_environment_path_file(&project_environment_path)
                    .is_ok_and(|target| is_centralized_environment_path(&target, cache)))
                && let Some(environment) = discover_project_environment(
                    &project_environment_path,
                    python_request,
                    python_preference,
                    python_arch,
                    requires_python,
                    policy,
                    centralized,
                    cache,
                )?
            {
                return Ok(Self::Environment(environment));
            }
        }

        let reporter = PythonDownloadReporter::single(printer);

        // Locate the Python interpreter to use in the environment.
        let python = PythonInstallation::find_or_download(
            python_request,
            EnvironmentPreference::OnlySystem,
            python_preference,
            python_arch,
            python_downloads,
            client_builder,
            cache,
            Some(&reporter),
            install_mirrors.mirrors(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await?;

        if centralized {
            let root =
                centralized_environment_root(target, python.interpreter(), upgrade_policy, cache);
            if let Some(environment) = discover_project_environment(
                &root,
                python_request,
                python_preference,
                python_arch,
                requires_python,
                policy,
                centralized,
                cache,
            )? {
                return Ok(Self::Environment(environment));
            }
        }

        let managed = python.source().is_managed();
        let implementation = python.implementation();
        let interpreter = python.into_interpreter();

        if managed {
            writeln!(
                printer.stderr(),
                "Using {} {}{}",
                implementation.pretty(),
                interpreter.python_version().cyan(),
                interpreter.variant().display_suffix().cyan(),
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "Using {} {}{} interpreter at: {}",
                implementation.pretty(),
                interpreter.python_version(),
                interpreter.variant().display_suffix(),
                interpreter.sys_executable().user_display().cyan()
            )?;
        }

        Ok(Self::Interpreter(project_python.validate(interpreter)?))
    }

    /// Convert the [`ProjectInterpreter`] into an [`Interpreter`].
    pub fn into_interpreter(self) -> Interpreter {
        match self {
            Self::Interpreter(interpreter) => interpreter.into_interpreter(),
            Self::Environment(environment) => environment.into_interpreter(),
        }
    }
}

/// Grab a file lock for the project environment to prevent concurrent writes across processes.
pub async fn lock_project_environment(
    target: ProjectEnvironmentTarget<'_>,
) -> Result<LockedFile, LockedFileError> {
    let install_path = target.install_path();
    LockedFile::acquire(
        std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(&install_path))),
        LockedFileMode::Exclusive,
        install_path.simplified_display(),
    )
    .await
}

/// Grab a file lock for the script environment to prevent concurrent writes across processes.
async fn lock_script_environment(script: Pep723ItemRef<'_>) -> Result<LockedFile, LockedFileError> {
    match script {
        Pep723ItemRef::Script(script) => {
            LockedFile::acquire(
                std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(&script.path))),
                LockedFileMode::Exclusive,
                script.path.simplified_display(),
            )
            .await
        }
        Pep723ItemRef::Remote(.., url) => {
            LockedFile::acquire(
                std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(url))),
                LockedFileMode::Exclusive,
                url.to_string(),
            )
            .await
        }
        Pep723ItemRef::Stdin(metadata) => {
            LockedFile::acquire(
                std::env::temp_dir().join(format!("uv-{}.lock", cache_digest(&metadata.raw))),
                LockedFileMode::Exclusive,
                "stdin".to_string(),
            )
            .await
        }
    }
}

/// The Python environment for a project.
#[derive(Debug)]
pub enum ProjectEnvironment {
    /// An existing [`PythonEnvironment`] was accepted by the compatibility policy.
    Existing(PythonEnvironment),
    /// An existing [`PythonEnvironment`] was discovered, but did not satisfy the project's
    /// requirements, and so was replaced.
    Replaced(PythonEnvironment),
    /// A new [`PythonEnvironment`] was created.
    Created(PythonEnvironment),
    /// An existing [`PythonEnvironment`] was discovered, but did not satisfy the project's
    /// requirements. A new environment would've been created, but `--dry-run` mode is enabled; as
    /// such, a temporary environment was created instead.
    WouldReplace(
        PathBuf,
        PythonEnvironment,
        #[allow(unused)] tempfile::TempDir,
    ),
    /// A new [`PythonEnvironment`] would've been created, but `--dry-run` mode is enabled; as such,
    /// a temporary environment was created instead.
    WouldCreate(
        PathBuf,
        PythonEnvironment,
        #[allow(unused)] tempfile::TempDir,
    ),
}

impl ProjectEnvironment {
    /// Initialize a virtual environment for the current project.
    pub async fn get_or_init(
        target: ProjectEnvironmentTarget<'_>,
        frozen_target: Option<InstallTarget<'_>>,
        groups: &DependencyGroupsWithDefaults,
        python: Option<PythonRequest>,
        install_mirrors: &PythonInstallMirrors,
        client_builder: &BaseClientBuilder<'_>,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        no_sync: bool,
        config_discovery: ConfigDiscovery,
        active: ActiveEnvironment,
        cache: &Cache,
        dry_run: DryRun,
        link_error_reporting: LinkErrorReporting,
        printer: Printer,
    ) -> Result<Self, EnvironmentError> {
        let environment_selection =
            ProjectEnvironmentSelection::from_install_path(target.install_path(), active);
        let centralized = centralized_environments_enabled(&environment_selection, cache);

        // Lock the project environment to avoid synchronization issues.
        let _lock = lock_project_environment(target)
            .await
            .inspect_err(|err| {
                warn!("Failed to acquire project environment lock: {err}");
            })
            .ok();

        // A selected installation target narrows group requirements. Otherwise a lockfile-only
        // environment uses the requirements of the entire workspace.
        let frozen_target = frozen_target.or_else(|| match target {
            ProjectEnvironmentTarget::Workspace(_) => None,
            ProjectEnvironmentTarget::Lockfile { root, lock } => Some(InstallTarget::Lockfile {
                root,
                project_name: lock.root().map(uv_lock::Package::name),
                selection: PackageSelection::Workspace,
                lock,
            }),
        });
        let project_python = if let Some(frozen_target) = frozen_target {
            ProjectPythonRequest::from_requirements(
                python,
                Some(frozen_target.install_path()),
                Some(frozen_target.python_requirement(groups)?),
                target.install_path(),
                config_discovery,
            )
            .await?
        } else {
            ProjectPythonRequest::from_request(
                python,
                target.workspace(),
                groups,
                target.install_path(),
                config_discovery,
            )
            .await?
        };

        match ProjectInterpreter::discover(
            target,
            project_python,
            client_builder,
            python_preference,
            python_arch,
            python_downloads,
            install_mirrors,
            if no_sync {
                ProjectEnvironmentPolicy::Preserve
            } else {
                ProjectEnvironmentPolicy::Compatible
            },
            active,
            cache,
            printer,
        )
        .await?
        {
            // Use the environment accepted by the compatibility policy.
            ProjectInterpreter::Environment(environment) => {
                if centralized && !dry_run.enabled() {
                    update_project_environment_link(&environment, target, link_error_reporting);
                }
                Ok(Self::Existing(environment))
            }

            // Otherwise, create a virtual environment with the discovered interpreter.
            ProjectInterpreter::Interpreter(interpreter) => {
                let requested = interpreter.into_requested_interpreter();
                let upgrade_policy = UpgradePolicy::from_request(requested.request());
                let interpreter = requested.into_interpreter();
                let root = if centralized {
                    centralized_environment_root(target, &interpreter, upgrade_policy, cache)
                } else {
                    environment_selection
                        .explicit_path()
                        .map_or_else(|| target.install_path().join(".venv"), Path::to_path_buf)
                };
                let centralized_environment_reference =
                    !centralized && is_centralized_environment_reference(&root, cache);

                // Avoid removing things that are not virtual environments and are outside the
                // environment cache.
                let replace_environment = if centralized_environment_reference {
                    true
                } else {
                    match (root.try_exists(), root.join("pyvenv.cfg").try_exists()) {
                        // It's a virtual environment we can remove it
                        (_, Ok(true)) => true,
                        // It doesn't exist at all, we should use it without deleting it to avoid TOCTOU bugs
                        (Ok(false), Ok(false)) => false,
                        // If it's not a virtual environment, bail
                        (Ok(true), Ok(false)) => {
                            // Unless it's empty, in which case we just ignore it
                            if root.read_dir().is_ok_and(|mut dir| dir.next().is_none()) {
                                false
                            } else if centralized {
                                // Unless it's the derived cache entry, which is uv-owned and safe to replace
                                true
                            } else {
                                return Err(EnvironmentError::InvalidProjectEnvironmentDir(
                                    root,
                                    "it is not a compatible environment but cannot be recreated because it is not a virtual environment".to_string(),
                                ));
                            }
                        }
                        // Similarly, if we can't _tell_ if it exists we should bail
                        (_, Err(err)) | (Err(err), _) => {
                            return Err(EnvironmentError::InvalidProjectEnvironmentDir(
                                root,
                                format!(
                                    "it is not a compatible environment but cannot be recreated because uv cannot determine if it is a virtual environment: {err}"
                                ),
                            ));
                        }
                    }
                };

                // Determine a prompt for the environment, in order of preference:
                //
                // 1) The name of the project
                // 2) The name of the directory at the root of the workspace
                // 3) No prompt
                let prompt = target
                    .project_name()
                    .map(ToString::to_string)
                    .or_else(|| {
                        target
                            .install_path()
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                    })
                    .map(uv_virtualenv::Prompt::Static)
                    .unwrap_or(uv_virtualenv::Prompt::None);

                // Under `--dry-run`, avoid modifying the environment.
                if dry_run.enabled() {
                    let temp_dir = cache.venv_dir()?;
                    let environment = uv_virtualenv::create_venv(
                        temp_dir.path(),
                        interpreter,
                        prompt,
                        false,
                        uv_virtualenv::OnExisting::Remove(
                            uv_virtualenv::RemovalReason::ManagedEnvironment,
                        ),
                        uv_preview::is_enabled(PreviewFeature::RelocatableEnvsDefault),
                        uv_virtualenv::Seed::Disabled,
                        upgrade_policy,
                    )?;
                    return Ok(if replace_environment {
                        Self::WouldReplace(root, environment, temp_dir)
                    } else {
                        Self::WouldCreate(root, environment, temp_dir)
                    });
                }

                if replace_environment {
                    // Remove centralized references directly to preserve their cached targets.
                    let removed = if centralized_environment_reference {
                        match uv_fs::remove_virtualenv(&root) {
                            Ok(()) => true,
                            Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                            Err(err) => return Err(uv_virtualenv::Error::from(err).into()),
                        }
                    } else {
                        uv_fs::clear_virtualenv(&root).map_err(uv_virtualenv::Error::from)?
                    };
                    if removed {
                        let removed_entry = if centralized_environment_reference {
                            "link to project environment"
                        } else {
                            "virtual environment"
                        };
                        writeln!(
                            printer.stderr(),
                            "Removed {removed_entry} at: {}",
                            root.user_display().cyan()
                        )?;
                    }
                }

                if centralized {
                    writeln!(
                        printer.stderr(),
                        "Creating virtual environment `{}`",
                        root.file_name()
                            .unwrap_or(root.as_os_str())
                            .to_string_lossy()
                            .cyan(),
                    )?;
                } else {
                    writeln!(
                        printer.stderr(),
                        "Creating virtual environment at: {}",
                        root.user_display().cyan()
                    )?;
                }

                let environment = uv_virtualenv::create_venv(
                    &root,
                    interpreter,
                    prompt,
                    false,
                    uv_virtualenv::OnExisting::Remove(
                        uv_virtualenv::RemovalReason::ManagedEnvironment,
                    ),
                    uv_preview::is_enabled(PreviewFeature::RelocatableEnvsDefault),
                    uv_virtualenv::Seed::Disabled,
                    upgrade_policy,
                )?;
                environment.cache_virtualenv(false, cache)?;

                if centralized {
                    update_project_environment_link(&environment, target, link_error_reporting);
                }

                if replace_environment {
                    Ok(Self::Replaced(environment))
                } else {
                    Ok(Self::Created(environment))
                }
            }
        }
    }

    /// Convert the [`ProjectEnvironment`] into a [`PythonEnvironment`].
    ///
    /// Returns an error if the environment was created in `--dry-run` mode, as dropping the
    /// associated temporary directory could lead to errors downstream.
    pub fn into_environment(self) -> Result<PythonEnvironment, EnvironmentError> {
        match self {
            Self::Existing(environment) => Ok(environment),
            Self::Replaced(environment) => Ok(environment),
            Self::Created(environment) => Ok(environment),
            Self::WouldReplace(..) => Err(EnvironmentError::DroppedEnvironment),
            Self::WouldCreate(..) => Err(EnvironmentError::DroppedEnvironment),
        }
    }

    /// Return the path to the actual target, if this was a dry run environment.
    pub fn dry_run_target(&self) -> Option<&Path> {
        match self {
            Self::WouldReplace(path, _, _) | Self::WouldCreate(path, _, _) => Some(path),
            Self::Created(_) | Self::Existing(_) | Self::Replaced(_) => None,
        }
    }
}

impl std::ops::Deref for ProjectEnvironment {
    type Target = PythonEnvironment;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Existing(environment) => environment,
            Self::Replaced(environment) => environment,
            Self::Created(environment) => environment,
            Self::WouldReplace(_, environment, _) => environment,
            Self::WouldCreate(_, environment, _) => environment,
        }
    }
}

/// The Python environment for a script.
#[derive(Debug)]
pub enum ScriptEnvironment {
    /// An existing [`PythonEnvironment`] was discovered, which satisfies the script's requirements.
    Existing(PythonEnvironment),
    /// An existing [`PythonEnvironment`] was discovered, but did not satisfy the script's
    /// requirements, and so was replaced.
    Replaced(PythonEnvironment),
    /// A new [`PythonEnvironment`] was created for the script.
    Created(PythonEnvironment),
    /// An existing [`PythonEnvironment`] was discovered, but did not satisfy the script's
    /// requirements. A new environment would've been created, but `--dry-run` mode is enabled; as
    /// such, a temporary environment was created instead.
    WouldReplace(
        PathBuf,
        PythonEnvironment,
        #[allow(unused)] tempfile::TempDir,
    ),
    /// A new [`PythonEnvironment`] would've been created, but `--dry-run` mode is enabled; as such,
    /// a temporary environment was created instead.
    WouldCreate(
        PathBuf,
        PythonEnvironment,
        #[allow(unused)] tempfile::TempDir,
    ),
}

impl ScriptEnvironment {
    /// Initialize a virtual environment for a PEP 723 script.
    pub async fn get_or_init(
        script: Pep723ItemRef<'_>,
        python_request: Option<PythonRequest>,
        client_builder: &BaseClientBuilder<'_>,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
        no_sync: bool,
        config_discovery: ConfigDiscovery,
        active: ActiveEnvironment,
        cache: &Cache,
        dry_run: DryRun,
        printer: Printer,
    ) -> Result<Self, EnvironmentError> {
        // Lock the script environment to avoid synchronization issues.
        let _lock = lock_script_environment(script)
            .await
            .inspect_err(|err| {
                warn!("Failed to acquire script environment lock: {err}");
            })
            .ok();

        match ScriptInterpreter::discover(
            script,
            python_request,
            client_builder,
            python_preference,
            python_arch,
            python_downloads,
            install_mirrors,
            no_sync,
            config_discovery,
            active,
            cache,
            printer,
        )
        .await?
        {
            // If we found an existing, compatible environment, use it.
            ScriptInterpreter::Environment(environment) => Ok(Self::Existing(environment)),

            // Otherwise, create a virtual environment with the discovered interpreter.
            ScriptInterpreter::Interpreter(requested) => {
                let upgrade_policy = UpgradePolicy::from_request(requested.request());
                let interpreter = requested.into_interpreter();
                let root = ScriptInterpreter::root(script, active, cache);

                // Determine a prompt for the environment, in order of preference:
                //
                // 1) The name of the script
                // 2) No prompt
                let prompt = script
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|f| f.to_string_lossy().to_string())
                    .map(uv_virtualenv::Prompt::Static)
                    .unwrap_or(uv_virtualenv::Prompt::None);

                // Under `--dry-run`, avoid modifying the environment.
                if dry_run.enabled() {
                    let temp_dir = cache.venv_dir()?;
                    let environment = uv_virtualenv::create_venv(
                        temp_dir.path(),
                        interpreter,
                        prompt,
                        false,
                        uv_virtualenv::OnExisting::Remove(
                            uv_virtualenv::RemovalReason::ManagedEnvironment,
                        ),
                        false,
                        uv_virtualenv::Seed::Disabled,
                        upgrade_policy,
                    )?;
                    return Ok(if root.exists() {
                        Self::WouldReplace(root, environment, temp_dir)
                    } else {
                        Self::WouldCreate(root, environment, temp_dir)
                    });
                }

                // Remove the existing virtual environment.
                let replaced = match uv_fs::remove_virtualenv(&root) {
                    Ok(()) => {
                        debug!(
                            "Removed virtual environment at: {}",
                            root.user_display().cyan()
                        );
                        true
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                    Err(err) => return Err(uv_virtualenv::Error::from(err).into()),
                };

                debug!(
                    "Creating script environment at: {}",
                    root.user_display().cyan()
                );

                let environment = uv_virtualenv::create_venv(
                    &root,
                    interpreter,
                    prompt,
                    false,
                    uv_virtualenv::OnExisting::Remove(
                        uv_virtualenv::RemovalReason::ManagedEnvironment,
                    ),
                    false,
                    uv_virtualenv::Seed::Disabled,
                    upgrade_policy,
                )?;
                environment.cache_virtualenv(false, cache)?;

                Ok(if replaced {
                    Self::Replaced(environment)
                } else {
                    Self::Created(environment)
                })
            }
        }
    }

    /// Convert the [`ScriptEnvironment`] into a [`PythonEnvironment`].
    ///
    /// Returns an error if the environment was created in `--dry-run` mode, as dropping the
    /// associated temporary directory could lead to errors downstream.
    pub fn into_environment(self) -> Result<PythonEnvironment, EnvironmentError> {
        match self {
            Self::Existing(environment) => Ok(environment),
            Self::Replaced(environment) => Ok(environment),
            Self::Created(environment) => Ok(environment),
            Self::WouldReplace(..) => Err(EnvironmentError::DroppedEnvironment),
            Self::WouldCreate(..) => Err(EnvironmentError::DroppedEnvironment),
        }
    }

    /// Return the path to the actual target, if this was a dry run environment.
    pub fn dry_run_target(&self) -> Option<&Path> {
        match self {
            Self::WouldReplace(path, _, _) | Self::WouldCreate(path, _, _) => Some(path),
            Self::Created(_) | Self::Existing(_) | Self::Replaced(_) => None,
        }
    }
}

impl std::ops::Deref for ScriptEnvironment {
    type Target = PythonEnvironment;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Existing(environment) => environment,
            Self::Replaced(environment) => environment,
            Self::Created(environment) => environment,
            Self::WouldReplace(_, environment, _) => environment,
            Self::WouldCreate(_, environment, _) => environment,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PreferenceLocation<'lock> {
    /// The preferences should be extracted from a lockfile.
    Lock {
        lock: &'lock Lock,
        install_path: &'lock Path,
    },
    /// The preferences will be provided directly as [`Preference`] entries.
    Entries(Vec<Preference>),
}

#[derive(Debug, Clone)]
pub struct EnvironmentSpecification<'lock> {
    /// The requirements to include in the environment.
    requirements: RequirementsSpecification,
    /// The preferences to respect when resolving.
    preferences: Option<PreferenceLocation<'lock>>,
}

impl From<RequirementsSpecification> for EnvironmentSpecification<'_> {
    fn from(requirements: RequirementsSpecification) -> Self {
        Self {
            requirements,
            preferences: None,
        }
    }
}

impl<'lock> EnvironmentSpecification<'lock> {
    /// Set the [`PreferenceLocation`] for the specification.
    #[must_use]
    pub fn with_preferences(self, preferences: PreferenceLocation<'lock>) -> Self {
        Self {
            preferences: Some(preferences),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
pub enum EnvironmentResolution {
    Specific,
    Universal,
}

/// Run dependency resolution for an interpreter, returning the [`ResolverOutput`].
pub async fn resolve_environment(
    spec: EnvironmentSpecification<'_>,
    resolution_scope: EnvironmentResolution,
    interpreter: &Interpreter,
    python_platform: Option<&TargetTriple>,
    source_tree_editable_policy: SourceTreeEditablePolicy,
    build_constraints: Constraints,
    settings: &ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &PlatformState,
    logger: Box<dyn ResolveLogger>,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ResolverOutput, EnvironmentError> {
    warn_on_requirements_txt_setting(&spec.requirements, settings);

    let ResolverSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        fork_strategy,
        dependency_metadata,
        config_setting,
        config_settings_package,
        build_isolation,
        extra_build_dependencies,
        extra_build_variables,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
        sources,
        torch_backend,
        cuda_driver_version,
        amd_gpu_architecture,
    } = settings;

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        override_dependencies,
        excludes,
        source_trees,
        ..
    } = spec.requirements;

    let client_builder = client_builder.clone().keyring(*keyring_provider);

    // Determine the tags and marker environment to use for resolution.
    let (tags, resolver_environment) = match resolution_scope {
        EnvironmentResolution::Specific => {
            let tags = uv_resolve_operations::resolution_tags(None, python_platform, interpreter)?;
            let marker_environment =
                uv_resolve_operations::resolution_markers(None, python_platform, interpreter);
            (
                Some(tags),
                ResolverEnvironment::specific(marker_environment),
            )
        }
        EnvironmentResolution::Universal => (None, ResolverEnvironment::universal(Vec::new())),
    };
    let python_requirement = match resolution_scope {
        EnvironmentResolution::Specific => PythonRequirement::from_interpreter(interpreter),
        EnvironmentResolution::Universal => PythonRequirement::from_requires_python(
            interpreter,
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version()),
        ),
    };

    let python_platform = match resolution_scope {
        EnvironmentResolution::Specific => python_platform,
        EnvironmentResolution::Universal => None,
    };

    // Determine the PyTorch backend.
    let torch_backend = torch_backend
        .map(|mode| {
            TorchStrategy::from_mode(
                mode,
                python_platform
                    .map(|t| t.platform())
                    .as_ref()
                    .unwrap_or(interpreter.platform())
                    .os(),
                cuda_driver_version.clone(),
                *amd_gpu_architecture,
            )
        })
        .transpose()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(*index_strategy)
        .torch_backend(torch_backend.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build()?;

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = match build_isolation {
        uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
        uv_configuration::BuildIsolation::Shared => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            BuildIsolation::Shared(&environment)
        }
        uv_configuration::BuildIsolation::SharedPackage(packages) => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            BuildIsolation::SharedPackage(&environment, packages)
        }
    };

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease(prerelease.clone())
        .fork_strategy(*fork_strategy)
        .exclude_newer(exclude_newer.clone())
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::new();
    let hasher = match resolution_scope {
        EnvironmentResolution::Specific => HashStrategy::default(),
        EnvironmentResolution::Universal => HashStrategy::collect(HashCollection::Url),
    };
    let build_hasher = HashStrategy::from_constraints(
        &build_constraints,
        Some(&interpreter.to_resolver_marker_environment()),
        HashCheckingMode::Verify,
    )?;

    // When resolving from an interpreter, we assume an empty environment, so reinstalls aren't
    // relevant. Upgrades are only relevant for universal resolutions that use an existing lock as
    // a preference source.
    let reinstall = Reinstall::default();
    let upgrade = match resolution_scope {
        EnvironmentResolution::Specific => Upgrade::default(),
        EnvironmentResolution::Universal => upgrade.clone(),
    };

    // If an existing lockfile exists, build up a set of preferences.
    let preferences = match spec.preferences {
        Some(PreferenceLocation::Lock { lock, install_path }) => {
            let LockedRequirements { preferences, git } =
                read_lock_requirements(lock, install_path, &upgrade)?;

            // Populate the Git resolver.
            for ResolvedRepositoryReference { reference, sha } in git {
                debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
                state.git().insert(reference, sha);
            }

            preferences
        }
        Some(PreferenceLocation::Entries(entries)) => entries,
        None => vec![],
    };

    // Resolve the flat indexes from `--find-links`.
    let flat_index = FlatIndex::load(&client, cache, index_locations).await?;

    // Lower the extra build dependencies, if any.
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            .into_inner();

    // Create a build dispatch.
    let resolve_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        *index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        &extra_build_requires,
        extra_build_variables,
        *link_mode,
        build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources.clone(),
        source_tree_editable_policy,
        workspace_cache.clone(),
        concurrency.clone(),
        preview,
    );

    // Resolve the requirements.
    Ok(uv_resolve_operations::resolve(
        requirements,
        constraints,
        overrides,
        override_dependencies,
        excludes,
        source_trees,
        project,
        BTreeMap::default(),
        &extras,
        &groups,
        preferences,
        None,
        &hasher,
        &reinstall,
        &upgrade,
        tags.as_deref(),
        resolver_environment,
        python_requirement,
        interpreter.markers(),
        Conflicts::empty(),
        &client,
        &flat_index,
        state.index(),
        &resolve_dispatch,
        concurrency,
        options,
        None,
        logger,
        printer,
    )
    .await?
    .0)
}

/// Sync a [`PythonEnvironment`] with a set of resolved requirements.
pub async fn sync_environment(
    venv: PythonEnvironment,
    resolution: &Resolution,
    hasher: HashStrategy,
    modifications: Modifications,
    build_constraints: Constraints,
    settings: InstallerSettingsRef<'_>,
    client_builder: &BaseClientBuilder<'_>,
    state: &PlatformState,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    concurrency: &Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<PythonEnvironment, EnvironmentError> {
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        dependency_metadata,
        config_setting,
        config_settings_package,
        build_isolation,
        extra_build_dependencies,
        extra_build_variables,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
        sources,
    } = settings;

    let client_builder = client_builder.clone().keyring(keyring_provider);

    let site_packages = SitePackages::from_environment(&venv)?;

    // Determine the markers tags to use for resolution.
    let interpreter = venv.interpreter();
    let tags = venv.interpreter().tags()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(index_strategy)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build()?;

    // Determine whether to enable build isolation.
    let build_isolation = match build_isolation {
        uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
        uv_configuration::BuildIsolation::Shared => BuildIsolation::Shared(&venv),
        uv_configuration::BuildIsolation::SharedPackage(packages) => {
            BuildIsolation::SharedPackage(&venv, packages)
        }
    };

    let build_hasher = HashStrategy::from_constraints(
        &build_constraints,
        Some(&interpreter.to_resolver_marker_environment()),
        HashCheckingMode::Verify,
    )?;
    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let dry_run = DryRun::default();
    let workspace_cache = WorkspaceCache::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = FlatIndex::load(&client, cache, index_locations).await?;

    // Lower the extra build dependencies, if any.
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            .into_inner();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        &extra_build_requires,
        extra_build_variables,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources,
        SourceTreeEditablePolicy::Project,
        workspace_cache,
        concurrency.clone(),
        preview,
    );

    // Sync the environment.
    uv_install_operations::install(
        resolution,
        site_packages,
        InstallationStrategy::Permissive,
        modifications,
        reinstall,
        build_options,
        link_mode,
        compile_bytecode.then_some(uv_install_operations::BytecodeCompilation::All),
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
        preview,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    uv_resolve_operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(venv)
}

/// The result of updating a [`PythonEnvironment`] to satisfy a [`RequirementsSpecification`].
#[derive(Debug)]
pub struct EnvironmentUpdate {
    /// The updated [`PythonEnvironment`].
    pub environment: PythonEnvironment,
    /// The [`Changelog`] of changes made to the environment.
    pub changelog: Changelog,
}

/// Update a [`PythonEnvironment`] to satisfy a [`RequirementsSpecification`].
pub async fn update_environment(
    venv: PythonEnvironment,
    spec: RequirementsSpecification,
    modifications: Modifications,
    python_platform: Option<&TargetTriple>,
    source_tree_editable_policy: SourceTreeEditablePolicy,
    build_constraints: Constraints,
    extra_build_requires: ExtraBuildRequires,
    settings: &ResolverInstallerSettings,
    client_builder: &BaseClientBuilder<'_>,
    state: &SharedState,
    resolve: Box<dyn ResolveLogger>,
    install: Box<dyn InstallLogger>,
    installer_metadata: bool,
    concurrency: &Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
) -> Result<EnvironmentUpdate, EnvironmentError> {
    warn_on_requirements_txt_setting(&spec, &settings.resolver);

    let ResolverInstallerSettings {
        resolver:
            ResolverSettings {
                build_options,
                config_setting,
                config_settings_package,
                dependency_metadata,
                exclude_newer,
                fork_strategy,
                index_locations,
                index_strategy,
                keyring_provider,
                link_mode,
                build_isolation,
                extra_build_dependencies: _,
                extra_build_variables,
                prerelease,
                resolution,
                sources,
                torch_backend,
                cuda_driver_version,
                amd_gpu_architecture,
                upgrade,
            },
        compile_bytecode,
        reinstall,
    } = settings;

    let client_builder = client_builder.clone().keyring(*keyring_provider);

    // Respect all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        override_dependencies,
        excludes,
        source_trees,
        ..
    } = spec;

    // Determine markers and tags to use for resolution.
    let interpreter = venv.interpreter();
    let marker_env = uv_resolve_operations::resolution_markers(None, python_platform, interpreter);
    let tags = uv_resolve_operations::resolution_tags(None, python_platform, interpreter)?;

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_environment(&venv)?;
    if reinstall.is_none()
        && upgrade.is_none()
        && source_trees.is_empty()
        && matches!(modifications, Modifications::Sufficient)
    {
        match site_packages.satisfies_spec(
            &requirements,
            &constraints,
            &overrides,
            &override_dependencies,
            &excludes,
            dependency_metadata,
            DependencyMode::Transitive,
            InstallationStrategy::Permissive,
            &marker_env,
            &tags,
            config_setting,
            config_settings_package,
            &extra_build_requires,
            extra_build_variables,
        )? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                if recursive_requirements.is_empty() {
                    debug!("No requirements to install");
                } else {
                    debug!(
                        "All requirements satisfied: {}",
                        recursive_requirements
                            .iter()
                            .map(ToString::to_string)
                            .sorted()
                            .join(" | ")
                    );
                }
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

    // Determine the PyTorch backend.
    let torch_backend = torch_backend
        .map(|mode| {
            TorchStrategy::from_mode(
                mode,
                python_platform
                    .map(|t| t.platform())
                    .as_ref()
                    .unwrap_or(interpreter.platform())
                    .os(),
                cuda_driver_version.clone(),
                *amd_gpu_architecture,
            )
        })
        .transpose()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(*index_strategy)
        .torch_backend(torch_backend.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build()?;

    // Determine whether to enable build isolation.
    let build_isolation = match build_isolation {
        uv_configuration::BuildIsolation::Isolate => BuildIsolation::Isolated,
        uv_configuration::BuildIsolation::Shared => BuildIsolation::Shared(&venv),
        uv_configuration::BuildIsolation::SharedPackage(packages) => {
            BuildIsolation::SharedPackage(&venv, packages)
        }
    };

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease(prerelease.clone())
        .fork_strategy(*fork_strategy)
        .exclude_newer(exclude_newer.clone())
        .index_strategy(*index_strategy)
        .build_options(build_options.clone())
        .build();

    let build_hasher = HashStrategy::from_constraints(
        &build_constraints,
        Some(&interpreter.to_resolver_marker_environment()),
        HashCheckingMode::Verify,
    )?;
    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::new();
    let hasher = HashStrategy::default();
    let preferences = Vec::default();

    // Determine the tags to use for resolution.
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = FlatIndex::load(&client, cache, index_locations).await?;

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &build_constraints,
        interpreter,
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone(),
        *index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        &extra_build_requires,
        extra_build_variables,
        *link_mode,
        build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources.clone(),
        source_tree_editable_policy,
        workspace_cache.clone(),
        concurrency.clone(),
        preview,
    );

    // Resolve the requirements.
    let (resolution, hasher) = match uv_resolve_operations::resolve(
        requirements,
        constraints,
        overrides,
        override_dependencies,
        excludes,
        source_trees,
        project,
        BTreeMap::default(),
        &extras,
        &groups,
        preferences,
        Some(site_packages.clone()),
        &hasher,
        reinstall,
        upgrade,
        Some(&tags),
        ResolverEnvironment::specific(marker_env.clone()),
        python_requirement,
        venv.interpreter().markers(),
        Conflicts::empty(),
        &client,
        &flat_index,
        state.index(),
        &build_dispatch,
        concurrency,
        options,
        None,
        resolve,
        printer,
    )
    .await
    {
        Ok((resolution, hasher)) => (Resolution::from(resolution), hasher),
        Err(err) => return Err(err.into()),
    };
    // Sync the environment.
    let changelog = uv_install_operations::install(
        &resolution,
        site_packages,
        InstallationStrategy::Permissive,
        modifications,
        reinstall,
        build_options,
        *link_mode,
        (*compile_bytecode).then_some(uv_install_operations::BytecodeCompilation::All),
        &hasher,
        &tags,
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
        preview,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    uv_resolve_operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(EnvironmentUpdate {
        environment: venv,
        changelog,
    })
}

/// Validate that we aren't trying to install extras or groups that
/// are declared as conflicting.
pub fn detect_conflicts(
    target: &InstallTarget,
    extras: &ExtrasSpecification,
    groups: &DependencyGroupsWithDefaults,
) -> Result<(), EnvironmentError> {
    // Validate that we aren't trying to install extras or groups that
    // are declared as conflicting. Note that we need to collect all
    // extras and groups that match in a particular set, since extras
    // can be declared as conflicting with groups. So if extra `x` and
    // group `g` are declared as conflicting, then enabling both of
    // those should result in an error.
    let lock = target.lock();
    let packages = target.packages(extras, groups);
    let conflicts = lock.conflicts();
    for set in conflicts.iter() {
        let mut conflicts: Vec<ConflictItem> = vec![];
        for item in set.iter() {
            if !packages.contains(item.package()) {
                // Ignore items that are not in the install targets
                continue;
            }
            let is_conflicting = match item.kind() {
                ConflictKind::Project => groups.prod(),
                ConflictKind::Extra(extra) => extras.contains(extra),
                ConflictKind::Group(group1) => groups.contains(group1),
            };
            if is_conflicting {
                conflicts.push(item.clone());
            }
        }
        if conflicts.len() >= 2 {
            return Err(EnvironmentError::Conflict(ConflictError {
                set: set.clone(),
                conflicts,
                groups: groups.clone(),
            }));
        }
    }
    Ok(())
}

/// Warn if the user provides (e.g.) an `--index-url` in a requirements file.
fn warn_on_requirements_txt_setting(spec: &RequirementsSpecification, settings: &ResolverSettings) {
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
        warn_user_once!(
            "Ignoring `--no-index` from requirements file. Instead, use the `--no-index` command-line argument, or set `no-index` in a `uv.toml` or `pyproject.toml` file."
        );
    } else {
        if let Some(index_url) = index_url {
            if settings.index_locations.default_index().map(Index::url) != Some(index_url) {
                warn_user_once!(
                    "Ignoring `--index-url` value `{index_url}` from requirements file. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file."
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
                    "Ignoring `--extra-index-url` value `{extra_index_url}` from requirements file. Instead, use the `--extra-index-url` command-line argument, or set `extra-index-url` in a `uv.toml` or `pyproject.toml` file."
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
                    "Ignoring `--find-links` value `{find_link}` from requirements file. Instead, use the `--find-links` command-line argument, or set `find-links` in a `uv.toml` or `pyproject.toml` file."
                );
            }
        }
    }

    if !no_binary.is_none() && settings.build_options.no_binary() != no_binary {
        warn_user_once!(
            "Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-binary` command-line argument, or set `no-binary` in a `uv.toml` or `pyproject.toml` file."
        );
    }

    if !no_build.is_none() && settings.build_options.no_build() != no_build {
        warn_user_once!(
            "Ignoring `--no-binary` setting from requirements file. Instead, use the `--no-build` command-line argument, or set `no-build` in a `uv.toml` or `pyproject.toml` file."
        );
    }
}
