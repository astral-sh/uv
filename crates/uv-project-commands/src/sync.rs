use std::collections::BTreeSet;
use std::fmt::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Result;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use serde::Serialize;
use tracing::warn;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, Constraints, DependencyGroups, DryRun, EditableMode,
    ExtrasSpecification, InstallOptions, Modifications, SyncFormat, TargetTriple,
};
use uv_dispatch::{PlatformState, UniversalState};
use uv_distribution_types::NameRequirementSpecification;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::malware::MalwareCheckContext;
use uv_environment_operations::{
    EnvironmentError, EnvironmentUpdate, LinkErrorReporting, ProjectEnvironment,
    ProjectEnvironmentTarget, ScriptEnvironment, detect_conflicts, sync_from_lock,
    update_environment,
};
use uv_fs::{PortablePathBuf, Simplified};
use uv_install_operations::Changelog;
use uv_install_operations::loggers::DefaultInstallLogger;
use uv_install_operations::report::{PackageChangesReport, SchemaReport};
use uv_lock::{Installable, Lock, PythonReport};
use uv_lock_operations::{
    DiscoveredProject, FrozenWorkspace, LockError, LockMode, LockOperation, LockResult, LockTarget,
    MissingLockfileSource,
};
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_python_discovery::ConfigDiscovery;
use uv_python_interpreter::PythonEnvironment;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest};
use uv_requirements::{script_extra_build_requires, script_specification};
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_scripts::Pep723Script;
use uv_settings::{
    FrozenSource, LockCheck, LockedSource, MalwareCheckSettings, PythonInstallMirrors,
    ResolverInstallerSettings,
};
use uv_types::SourceTreeEditablePolicy;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace, WorkspaceCache};

/// Sync the project environment.
pub async fn sync(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    dry_run: DryRun,
    active: ActiveEnvironment,
    all_packages: bool,
    package: Vec<PackageName>,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    editable: Option<EditableMode>,
    install_options: InstallOptions,
    modifications: Modifications,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    script: Option<Pep723Script>,
    installer_metadata: bool,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
    output_format: SyncFormat,
    malware_settings: MalwareCheckSettings,
) -> Result<ExitStatus> {
    if preview.is_enabled(PreviewFeature::JsonOutput) && matches!(output_format, SyncFormat::Json) {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    // Identify the target.
    let manifest_target;
    let frozen_workspace;
    let target = if let Some(script) = script {
        manifest_target = SyncManifest::Script(script);
        SyncTarget::Manifest(&manifest_target)
    } else {
        let options = DiscoveryOptions {
            members: if frozen.is_some() {
                MemberDiscovery::Existing
            } else {
                MemberDiscovery::All
            },
            ..DiscoveryOptions::default()
        };
        let selected_package = if let [name] = package.as_slice()
            && frozen.is_none()
        {
            Some(name)
        } else {
            None
        };
        match DiscoveredProject::discover(
            project_dir,
            &options,
            selected_package,
            frozen,
            preview,
            cache,
            workspace_cache,
        )
        .await?
        {
            DiscoveredProject::Manifest(project) => {
                if frozen.is_none() {
                    for name in &package {
                        if !project.workspace().packages().contains_key(name) {
                            return Err(anyhow::anyhow!("Package `{name}` not found in workspace"));
                        }
                    }
                }
                manifest_target = SyncManifest::Project(project);
                SyncTarget::Manifest(&manifest_target)
            }
            DiscoveredProject::Lockfile(workspace) => {
                workspace.validate_packages(&package)?;
                frozen_workspace = workspace;
                let project_name = frozen_workspace.current_project(project_dir).cloned();
                SyncTarget::Lockfile {
                    path: frozen_workspace.root().join("uv.lock"),
                    workspace: &frozen_workspace,
                    project_name,
                }
            }
        }
    };

    // Read the frozen lock before selecting an environment, since the selected member's default
    // groups can affect the Python requirement. Manifest-free targets were read during discovery.
    let frozen_lock = if let Some(source) = frozen
        && let SyncTarget::Manifest(manifest) = &target
    {
        Some(
            LockTarget::from(*manifest)
                .read_frozen(MissingLockfileSource::from(source))
                .await
                .map_err(|err| match (err, *manifest) {
                    (LockError::MissingLockfile(..), SyncManifest::Script(script)) => anyhow::anyhow!(
                        "`uv sync --frozen` requires a script lockfile; run `{}` to lock the script",
                        format!("uv lock --script {}", script.path.user_display()).green(),
                    ),
                    (err, _) => UvError::from(err).into(),
                })?,
        )
    } else {
        None
    };

    let locked_default_groups = match (&frozen_lock, package.as_slice()) {
        (Some(lock), [name]) => lock.member_default_groups(name),
        _ => None,
    };
    let use_locked_python = locked_default_groups.is_some();

    // Determine the groups and extras to include.
    let groups = match &target {
        SyncTarget::Manifest(SyncManifest::Project(project)) => {
            groups.with_defaults(match locked_default_groups {
                Some(defaults) => defaults,
                None => project.default_groups()?,
            })
        }
        SyncTarget::Manifest(SyncManifest::Script(..)) => {
            groups.with_defaults(DefaultGroups::default())
        }
        SyncTarget::Lockfile {
            workspace,
            project_name,
            ..
        } => {
            let project = match package.as_slice() {
                [name] => Some(name),
                _ => project_name.as_ref(),
            };
            workspace.resolve_groups(&groups, project)?
        }
    };
    let extras = extras.with_defaults(DefaultExtras::default());

    // Reject invalid lockfile selections before creating an environment.
    if let SyncTarget::Lockfile { workspace, .. } = &target {
        let install_target =
            identify_installation_target(&target, workspace.lock(), all_packages, &package);
        install_target.validate_extras(&extras)?;
        install_target.validate_groups(&groups)?;
        detect_conflicts(&install_target, &extras, &groups)?;
    }

    // Discover or create the virtual environment.
    let environment = match &target {
        SyncTarget::Manifest(SyncManifest::Project(project)) => SyncEnvironment::Project(
            ProjectEnvironment::get_or_init(
                ProjectEnvironmentTarget::from(project.workspace()),
                frozen_lock
                    .as_ref()
                    .filter(|_| use_locked_python)
                    .map(|lock| {
                        identify_installation_target(&target, lock, all_packages, &package)
                    }),
                &groups,
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &client_builder,
                python_preference,
                python_arch,
                python_downloads,
                false,
                config_discovery,
                active,
                cache,
                dry_run,
                LinkErrorReporting::User,
                printer,
            )
            .await?,
        ),
        SyncTarget::Lockfile { workspace, .. } => SyncEnvironment::Project(
            ProjectEnvironment::get_or_init(
                ProjectEnvironmentTarget::Lockfile {
                    root: workspace.root(),
                    lock: workspace.lock(),
                },
                Some(identify_installation_target(
                    &target,
                    workspace.lock(),
                    all_packages,
                    &package,
                )),
                &groups,
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &client_builder,
                python_preference,
                python_arch,
                python_downloads,
                false,
                config_discovery,
                active,
                cache,
                dry_run,
                LinkErrorReporting::User,
                printer,
            )
            .await?,
        ),
        SyncTarget::Manifest(SyncManifest::Script(script)) => SyncEnvironment::Script(
            ScriptEnvironment::get_or_init(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_arch,
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                active,
                cache,
                dry_run,
                printer,
            )
            .await?,
        ),
    };

    let _lock = environment
        .lock()
        .await
        .inspect_err(|err| {
            warn!("Failed to acquire environment lock: {err}");
        })
        .ok();

    let sync_report = SyncReport {
        dry_run: dry_run.enabled(),
        environment: EnvironmentReport::from(&environment),
        action: SyncAction::from(&environment),
        target: TargetName::from(&target),
        changes: PackageChangesReport::default(),
    };

    // Show the intermediate results if relevant
    if let Some(message) = sync_report.format(output_format) {
        writeln!(printer.stderr(), "{message}")?;
    }

    // Special-case: we're syncing a script that doesn't have an associated lockfile. In that case,
    // we don't create a lockfile, so the resolve-and-install semantics are different.
    if let SyncTarget::Manifest(SyncManifest::Script(script)) = &target
        && frozen_lock.is_none()
    {
        let lockfile = LockTarget::from(script).lock_path();
        if !lockfile.is_file() {
            if let LockCheck::Enabled(lock_check) = lock_check {
                return Err(anyhow::anyhow!(
                    "`uv sync {lock_check}` requires a script lockfile; run `{}` to lock the script",
                    format!("uv lock --script {}", script.path.user_display()).green(),
                ));
            }

            // Parse the requirements from the script.
            let spec = script_specification(
                script.into(),
                &settings.resolver.sources,
                &settings.resolver.index_locations,
                cache,
                workspace_cache,
                client_builder.credentials_cache(),
            )
            .await?
            .unwrap_or_default();
            let script_extra_build_requires = script_extra_build_requires(
                script.into(),
                &settings.resolver.sources,
                &settings.resolver.index_locations,
                cache,
                workspace_cache,
                client_builder.credentials_cache(),
            )
            .await?
            .into_inner();

            // Parse the build constraints from the script.
            let build_constraints = script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| {
                    tool.uv
                        .as_ref()
                        .and_then(|uv| uv.build_constraint_dependencies.as_ref())
                })
                .map(|constraints| {
                    Constraints::from_specifications(
                        constraints
                            .iter()
                            .cloned()
                            .map(NameRequirementSpecification::from),
                    )
                });

            match update_environment(
                environment.clone(),
                spec,
                modifications,
                python_platform.as_ref(),
                SourceTreeEditablePolicy::Project,
                build_constraints.unwrap_or_default(),
                script_extra_build_requires,
                &settings,
                &client_builder,
                &PlatformState::default(),
                Box::new(DefaultResolveLogger),
                Box::new(DefaultInstallLogger),
                installer_metadata,
                &concurrency,
                cache,
                workspace_cache,
                dry_run,
                printer,
                preview,
            )
            .await
            {
                Ok(EnvironmentUpdate { changelog, .. }) => {
                    write_sync_report(
                        &target,
                        &environment,
                        &changelog,
                        None,
                        dry_run,
                        output_format,
                        printer,
                    )?;
                    return Ok(ExitStatus::Success);
                }
                Err(EnvironmentError::Install(error)) => {
                    let error = *error;
                    if let Some(changelog) = error.outdated_environment() {
                        write_sync_report(
                            &target,
                            &environment,
                            changelog,
                            None,
                            dry_run,
                            output_format,
                            printer,
                        )?;
                    }
                    return Err(UvError::from(error).into());
                }
                Err(err) => return Err(UvError::from(err).into()),
            }
        }
    }

    // Initialize any shared state.
    let state = UniversalState::default();

    // Determine the lock mode.
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(environment.interpreter(), lock_check)
    } else if dry_run.enabled() {
        LockMode::DryRun(environment.interpreter())
    } else {
        LockMode::Write(environment.interpreter())
    };

    let (outcome, lock_report) = match &target {
        SyncTarget::Lockfile {
            path, workspace, ..
        } => (
            Outcome::Frozen(workspace.lock()),
            LockReport {
                path: path.as_path().into(),
                action: LockAction::Use,
                dry_run: dry_run.enabled(),
            },
        ),
        SyncTarget::Manifest(manifest) => {
            let lock_target = LockTarget::from(*manifest);
            let first_party_exclusions = target.project().map_or_else(BTreeSet::new, |project| {
                PackageSelection::from_args(all_packages, &package, project.project_name())
                    .first_party_exclusions(
                        project.workspace(),
                        project.project_name(),
                        &install_options,
                    )
            });

            let result = if let Some(lock) = frozen_lock {
                Ok(LockResult::Unchanged(lock))
            } else {
                Box::pin(
                    LockOperation::new(
                        mode,
                        &settings.resolver,
                        &client_builder,
                        &state,
                        Box::new(DefaultResolveLogger),
                        &concurrency,
                        cache,
                        workspace_cache,
                        printer,
                        preview,
                    )
                    .with_first_party_exclusions(first_party_exclusions)
                    .execute(lock_target),
                )
                .await
            };
            let outcome = match result {
                Ok(result) => Outcome::Success(result),
                Err(LockError::Resolve(err)) => return Err(UvError::from(*err).into()),
                Err(err @ LockError::LockFormat(..)) => return Err(UvError::user(err).into()),
                Err(LockError::LockMismatch(prev, cur, lock_source)) => {
                    if dry_run.enabled() {
                        // A dry run continues with the new resolution but exits unsuccessfully.
                        Outcome::LockMismatch(prev, cur, lock_source)
                    } else {
                        return Err(
                            UvError::user(LockError::LockMismatch(prev, cur, lock_source)).into(),
                        );
                    }
                }
                Err(err) => return Err(UvError::from(err).into()),
            };
            let report = LockReport::from((&lock_target, &mode, &outcome));
            (outcome, report)
        }
    };

    if let Some(message) = lock_report.format(output_format) {
        writeln!(printer.stderr(), "{message}")?;
    }

    // Identify the installation target.
    let sync_target = identify_installation_target(&target, outcome.lock(), all_packages, &package);

    // TODO(lucab): improve warning content
    // <https://github.com/astral-sh/uv/issues/7428>
    if let SyncTarget::Manifest(SyncManifest::Project(project)) = &target {
        let roots = sync_target.roots().collect::<FxHashSet<_>>();
        for (name, member) in project.workspace().packages() {
            let is_required_member = project.workspace().required_members().contains_key(name);
            if roots.contains(name)
                && member.pyproject_toml().has_scripts()
                && !member.pyproject_toml().is_package(!is_required_member)
            {
                warn_user!(
                    "Skipping installation of entry points (`project.scripts`) for package `{}` because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`",
                    name
                );
            }
        }
    }

    let state = state.fork();

    // Perform the sync operation.
    let changelog = match sync_from_lock(
        sync_target,
        &environment,
        &extras,
        &groups,
        editable,
        install_options,
        modifications,
        python_platform.as_ref(),
        (&settings).into(),
        &client_builder,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        &concurrency,
        cache,
        workspace_cache,
        dry_run,
        printer,
        preview,
        MalwareCheckContext::from(&malware_settings),
    )
    .await
    {
        Ok(changelog) => changelog,
        Err(EnvironmentError::Install(error)) => {
            let error = *error;
            if let Some(changelog) = error.outdated_environment() {
                write_sync_report(
                    &target,
                    &environment,
                    changelog,
                    Some(lock_report),
                    dry_run,
                    output_format,
                    printer,
                )?;
            }
            return Err(UvError::from(error).into());
        }
        Err(err) => return Err(UvError::from(err).into()),
    };

    write_sync_report(
        &target,
        &environment,
        &changelog,
        Some(lock_report),
        dry_run,
        output_format,
        printer,
    )?;

    match outcome {
        Outcome::Success(..) | Outcome::Frozen(..) => Ok(ExitStatus::Success),
        Outcome::LockMismatch(prev, cur, lock_source) => {
            Err(UvError::user(LockError::LockMismatch(prev, cur, lock_source)).into())
        }
    }
}

/// The outcome of a `lock` operation within a `sync` operation.
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
enum Outcome<'a> {
    /// The `lock` operation was successful.
    Success(LockResult),
    /// A frozen lockfile was discovered without a lock operation.
    Frozen(&'a Lock),
    /// The `lock` operation successfully resolved, but failed due to a mismatch (e.g., with `--locked`).
    LockMismatch(Option<Box<Lock>>, Box<Lock>, LockedSource),
}

impl Outcome<'_> {
    /// Return the [`Lock`] associated with this outcome.
    fn lock(&self) -> &Lock {
        match self {
            Self::Success(lock) => match lock {
                LockResult::Changed(_, lock) => lock,
                LockResult::Unchanged(lock) => lock,
            },
            Self::Frozen(lock) => lock,
            Self::LockMismatch(_prev, cur, _lock_source) => cur,
        }
    }
}

fn identify_installation_target<'a>(
    target: &'a SyncTarget<'_>,
    lock: &'a Lock,
    all_packages: bool,
    package: &'a [PackageName],
) -> InstallTarget<'a> {
    match target {
        SyncTarget::Manifest(SyncManifest::Project(project)) => InstallTarget::from_project(
            project,
            lock,
            PackageSelection::from_args(all_packages, package, project.project_name()),
        ),
        SyncTarget::Lockfile {
            workspace,
            project_name,
            ..
        } => InstallTarget::Lockfile {
            root: workspace.root(),
            project_name: project_name.as_ref(),
            selection: PackageSelection::from_args(all_packages, package, project_name.as_ref()),
            lock,
        },
        SyncTarget::Manifest(SyncManifest::Script(script)) => {
            InstallTarget::Script { script, lock }
        }
    }
}

/// A sync reads an existing workspace lock or resolves a project or script manifest.
#[derive(Debug, Clone)]
enum SyncTarget<'a> {
    Manifest(&'a SyncManifest),
    Lockfile {
        workspace: &'a FrozenWorkspace,
        path: PathBuf,
        project_name: Option<PackageName>,
    },
}

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
enum SyncManifest {
    /// Sync a project environment.
    Project(VirtualProject),
    /// Sync a PEP 723 script environment.
    Script(Pep723Script),
}

impl<'a> From<&'a SyncManifest> for LockTarget<'a> {
    fn from(manifest: &'a SyncManifest) -> Self {
        match manifest {
            SyncManifest::Project(project) => Self::Workspace(project.workspace()),
            SyncManifest::Script(script) => Self::Script(script),
        }
    }
}

impl SyncTarget<'_> {
    fn project(&self) -> Option<&VirtualProject> {
        match self {
            Self::Manifest(SyncManifest::Project(project)) => Some(project),
            Self::Manifest(SyncManifest::Script(_)) | Self::Lockfile { .. } => None,
        }
    }

    fn script(&self) -> Option<&Pep723Script> {
        match self {
            Self::Manifest(SyncManifest::Project(_)) | Self::Lockfile { .. } => None,
            Self::Manifest(SyncManifest::Script(script)) => Some(script),
        }
    }

    /// Report the selected member's path, or the workspace root if no member is selected.
    fn project_report(&self) -> Option<ProjectReport> {
        match self {
            Self::Manifest(SyncManifest::Project(project)) => Some(ProjectReport::from(project)),
            Self::Lockfile {
                workspace,
                project_name,
                ..
            } => {
                let root = workspace.root();
                let path = workspace
                    .lock()
                    .workspace_member_paths()
                    .find(|(name, _)| Some(*name) == project_name.as_ref())
                    .map_or_else(|| root.to_path_buf(), |(_, path)| root.join(path));
                Some(ProjectReport {
                    path: uv_fs::normalize_path(&path).as_ref().into(),
                    workspace: WorkspaceReport { path: root.into() },
                })
            }
            Self::Manifest(SyncManifest::Script(_)) => None,
        }
    }
}

#[derive(Debug)]
enum SyncEnvironment {
    /// A Python environment for a project.
    Project(ProjectEnvironment),
    /// A Python environment for a script.
    Script(ScriptEnvironment),
}

impl SyncEnvironment {
    fn dry_run_target(&self) -> Option<&Path> {
        match self {
            Self::Project(env) => env.dry_run_target(),
            Self::Script(env) => env.dry_run_target(),
        }
    }
}

impl Deref for SyncEnvironment {
    type Target = PythonEnvironment;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Project(environment) => environment,
            Self::Script(environment) => environment,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct WorkspaceReport {
    /// The workspace directory path.
    path: PortablePathBuf,
}

impl From<&Workspace> for WorkspaceReport {
    fn from(workspace: &Workspace) -> Self {
        Self {
            path: workspace.install_path().as_path().into(),
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ProjectReport {
    //
    path: PortablePathBuf,
    workspace: WorkspaceReport,
}

impl From<&VirtualProject> for ProjectReport {
    fn from(project: &VirtualProject) -> Self {
        Self {
            path: project.root().into(),
            workspace: WorkspaceReport::from(project.workspace()),
        }
    }
}

impl From<&SyncTarget<'_>> for TargetName {
    fn from(target: &SyncTarget<'_>) -> Self {
        match target {
            SyncTarget::Manifest(SyncManifest::Project(_)) | SyncTarget::Lockfile { .. } => {
                Self::Project
            }
            SyncTarget::Manifest(SyncManifest::Script(_)) => Self::Script,
        }
    }
}

#[derive(Serialize, Debug)]
struct ScriptReport {
    /// The path to the script.
    path: PortablePathBuf,
}

impl From<&Pep723Script> for ScriptReport {
    fn from(script: &Pep723Script) -> Self {
        Self {
            path: script.path.as_path().into(),
        }
    }
}

/// A report of the uv sync operation
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct Report {
    /// The schema of this report.
    schema: SchemaReport,
    /// The target of the sync operation, either a project or a script.
    target: TargetName,
    /// The report for a [`TargetName::Project`], if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<ProjectReport>,
    /// The report for a [`TargetName::Script`], if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<ScriptReport>,
    /// The report for the sync operation.
    sync: SyncReport,
    /// The report for the lock operation.
    lock: Option<LockReport>,
    /// Whether this is a dry run.
    dry_run: bool,
}

/// The kind of target
#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum TargetName {
    Project,
    Script,
}

impl std::fmt::Display for TargetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Project => write!(f, "project"),
            Self::Script => write!(f, "script"),
        }
    }
}

/// Represents the action taken during a sync.
#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum SyncAction {
    /// The environment was checked and required no updates.
    Check,
    /// The environment was updated.
    Update,
    /// The environment was replaced.
    Replace,
    /// A new environment was created.
    Create,
}

impl From<&SyncEnvironment> for SyncAction {
    fn from(env: &SyncEnvironment) -> Self {
        match &env {
            SyncEnvironment::Project(ProjectEnvironment::Existing(..)) => Self::Check,
            SyncEnvironment::Project(ProjectEnvironment::Created(..)) => Self::Create,
            SyncEnvironment::Project(ProjectEnvironment::WouldCreate(..)) => Self::Create,
            SyncEnvironment::Project(ProjectEnvironment::WouldReplace(..)) => Self::Replace,
            SyncEnvironment::Project(ProjectEnvironment::Replaced(..)) => Self::Update,
            SyncEnvironment::Script(ScriptEnvironment::Existing(..)) => Self::Check,
            SyncEnvironment::Script(ScriptEnvironment::Created(..)) => Self::Create,
            SyncEnvironment::Script(ScriptEnvironment::WouldCreate(..)) => Self::Create,
            SyncEnvironment::Script(ScriptEnvironment::WouldReplace(..)) => Self::Replace,
            SyncEnvironment::Script(ScriptEnvironment::Replaced(..)) => Self::Update,
        }
    }
}

impl SyncAction {
    fn message(&self, target: TargetName, dry_run: bool) -> Option<&'static str> {
        let message = if dry_run {
            match self {
                Self::Check => "Would use",
                Self::Update => "Would update",
                Self::Replace => "Would replace",
                Self::Create => "Would create",
            }
        } else {
            // For projects, we omit some of these messages when we're not in dry-run mode
            let is_project = matches!(target, TargetName::Project);
            match self {
                Self::Check | Self::Update | Self::Create if is_project => {
                    return None;
                }
                Self::Check => "Using",
                Self::Update => "Updating",
                Self::Replace => "Replacing",
                Self::Create => "Creating",
            }
        };
        Some(message)
    }
}

/// Represents the action taken during a lock.
#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
enum LockAction {
    /// The lockfile was used without checking.
    Use,
    /// The lockfile was checked and required no updates.
    Check,
    /// The lockfile was updated.
    Update,
    /// A new lockfile was created.
    Create,
}

impl LockAction {
    fn message(&self, dry_run: bool) -> Option<&'static str> {
        let message = if dry_run {
            match self {
                Self::Use => return None,
                Self::Check => "Found up-to-date",
                Self::Update => "Would update",
                Self::Create => "Would create",
            }
        } else {
            return None;
        };
        Some(message)
    }
}

#[derive(Serialize, Debug)]
struct EnvironmentReport {
    /// The path to the environment.
    path: PortablePathBuf,
    /// The Python interpreter for the environment.
    python: PythonReport,
}

impl From<&PythonEnvironment> for EnvironmentReport {
    fn from(env: &PythonEnvironment) -> Self {
        Self {
            python: PythonReport::from(env.interpreter()),
            path: env.root().into(),
        }
    }
}

impl From<&SyncEnvironment> for EnvironmentReport {
    fn from(env: &SyncEnvironment) -> Self {
        let report = Self::from(&**env);
        // Replace the path if necessary; we construct a temporary virtual environment during dry
        // run invocations and want to report the path we _would_ use.
        if let Some(path) = env.dry_run_target() {
            report.with_path(path.into())
        } else {
            report
        }
    }
}

impl EnvironmentReport {
    /// Set the path for this environment report.
    #[must_use]
    fn with_path(mut self, path: PortablePathBuf) -> Self {
        if let Ok(python_path) = self.python.path().strip_prefix(self.path) {
            let new_path = path.as_ref().to_path_buf().join(python_path);
            self.python = self.python.with_path(new_path.as_path().into());
        }
        self.path = path;
        self
    }
}

/// The report for a sync operation.
#[derive(Serialize, Debug)]
struct SyncReport {
    /// The environment.
    environment: EnvironmentReport,
    /// The action performed during the sync, e.g., what was done to the environment.
    action: SyncAction,
    /// The packages that changed during the sync.
    #[serde(default)]
    changes: PackageChangesReport,

    // We store these fields so the report can format itself self-contained, but the outer
    // [`Report`] is intended to include these in user-facing output
    #[serde(skip)]
    dry_run: bool,
    #[serde(skip)]
    target: TargetName,
}

impl SyncReport {
    fn format(&self, output_format: SyncFormat) -> Option<String> {
        match output_format {
            // This is an intermediate report, when using JSON, it's only rendered at the end
            SyncFormat::Json => None,
            SyncFormat::Text => self.to_human_readable_string(),
        }
    }

    fn to_human_readable_string(&self) -> Option<String> {
        let Self {
            environment,
            action,
            changes: _,
            dry_run,
            target,
        } = self;

        let action = action.message(*target, *dry_run)?;

        let message = format!(
            "{action} {target} environment at: {path}",
            path = environment.path.user_display().cyan(),
        );
        if *dry_run {
            return Some(message.dimmed().to_string());
        }

        Some(message)
    }
}

/// The report for a lock operation.
#[derive(Debug, Serialize)]
struct LockReport {
    /// The path to the lockfile
    path: PortablePathBuf,
    /// Whether the lockfile was preserved, created, or updated.
    action: LockAction,

    // We store this field so the report can format itself self-contained, but the outer
    // [`Report`] is intended to include this in user-facing output
    #[serde(skip)]
    dry_run: bool,
}

impl From<(&LockTarget<'_>, &LockMode<'_>, &Outcome<'_>)> for LockReport {
    fn from((target, mode, outcome): (&LockTarget, &LockMode, &Outcome<'_>)) -> Self {
        Self {
            path: target.lock_path().deref().into(),
            action: match outcome {
                Outcome::Success(result) => {
                    match result {
                        LockResult::Unchanged(..) => match mode {
                            // When `--frozen` is used, we don't check the lockfile.
                            LockMode::Frozen(_) => LockAction::Use,
                            LockMode::DryRun(_) | LockMode::Locked(_, _) | LockMode::Write(_) => {
                                LockAction::Check
                            }
                        },
                        LockResult::Changed(None, ..) => LockAction::Create,
                        LockResult::Changed(Some(_), ..) => LockAction::Update,
                    }
                }
                Outcome::Frozen(_) => LockAction::Use,
                // TODO(zanieb): We don't have a way to report the outcome of the lock yet
                Outcome::LockMismatch(..) => LockAction::Check,
            },
            dry_run: matches!(mode, LockMode::DryRun(_)),
        }
    }
}

impl LockReport {
    fn format(&self, output_format: SyncFormat) -> Option<String> {
        match output_format {
            SyncFormat::Json => None,
            SyncFormat::Text => self.to_human_readable_string(),
        }
    }

    fn to_human_readable_string(&self) -> Option<String> {
        let Self {
            path,
            action,
            dry_run,
        } = self;

        let action = action.message(*dry_run)?;

        let message = format!(
            "{action} lockfile at: {path}",
            path = path.user_display().cyan(),
        );
        if *dry_run {
            return Some(message.dimmed().to_string());
        }

        Some(message)
    }
}

impl Report {
    fn format(&self, output_format: SyncFormat) -> Option<String> {
        match output_format {
            SyncFormat::Json => serde_json::to_string_pretty(self).ok(),
            SyncFormat::Text => None,
        }
    }
}

fn write_sync_report(
    target: &SyncTarget<'_>,
    environment: &SyncEnvironment,
    changelog: &Changelog,
    lock: Option<LockReport>,
    dry_run: DryRun,
    output_format: SyncFormat,
    printer: Printer,
) -> Result<()> {
    let report = Report {
        schema: SchemaReport::default(),
        target: TargetName::from(target),
        project: target.project_report(),
        script: target.script().map(ScriptReport::from),
        sync: SyncReport {
            environment: EnvironmentReport::from(environment),
            action: SyncAction::from(environment),
            changes: PackageChangesReport::from_changelog(changelog),
            dry_run: dry_run.enabled(),
            target: TargetName::from(target),
        },
        lock,
        dry_run: dry_run.enabled(),
    };

    if let Some(output) = report.format(output_format) {
        writeln!(printer.stdout_important(), "{output}")?;
    }

    Ok(())
}
