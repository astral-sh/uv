use std::fmt::Write;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_auth::UrlAuthPolicies;
use uv_cache::Cache;
use uv_client::{FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DependencyGroups, DependencyGroupsWithDefaults, DryRun, EditableMode,
    ExtrasSpecification, HashCheckingMode, InstallOptions, PreviewMode,
};
use uv_dispatch::BuildDispatch;
use uv_distribution_types::{
    DirectorySourceDist, Dist, Index, Resolution, ResolvedDist, SourceDist,
};
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep508::{MarkerTree, VersionOrUrl};
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitUrl, ParsedUrl};
use uv_python::{PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_resolver::{FlatIndex, Installable};
use uv_scripts::{Pep723ItemRef, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::pyproject::Source;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace, WorkspaceCache};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger, InstallLogger};
use crate::commands::pip::operations;
use crate::commands::pip::operations::Modifications;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockOperation, LockResult};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    default_dependency_groups, detect_conflicts, script_specification, update_environment,
    PlatformState, ProjectEnvironment, ProjectError, ScriptEnvironment, UniversalState,
};
use crate::commands::{diagnostics, ExitStatus};
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
    dev: DependencyGroups,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
    python: Option<String>,
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
    preview: PreviewMode,
) -> Result<ExitStatus> {
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
            && !project.workspace().pyproject_toml().is_package()
        {
            warn_user!("Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`");
        }

        SyncTarget::Project(project)
    };

    // Determine the default groups to include.
    let defaults = match &target {
        SyncTarget::Project(project) => default_dependency_groups(project.pyproject_toml())?,
        SyncTarget::Script(..) => Vec::new(),
    };

    // Discover or create the virtual environment.
    let environment = match &target {
        SyncTarget::Project(project) => SyncEnvironment::Project(
            ProjectEnvironment::get_or_init(
                project.workspace(),
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &network_settings,
                python_preference,
                python_downloads,
                no_config,
                active,
                cache,
                dry_run,
                printer,
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
                no_config,
                active,
                cache,
                dry_run,
                printer,
            )
            .await?,
        ),
    };

    // Notify the user of any environment changes.
    match &environment {
        SyncEnvironment::Project(ProjectEnvironment::Existing(environment))
            if dry_run.enabled() =>
        {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Discovered existing environment at: {}",
                    environment.root().user_display().bold()
                )
                .dimmed()
            )?;
        }
        SyncEnvironment::Project(ProjectEnvironment::WouldReplace(root, ..))
            if dry_run.enabled() =>
        {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Would replace existing virtual environment at: {}",
                    root.user_display().bold()
                )
                .dimmed()
            )?;
        }
        SyncEnvironment::Project(ProjectEnvironment::WouldCreate(root, ..))
            if dry_run.enabled() =>
        {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Would create virtual environment at: {}",
                    root.user_display().bold()
                )
                .dimmed()
            )?;
        }
        SyncEnvironment::Script(ScriptEnvironment::Existing(environment)) => {
            if dry_run.enabled() {
                writeln!(
                    printer.stderr(),
                    "{}",
                    format!(
                        "Discovered existing environment at: {}",
                        environment.root().user_display().bold()
                    )
                    .dimmed()
                )?;
            } else {
                writeln!(
                    printer.stderr(),
                    "Using script environment at: {}",
                    environment.root().user_display().cyan()
                )?;
            }
        }
        SyncEnvironment::Script(ScriptEnvironment::Replaced(environment)) if !dry_run.enabled() => {
            writeln!(
                printer.stderr(),
                "Recreating script environment at: {}",
                environment.root().user_display().cyan()
            )?;
        }
        SyncEnvironment::Script(ScriptEnvironment::Created(environment)) if !dry_run.enabled() => {
            writeln!(
                printer.stderr(),
                "Creating script environment at: {}",
                environment.root().user_display().cyan()
            )?;
        }
        SyncEnvironment::Script(ScriptEnvironment::WouldReplace(root, ..)) if dry_run.enabled() => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Would replace existing script environment at: {}",
                    root.user_display().bold()
                )
                .dimmed()
            )?;
        }
        SyncEnvironment::Script(ScriptEnvironment::WouldCreate(root, ..)) if dry_run.enabled() => {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Would create script environment at: {}",
                    root.user_display().bold()
                )
                .dimmed()
            )?;
        }
        _ => {}
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

            let spec = script_specification(Pep723ItemRef::Script(script), &settings.resolver)?
                .unwrap_or_default();
            match update_environment(
                Deref::deref(&environment).clone(),
                spec,
                modifications,
                &settings,
                &network_settings,
                &PlatformState::default(),
                Box::new(DefaultResolveLogger),
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
                Ok(..) => return Ok(ExitStatus::Success),
                Err(ProjectError::Operation(err)) => {
                    return diagnostics::OperationDiagnostic::native_tls(
                        network_settings.native_tls,
                    )
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
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

    let lock = match LockOperation::new(
        mode,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute(lock_target)
    .await
    {
        Ok(result) => {
            if dry_run.enabled() {
                match result {
                    LockResult::Unchanged(..) => {
                        writeln!(
                            printer.stderr(),
                            "{}",
                            format!(
                                "Found up-to-date lockfile at: {}",
                                lock_target.lock_path().user_display().bold()
                            )
                            .dimmed()
                        )?;
                    }
                    LockResult::Changed(None, ..) => {
                        writeln!(
                            printer.stderr(),
                            "{}",
                            format!(
                                "Would create lockfile at: {}",
                                lock_target.lock_path().user_display().bold()
                            )
                            .dimmed()
                        )?;
                    }
                    LockResult::Changed(Some(..), ..) => {
                        writeln!(
                            printer.stderr(),
                            "{}",
                            format!(
                                "Would update lockfile at: {}",
                                lock_target.lock_path().user_display().bold()
                            )
                            .dimmed()
                        )?;
                    }
                }
            }
            result.into_lock()
        }
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    };

    // Identify the installation target.
    let sync_target = match &target {
        SyncTarget::Project(project) => {
            match &project {
                VirtualProject::Project(project) => {
                    if all_packages {
                        InstallTarget::Workspace {
                            workspace: project.workspace(),
                            lock: &lock,
                        }
                    } else if let Some(package) = package.as_ref() {
                        InstallTarget::Project {
                            workspace: project.workspace(),
                            name: package,
                            lock: &lock,
                        }
                    } else {
                        // By default, install the root package.
                        InstallTarget::Project {
                            workspace: project.workspace(),
                            name: project.project_name(),
                            lock: &lock,
                        }
                    }
                }
                VirtualProject::NonProject(workspace) => {
                    if all_packages {
                        InstallTarget::NonProjectWorkspace {
                            workspace,
                            lock: &lock,
                        }
                    } else if let Some(package) = package.as_ref() {
                        InstallTarget::Project {
                            workspace,
                            name: package,
                            lock: &lock,
                        }
                    } else {
                        // By default, install the entire workspace.
                        InstallTarget::NonProjectWorkspace {
                            workspace,
                            lock: &lock,
                        }
                    }
                }
            }
        }
        SyncTarget::Script(script) => InstallTarget::Script {
            script,
            lock: &lock,
        },
    };

    let state = state.fork();

    // Perform the sync operation.
    match do_sync(
        sync_target,
        &environment,
        &extras,
        &dev.with_defaults(defaults),
        editable,
        install_options,
        modifications,
        (&settings).into(),
        &network_settings,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
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
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug, Clone)]
enum SyncTarget {
    /// Sync a project environment.
    Project(VirtualProject),
    /// Sync a PEP 723 script environment.
    Script(Pep723Script),
}

#[derive(Debug)]
enum SyncEnvironment {
    /// A Python environment for a project.
    Project(ProjectEnvironment),
    /// A Python environment for a script.
    Script(ScriptEnvironment),
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
    extras: &ExtrasSpecification,
    dev: &DependencyGroupsWithDefaults,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
    settings: InstallerSettingsRef<'_>,
    network_settings: &NetworkSettings,
    state: &PlatformState,
    logger: Box<dyn InstallLogger>,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    dry_run: DryRun,
    printer: Printer,
    preview: PreviewMode,
) -> Result<(), ProjectError> {
    // Extract the project settings.
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
    detect_conflicts(target.lock(), extras, dev)?;

    // Validate that the set of requested extras and development groups are defined in the lockfile.
    target.validate_extras(extras)?;
    target.validate_groups(dev)?;

    // Determine the markers to use for resolution.
    let marker_env = venv.interpreter().resolver_marker_environment();

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

    // Determine the tags to use for resolution.
    let tags = venv.interpreter().tags()?;

    // Read the lockfile.
    let resolution = target.to_resolution(
        &marker_env,
        tags,
        extras,
        dev,
        build_options,
        &install_options,
    )?;

    // Always skip virtual projects, which shouldn't be built or installed.
    let resolution = apply_no_virtual_project(resolution);

    // If necessary, convert editable to non-editable distributions.
    let resolution = apply_editable_mode(resolution, editable);

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

    // Populate credentials from the target.
    store_credentials_from_target(target);

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(network_settings.native_tls)
        .connectivity(network_settings.connectivity)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .url_auth_policies(UrlAuthPolicies::from(index_locations))
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
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

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();

    // Extract the hashes from the lockfile.
    let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

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
        venv.interpreter(),
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
        sources,
        WorkspaceCache::default(),
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
        &hasher,
        tags,
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

        !dist.r#virtual
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
                editable: true,
                r#virtual: false,
                url,
            })) = dist.as_ref()
            else {
                return None;
            };

            Some(ResolvedDist::Installable {
                dist: Arc::new(Dist::Source(SourceDist::Directory(DirectorySourceDist {
                    name: name.clone(),
                    install_path: install_path.clone(),
                    editable: false,
                    r#virtual: false,
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
    // Iterate over any idnexes in the target.
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
