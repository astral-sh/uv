use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;

use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{ActiveEnvironment, Concurrency, DependencyGroupsWithDefaults, DryRun};
use uv_dispatch::UniversalState;
use uv_environment_operations::{
    ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter,
};
use uv_git_types::GitOid;
use uv_lock::{Lock, Package};
use uv_lock_operations::{
    LockError, LockMode, LockOperation, LockResult, LockTarget, MissingLockfileSource,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_preview::{Preview, PreviewFeature};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::PythonDownloadReporter;
use uv_python_discovery::ScriptInterpreter;
use uv_python_discovery::init_script_python_requirement;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest};
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_scripts::Pep723Script;
use uv_settings::{FrozenSource, LockCheck, PythonInstallMirrors, ResolverSettings};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::ScriptPath;

/// Resolve the project requirements into a lockfile.
pub async fn lock(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    dry_run: DryRun,
    refresh: Refresh,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    script: Option<ScriptPath>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> anyhow::Result<ExitStatus> {
    // If necessary, initialize the PEP 723 script.
    let script = match script {
        Some(ScriptPath::Path(path)) => {
            let reporter = PythonDownloadReporter::single(printer);
            let requires_python = init_script_python_requirement(
                python.as_deref(),
                &install_mirrors,
                project_dir,
                false,
                python_preference,
                python_arch,
                python_downloads,
                config_discovery,
                &client_builder,
                cache,
                &reporter,
            )
            .await?;
            Some(Pep723Script::init(&path, requires_python.specifiers()).await?)
        }
        Some(ScriptPath::Script(script)) => Some(script),
        None => None,
    };

    // Find the project requirements.
    let workspace;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        workspace = VirtualProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            cache,
            workspace_cache,
        )
        .await?;
        LockTarget::Workspace(workspace.workspace())
    };

    // Determine the lock mode.
    let interpreter;
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else {
        interpreter = match target {
            LockTarget::Workspace(workspace) => {
                // Don't enable any groups' requires-python for interpreter discovery
                let groups = DependencyGroupsWithDefaults::none();
                let project_python = ProjectPythonRequest::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(workspace),
                    &groups,
                    project_dir,
                    config_discovery,
                )
                .await?;
                ProjectInterpreter::discover(
                    ProjectEnvironmentTarget::from(workspace),
                    project_python,
                    &client_builder,
                    python_preference,
                    python_arch,
                    python_downloads,
                    &install_mirrors,
                    ProjectEnvironmentPolicy::Optional,
                    ActiveEnvironment::Ignore,
                    cache,
                    printer,
                )
                .await?
                .into_interpreter()
            }
            LockTarget::Script(script) => ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_arch,
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                ActiveEnvironment::Ignore,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        };

        if let LockCheck::Enabled(lock_check) = lock_check {
            LockMode::Locked(&interpreter, lock_check)
        } else if dry_run.enabled() {
            LockMode::DryRun(&interpreter)
        } else {
            LockMode::Write(&interpreter)
        }
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Perform the lock operation.
    match Box::pin(
        LockOperation::new(
            mode,
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            cache,
            workspace_cache,
            printer,
            preview,
        )
        .with_refresh(&refresh)
        .with_lockfile_contents_check(
            matches!(&refresh, Refresh::All(..))
                && preview.is_enabled(PreviewFeature::LockfileFormatCheck),
        )
        .execute(target),
    )
    .await
    {
        Ok(lock) => {
            if let Some(frozen_source) = frozen {
                warn_user!(
                    "The lockfile at `uv.lock` was only checked for validity, not whether it is up-to-date, because {} was provided; use `--check` instead",
                    MissingLockfileSource::from(frozen_source)
                );
            }

            if dry_run.enabled() {
                // In `--dry-run` mode, show all changes.
                if let LockResult::Changed(previous, lock) = &lock {
                    let mut changed = false;
                    for event in LockEvent::detect_changes(previous.as_ref(), lock, dry_run) {
                        changed = true;
                        writeln!(printer.stderr(), "{event}")?;
                    }

                    // If we didn't report any version changes, but the lockfile changed, report back.
                    if !changed {
                        writeln!(printer.stderr(), "{}", "Lockfile changes detected".bold())?;
                    }
                } else {
                    writeln!(
                        printer.stderr(),
                        "{}",
                        "No lockfile changes detected".bold()
                    )?;
                }
            } else {
                if let LockResult::Changed(Some(previous), lock) = &lock {
                    for event in LockEvent::detect_changes(Some(previous), lock, dry_run) {
                        writeln!(printer.stderr(), "{event}")?;
                    }
                }
            }

            Ok(ExitStatus::Success)
        }
        // Lock mismatches from `--check`/`--locked` are expected validation failures.
        Err(err @ (LockError::LockMismatch(..) | LockError::LockFormat(..))) => {
            Err(UvError::user(err).into())
        }
        Err(err) => Err(UvError::from(err).into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct LockEventVersion<'lock> {
    /// The version of the package, or `None` if the package has a dynamic version.
    version: Option<&'lock Version>,
    /// The short Git SHA of the package, if it was installed from a Git repository.
    sha: Option<&'lock str>,
}

impl<'lock> From<&'lock Package> for LockEventVersion<'lock> {
    fn from(value: &'lock Package) -> Self {
        Self {
            version: value.version(),
            sha: value.git_sha().map(GitOid::as_tiny_str),
        }
    }
}

impl std::fmt::Display for LockEventVersion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.version, self.sha) {
            (Some(version), Some(sha)) => write!(f, "v{version} ({sha})"),
            (Some(version), None) => write!(f, "v{version}"),
            (None, Some(sha)) => write!(f, "(dynamic) ({sha})"),
            (None, None) => write!(f, "(dynamic)"),
        }
    }
}

/// A modification to a lockfile.
#[derive(Debug, Clone)]
pub(super) enum LockEvent<'lock> {
    Update(
        DryRun,
        PackageName,
        BTreeSet<LockEventVersion<'lock>>,
        BTreeSet<LockEventVersion<'lock>>,
    ),
    Add(DryRun, PackageName, BTreeSet<LockEventVersion<'lock>>),
    Remove(DryRun, PackageName, BTreeSet<LockEventVersion<'lock>>),
}

impl<'lock> LockEvent<'lock> {
    /// Detect the change events between an (optional) existing and updated lockfile.
    pub(super) fn detect_changes(
        existing_lock: Option<&'lock Lock>,
        new_lock: &'lock Lock,
        dry_run: DryRun,
    ) -> impl Iterator<Item = Self> {
        // Identify the package-versions in the existing lockfile.
        let mut existing_packages: FxHashMap<&PackageName, BTreeSet<LockEventVersion>> =
            if let Some(existing_lock) = existing_lock {
                existing_lock.packages().iter().fold(
                    FxHashMap::with_capacity_and_hasher(
                        existing_lock.packages().len(),
                        FxBuildHasher,
                    ),
                    |mut acc, package| {
                        acc.entry(package.name())
                            .or_default()
                            .insert(LockEventVersion::from(package));
                        acc
                    },
                )
            } else {
                FxHashMap::default()
            };

        // Identify the package-versions in the updated lockfile.
        let mut new_packages: FxHashMap<&PackageName, BTreeSet<LockEventVersion>> =
            new_lock.packages().iter().fold(
                FxHashMap::with_capacity_and_hasher(new_lock.packages().len(), FxBuildHasher),
                |mut acc, package| {
                    acc.entry(package.name())
                        .or_default()
                        .insert(LockEventVersion::from(package));
                    acc
                },
            );

        let names = existing_packages
            .keys()
            .chain(new_packages.keys())
            .map(|name| (*name).clone())
            .collect::<BTreeSet<_>>();

        names.into_iter().filter_map(move |name| {
            match (existing_packages.remove(&name), new_packages.remove(&name)) {
                (Some(existing_versions), Some(new_versions)) => {
                    if existing_versions != new_versions {
                        Some(Self::Update(dry_run, name, existing_versions, new_versions))
                    } else {
                        None
                    }
                }
                (Some(existing_versions), None) => {
                    Some(Self::Remove(dry_run, name, existing_versions))
                }
                (None, Some(new_versions)) => Some(Self::Add(dry_run, name, new_versions)),
                (None, None) => {
                    unreachable!("The key `{name}` should exist in at least one of the maps");
                }
            }
        })
    }

    pub(super) fn package(&self) -> &PackageName {
        match self {
            Self::Update(_, package, ..)
            | Self::Add(_, package, ..)
            | Self::Remove(_, package, ..) => package,
        }
    }
}

impl std::fmt::Display for LockEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Update(dry_run, name, existing_versions, new_versions) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let new_versions = new_versions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {existing_versions} -> {new_versions}",
                    if dry_run.enabled() {
                        "Update"
                    } else {
                        "Updated"
                    }
                    .green()
                    .bold()
                )
            }
            Self::Add(dry_run, name, new_versions) => {
                let new_versions = new_versions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {new_versions}",
                    if dry_run.enabled() { "Add" } else { "Added" }
                        .green()
                        .bold()
                )
            }
            Self::Remove(dry_run, name, existing_versions) => {
                let existing_versions = existing_versions
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");

                write!(
                    f,
                    "{} {name} {existing_versions}",
                    if dry_run.enabled() {
                        "Remove"
                    } else {
                        "Removed"
                    }
                    .red()
                    .bold()
                )
            }
        }
    }
}
