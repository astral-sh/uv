use std::fmt::Write;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use serde::Serialize;
use tracing::warn;
use uv_cache::Cache;
use uv_cli::SyncFormat;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DependencyGroups, DependencyGroupsWithDefaults, DryRun, EditableMode,
    ExtrasSpecification, ExtrasSpecificationWithDefaults, HashCheckingMode, InstallOptions,
    Preview, PreviewFeatures, TargetTriple,
};
use uv_dispatch::BuildDispatch;
use uv_distribution_types::{
    DirectorySourceDist, Dist, Index, Requirement, Resolution, ResolvedDist, SourceDist,
};
use uv_fs::{PortablePathBuf, Simplified};
use uv_installer::SitePackages;
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_pep508::{MarkerTree, VersionOrUrl};
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitUrl, ParsedUrl};
use uv_python::{PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_resolver::{FlatIndex, Installable, Lock};
use uv_scripts::{Pep723ItemRef, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::pyproject::Source;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace, WorkspaceCache};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger, InstallLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::resolution_markers;
use crate::commands::pip::{operations, resolution_tags};
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockOperation, LockResult};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    PlatformState, ProjectEnvironment, ProjectError, ScriptEnvironment, UniversalState,
    default_dependency_groups, detect_conflicts, script_specification, update_environment,
};
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, NetworkSettings, ResolverInstallerSettings};

/// Sync the project environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn sync(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    dry_run: DryRun,
    active: Option<bool>,
    all_packages: bool,
    package: Option<PackageName>,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    script: Option<Pep723Script>,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
    output_format: SyncFormat,
) -> Result<ExitStatus> {
    if preview.is_enabled(PreviewFeatures::JSON_OUTPUT) && matches!(output_format, SyncFormat::Json)
    {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::JSON_OUTPUT
        );
    }

    // Identify the target.
    let workspace_cache = WorkspaceCache::default();
    let target = if let Some(script) = script {
        SyncTarget::Script(script)
    } else {
        // Identify the project.
        let project = if frozen {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions {
                    members: MemberDiscovery::None,
                    ..DiscoveryOptions::default()
                },
                &workspace_cache,
            )
            .await?
        } else if let Some(package) = package.as_ref() {
            VirtualProject::Project(
                Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
                .await?
        };

        // TODO(lucab): improve warning content
        // <https://github.com/astral-sh/uv/issues/7428>
        if project.workspace().pyproject_toml().has_scripts()
            && !project.workspace().pyproject_toml().is_package(true)
        {
            warn_user!(
                "Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`"
            );
        }

        SyncTarget::Project(project)
    };

    // Determine the groups and extras to include.
    let default_groups = match &target {
        SyncTarget::Project(project) => default_dependency_groups(project.pyproject_toml())?,
        SyncTarget::Script(..) => DefaultGroups::default(),
    };
    let default_extras = match &target {
        SyncTarget::Project(_project) => DefaultExtras::default(),
        SyncTarget::Script(..) => DefaultExtras::default(),
    };
    let groups = groups.with_defaults(default_groups);
    let extras = extras.with_defaults(default_extras);

    // Discover or create the virtual environment.
    let environment = match &target {
        SyncTarget::Project(project) => SyncEnvironment::Project(
            ProjectEnvironment::get_or_init(
                project.workspace(),
                &groups,
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &network_settings,
                python_preference,
                python_downloads,
                false,
                no_config,
                active,
                cache,
                dry_run,
                printer,
                preview,
            )
            .await?,
        ),
        SyncTarget::Script(script) => SyncEnvironment::Script(
            ScriptEnvironment::get_or_init(
                Pep723ItemRef::Script(script),
                python.as_deref().map(PythonRequest::parse),
                &network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                no_config,
                active,
                cache,
                dry_run,
                printer,
                preview,
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
    };

    // Show the intermediate results if relevant
    if let Some(message) = sync_report.format(output_format) {
        writeln!(printer.stderr(), "{message}")?;
    }

    // Special-case: we're syncing a script that doesn't have an associated lockfile. In that case,
    // we don't create a lockfile, so the resolve-and-install semantics are different.
    if let SyncTarget::Script(script) = &target {
        let lockfile = LockTarget::from(script).lock_path();
        if !lockfile.is_file() {
            if frozen {
                return Err(anyhow::anyhow!(
                    "`uv sync --frozen` requires a script lockfile; run `{}` to lock the script",
                    format!("uv lock --script {}", script.path.user_display()).green(),
                ));
            }

            if locked {
                return Err(anyhow::anyhow!(
                    "`uv sync --locked` requires a script lockfile; run `{}` to lock the script",
                    format!("uv lock --script {}", script.path.user_display()).green(),
                ));
            }

            // Parse the requirements from the script.
            let spec = script_specification(Pep723ItemRef::Script(script), &settings.resolver)?
                .unwrap_or_default();

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
                    Constraints::from_requirements(
                        constraints
                            .iter()
                            .map(|constraint| Requirement::from(constraint.clone())),
                    )
                });

            match update_environment(
                Deref::deref(&environment).clone(),
                spec,
                modifications,
                build_constraints.unwrap_or_default(),
                &settings,
                &network_settings,
                &PlatformState::default(),
                Box::new(DefaultResolveLogger),
                Box::new(DefaultInstallLogger),
                installer_metadata,
                concurrency,
                cache,
                workspace_cache.clone(),
                dry_run,
                printer,
                preview,
            )
            .await
            {
                Ok(..) => {
                    // Generate a report for the script without a lockfile
                    let report = Report {
                        schema: SchemaReport::default(),
                        target: TargetName::from(&target),
                        project: None,
                        script: Some(ScriptReport::from(script)),
                        sync: sync_report,
                        lock: None,
                        dry_run: dry_run.enabled(),
                    };
                    if let Some(output) = report.format(output_format) {
                        writeln!(printer.stdout_important(), "{output}")?;
                    }
                    return Ok(ExitStatus::Success);
                }
                // TODO(zanieb): We should respect `--output-format json` for the error case
                Err(ProjectError::Operation(err)) => {
                    return diagnostics::OperationDiagnostic::native_tls(
                        network_settings.native_tls,
                    )
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    // Initialize any shared state.
    let state = UniversalState::default();

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(environment.interpreter())
    } else if dry_run.enabled() {
        LockMode::DryRun(environment.interpreter())
    } else {
        LockMode::Write(environment.interpreter())
    };

    let lock_target = match &target {
        SyncTarget::Project(project) => LockTarget::from(project.workspace()),
        SyncTarget::Script(script) => LockTarget::from(script),
    };

    let outcome = match LockOperation::new(
        mode,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        &workspace_cache,
        printer,
        preview,
    )
    .execute(lock_target)
    .await
    {
        Ok(result) => Outcome::Success(result),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(ProjectError::LockMismatch(prev, cur)) => {
            if dry_run.enabled() {
                // The lockfile is mismatched, but we're in dry-run mode. We should proceed with the
                // sync operation, but exit with a non-zero status.
                Outcome::LockMismatch(prev, cur)
            } else {
                writeln!(
                    printer.stderr(),
                    "{}",
                    ProjectError::LockMismatch(prev, cur).to_string().bold()
                )?;
                return Ok(ExitStatus::Failure);
            }
        }
        Err(err) => return Err(err.into()),
    };

    let lock_report = LockReport::from((&lock_target, &mode, &outcome));
    if let Some(message) = lock_report.format(output_format) {
        writeln!(printer.stderr(), "{message}")?;
    }

    let report = Report {
        schema: SchemaReport::default(),
        target: TargetName::from(&target),
        project: target.project().map(ProjectReport::from),
        script: target.script().map(ScriptReport::from),
        sync: sync_report,
        lock: Some(lock_report),
        dry_run: dry_run.enabled(),
    };

    if let Some(output) = report.format(output_format) {
        writeln!(printer.stdout_important(), "{output}")?;
    }

    // Identify the installation target.
    let sync_target =
        identify_installation_target(&target, outcome.lock(), all_packages, package.as_ref());

    let state = state.fork();

    // Perform the sync operation.
    match do_sync(
        sync_target,
        &environment,
        &extras,
        &groups,
        editable,
        install_options,
        modifications,
        python_platform.as_ref(),
        (&settings).into(),
        &network_settings,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        workspace_cache,
        dry_run,
        printer,
        preview,
    )
    .await
    {
        Ok(()) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    }

    match outcome {
        Outcome::Success(..) => Ok(ExitStatus::Success),
        Outcome::LockMismatch(prev, cur) => {
            writeln!(
                printer.stderr(),
                "{}",
                ProjectError::LockMismatch(prev, cur).to_string().bold()
            )?;
            Ok(ExitStatus::Failure)
        }
    }
}

/// The outcome of a `lock` operation within a `sync` operation.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Outcome {
    /// The `lock` operation was successful.
    Success(LockResult),
    /// The `lock` operation successfully resolved, but failed due to a mismatch (e.g., with `--locked`).
    LockMismatch(Option<Box<Lock>>, Box<Lock>),
}

impl Outcome {
    /// Return the [`Lock`] associated with this outcome.
    fn lock(&self) -> &Lock {
        match self {
            Self::Success(lock) => match lock {
                LockResult::Changed(_, lock) => lock,
                LockResult::Unchanged(lock) => lock,
            },
            Self::LockMismatch(_prev, cur) => cur,
        }
    }
}

fn identify_installation_target<'a>(
    target: &'a SyncTarget,
    lock: &'a Lock,
    all_packages: bool,
    package: Option<&'a PackageName>,
) -> InstallTarget<'a> {
    match &target {
        SyncTarget::Project(project) => {
            match &project {
                VirtualProject::Project(project) => {
                    if all_packages {
                        InstallTarget::Workspace {
                            workspace: project.workspace(),
                            lock,
                        }
                    } else if let Some(package) = package {
                        InstallTarget::Project {
                            workspace: project.workspace(),
                            name: package,
                            lock,
                        }
                    } else {
                        // By default, install the root package.
                        InstallTarget::Project {
                            workspace: project.workspace(),
                            name: project.project_name(),
                            lock,
                        }
                    }
                }
                VirtualProject::NonProject(workspace) => {
                    if all_packages {
                        InstallTarget::NonProjectWorkspace { workspace, lock }
                    } else if let Some(package) = package {
                        InstallTarget::Project {
                            workspace,
                            name: package,
                            lock,
                        }
                    } else {
                        // By default, install the entire workspace.
                        InstallTarget::NonProjectWorkspace { workspace, lock }
                    }
                }
            }
        }
        SyncTarget::Script(script) => InstallTarget::Script { script, lock },
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum SyncTarget {
    /// Sync a project environment.
    Project(VirtualProject),
    /// Sync a PEP 723 script environment.
    Script(Pep723Script),
}

impl SyncTarget {
    fn project(&self) -> Option<&VirtualProject> {
        match self {
            Self::Project(project) => Some(project),
            Self::Script(_) => None,
        }
    }

    fn script(&self) -> Option<&Pep723Script> {
        match self {
            Self::Project(_) => None,
            Self::Script(script) => Some(script),
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
            Self::Project(environment) => Deref::deref(environment),
            Self::Script(environment) => Deref::deref(environment),
        }
    }
}

/// Sync a lockfile with an environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(super) async fn do_sync(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
    python_platform: Option<&TargetTriple>,
    settings: InstallerSettingsRef<'_>,
    network_settings: &NetworkSettings,
    state: &PlatformState,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    workspace_cache: WorkspaceCache,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
) -> Result<(), ProjectError> {
    // Extract the project settings.
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        dependency_metadata,
        config_setting,
        config_settings_package,
        no_build_isolation,
        no_build_isolation_package,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
        sources,
    } = settings;

    let client_builder = BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .keyring(keyring_provider)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // Validate that the Python version is supported by the lockfile.
    if !target
        .lock()
        .requires_python()
        .contains(venv.interpreter().python_version())
    {
        return Err(ProjectError::LockedPythonIncompatibility(
            venv.interpreter().python_version().clone(),
            target.lock().requires_python().clone(),
        ));
    }

    // Validate that the set of requested extras and development groups are compatible.
    detect_conflicts(target.lock(), extras, groups)?;

    // Validate that the set of requested extras and development groups are defined in the lockfile.
    target.validate_extras(extras)?;
    target.validate_groups(groups)?;

    // Determine the markers to use for resolution.
    let marker_env = resolution_markers(None, python_platform, venv.interpreter());

    // Validate that the platform is supported by the lockfile.
    let environments = target.lock().supported_environments();
    if !environments.is_empty() {
        if !environments
            .iter()
            .any(|env| env.evaluate(&marker_env, &[]))
        {
            return Err(ProjectError::LockedPlatformIncompatibility(
                // For error reporting, we use the "simplified"
                // supported environments, because these correspond to
                // what the end user actually wrote. The non-simplified
                // environments, by contrast, are explicitly
                // constrained by `requires-python`.
                target
                    .lock()
                    .simplified_supported_environments()
                    .into_iter()
                    .filter_map(MarkerTree::contents)
                    .map(|env| format!("`{env}`"))
                    .join(", "),
            ));
        }
    }

    // Determine the tags to use for the resolution.
    let tags = resolution_tags(None, python_platform, venv.interpreter())?;

    // Read the lockfile.
    let resolution = target.to_resolution(
        &marker_env,
        &tags,
        extras,
        groups,
        build_options,
        &install_options,
    )?;

    // Always skip virtual projects, which shouldn't be built or installed.
    let resolution = apply_no_virtual_project(resolution);

    // If necessary, convert editable to non-editable distributions.
    let resolution = apply_editable_mode(resolution, editable);

    index_locations.cache_index_credentials();

    // Populate credentials from the target.
    store_credentials_from_target(target);

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .cache(cache.clone())
        .index_locations(index_locations)
        .index_strategy(index_strategy)
        .markers(venv.interpreter().markers())
        .platform(venv.interpreter().platform())
        .build();

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(venv)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(venv, no_build_isolation_package)
    };

    // Read the build constraints from the lockfile.
    let build_constraints = target.build_constraints();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_hasher = HashStrategy::default();

    // Extract the hashes from the lockfile.
    let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), cache);
        let entries = client
            .fetch_all(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        build_constraints,
        venv.interpreter(),
        index_locations,
        &flat_index,
        dependency_metadata,
        state.clone().into_inner(),
        index_strategy,
        config_setting,
        config_settings_package,
        build_isolation,
        link_mode,
        build_options,
        &build_hasher,
        exclude_newer,
        sources,
        workspace_cache.clone(),
        concurrency,
        preview,
    );

    let site_packages = SitePackages::from_environment(venv)?;

    // Sync the environment.
    operations::install(
        &resolution,
        site_packages,
        modifications,
        reinstall,
        build_options,
        link_mode,
        compile_bytecode,
        index_locations,
        config_setting,
        config_settings_package,
        &hasher,
        &tags,
        &client,
        state.in_flight(),
        concurrency,
        &build_dispatch,
        cache,
        venv,
        logger,
        installer_metadata,
        dry_run,
        printer,
    )
    .await?;

    Ok(())
}

/// Filter out any virtual workspace members.
fn apply_no_virtual_project(resolution: Resolution) -> Resolution {
    resolution.filter(|dist| {
        let ResolvedDist::Installable { dist, .. } = dist else {
            return true;
        };

        let Dist::Source(dist) = dist.as_ref() else {
            return true;
        };

        let SourceDist::Directory(dist) = dist else {
            return true;
        };

        !dist.r#virtual.unwrap_or(false)
    })
}

/// If necessary, convert any editable requirements to non-editable.
fn apply_editable_mode(resolution: Resolution, editable: EditableMode) -> Resolution {
    match editable {
        // No modifications are necessary for editable mode; retain any editable distributions.
        EditableMode::Editable => resolution,

        // Filter out any editable distributions.
        EditableMode::NonEditable => resolution.map(|dist| {
            let ResolvedDist::Installable { dist, version } = dist else {
                return None;
            };
            let Dist::Source(SourceDist::Directory(DirectorySourceDist {
                name,
                install_path,
                editable: Some(true),
                r#virtual,
                url,
            })) = dist.as_ref()
            else {
                return None;
            };

            Some(ResolvedDist::Installable {
                dist: Arc::new(Dist::Source(SourceDist::Directory(DirectorySourceDist {
                    name: name.clone(),
                    install_path: install_path.clone(),
                    editable: Some(false),
                    r#virtual: *r#virtual,
                    url: url.clone(),
                }))),
                version: version.clone(),
            })
        }),
    }
}

/// Extract any credentials that are defined on the workspace dependencies themselves. While we
/// don't store plaintext credentials in the `uv.lock`, we do respect credentials that are defined
/// in the `pyproject.toml`.
///
/// These credentials can come from any of `tool.uv.sources`, `tool.uv.dev-dependencies`,
/// `project.dependencies`, and `project.optional-dependencies`.
fn store_credentials_from_target(target: InstallTarget<'_>) {
    // Iterate over any indexes in the target.
    for index in target.indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Iterate over any sources in the target.
    for source in target.sources() {
        match source {
            Source::Git { git, .. } => {
                uv_git::store_credentials_from_url(git);
            }
            Source::Url { url, .. } => {
                uv_auth::store_credentials_from_url(url);
            }
            _ => {}
        }
    }

    // Iterate over any dependencies defined in the target.
    for requirement in target.requirements() {
        let Some(VersionOrUrl::Url(url)) = &requirement.version_or_url else {
            continue;
        };
        match &url.parsed_url {
            ParsedUrl::Git(ParsedGitUrl { url, .. }) => {
                uv_git::store_credentials_from_url(url.repository());
            }
            ParsedUrl::Archive(ParsedArchiveUrl { url, .. }) => {
                uv_auth::store_credentials_from_url(url);
            }
            _ => {}
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

impl From<&SyncTarget> for TargetName {
    fn from(target: &SyncTarget) -> Self {
        match target {
            SyncTarget::Project(_) => TargetName::Project,
            SyncTarget::Script(_) => TargetName::Script,
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

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    /// An unstable, experimental schema.
    #[default]
    Preview,
}

#[derive(Serialize, Debug, Default)]
struct SchemaReport {
    /// The version of the schema.
    version: SchemaVersion,
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
            TargetName::Project => write!(f, "project"),
            TargetName::Script => write!(f, "script"),
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
            SyncEnvironment::Project(ProjectEnvironment::Existing(..)) => SyncAction::Check,
            SyncEnvironment::Project(ProjectEnvironment::Created(..)) => SyncAction::Create,
            SyncEnvironment::Project(ProjectEnvironment::WouldCreate(..)) => SyncAction::Create,
            SyncEnvironment::Project(ProjectEnvironment::WouldReplace(..)) => SyncAction::Replace,
            SyncEnvironment::Project(ProjectEnvironment::Replaced(..)) => SyncAction::Update,
            SyncEnvironment::Script(ScriptEnvironment::Existing(..)) => SyncAction::Check,
            SyncEnvironment::Script(ScriptEnvironment::Created(..)) => SyncAction::Create,
            SyncEnvironment::Script(ScriptEnvironment::WouldCreate(..)) => SyncAction::Create,
            SyncEnvironment::Script(ScriptEnvironment::WouldReplace(..)) => SyncAction::Replace,
            SyncEnvironment::Script(ScriptEnvironment::Replaced(..)) => SyncAction::Update,
        }
    }
}

impl SyncAction {
    fn message(&self, target: TargetName, dry_run: bool) -> Option<&'static str> {
        let message = if dry_run {
            match self {
                SyncAction::Check => "Would use",
                SyncAction::Update => "Would update",
                SyncAction::Replace => "Would replace",
                SyncAction::Create => "Would create",
            }
        } else {
            // For projects, we omit some of these messages when we're not in dry-run mode
            let is_project = matches!(target, TargetName::Project);
            match self {
                SyncAction::Check | SyncAction::Update | SyncAction::Create if is_project => {
                    return None;
                }
                SyncAction::Check => "Using",
                SyncAction::Update => "Updating",
                SyncAction::Replace => "Replacing",
                SyncAction::Create => "Creating",
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
                LockAction::Use => return None,
                LockAction::Check => "Found up-to-date",
                LockAction::Update => "Would update",
                LockAction::Create => "Would create",
            }
        } else {
            return None;
        };
        Some(message)
    }
}

#[derive(Serialize, Debug)]
struct PythonReport {
    path: PortablePathBuf,
    version: uv_pep508::StringVersion,
    implementation: String,
}

impl From<&uv_python::Interpreter> for PythonReport {
    fn from(interpreter: &uv_python::Interpreter) -> Self {
        Self {
            path: interpreter.sys_executable().into(),
            version: interpreter.python_full_version().clone(),
            implementation: interpreter.implementation_name().to_string(),
        }
    }
}

impl PythonReport {
    /// Set the path for this Python report.
    #[must_use]
    fn with_path(mut self, path: PortablePathBuf) -> Self {
        self.path = path;
        self
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
        let report = EnvironmentReport::from(&**env);
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
        let python_path = &self.python.path;
        if let Ok(python_path) = python_path.as_ref().strip_prefix(self.path) {
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

impl From<(&LockTarget<'_>, &LockMode<'_>, &Outcome)> for LockReport {
    fn from((target, mode, outcome): (&LockTarget, &LockMode, &Outcome)) -> Self {
        Self {
            path: target.lock_path().deref().into(),
            action: match outcome {
                Outcome::Success(result) => {
                    match result {
                        LockResult::Unchanged(..) => match mode {
                            // When `--frozen` is used, we don't check the lockfile
                            LockMode::Frozen => LockAction::Use,
                            LockMode::DryRun(_) | LockMode::Locked(_) | LockMode::Write(_) => {
                                LockAction::Check
                            }
                        },
                        LockResult::Changed(None, ..) => LockAction::Create,
                        LockResult::Changed(Some(_), ..) => LockAction::Update,
                    }
                }
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
