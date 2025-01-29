use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use itertools::Itertools;

use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DevGroupsManifest, DevGroupsSpecification, EditableMode,
    ExtrasSpecification, HashCheckingMode, InstallOptions, LowerBound, PreviewMode, TrustedHost,
};
use uv_dispatch::BuildDispatch;
use uv_distribution_types::{
    DirectorySourceDist, Dist, Index, Resolution, ResolvedDist, SourceDist,
};
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep508::{MarkerTree, VersionOrUrl};
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitUrl, ParsedUrl};
use uv_python::{PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest};
use uv_resolver::{FlatIndex, Installable};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::pyproject::Source;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger, InstallLogger};
use crate::commands::pip::operations;
use crate::commands::pip::operations::Modifications;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{do_safe_lock, LockMode};
use crate::commands::project::{
    default_dependency_groups, detect_conflicts, DependencyGroupsTarget, PlatformState,
    ProjectError, UniversalState,
};
use crate::commands::{diagnostics, project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverInstallerSettings};

/// Sync the project environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn sync(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    all_packages: bool,
    package: Option<PackageName>,
    extras: ExtrasSpecification,
    dev: DevGroupsSpecification,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    settings: ResolverInstallerSettings,
    installer_metadata: bool,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Identify the project.
    let project = if frozen {
        VirtualProject::discover(
            project_dir,
            &DiscoveryOptions {
                members: MemberDiscovery::None,
                ..DiscoveryOptions::default()
            },
        )
        .await?
    } else if let Some(package) = package.as_ref() {
        VirtualProject::Project(
            Workspace::discover(project_dir, &DiscoveryOptions::default())
                .await?
                .with_current_project(package.clone())
                .with_context(|| format!("Package `{package}` not found in workspace"))?,
        )
    } else {
        VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await?
    };

    // Validate that any referenced dependency groups are defined in the workspace.
    if !frozen {
        let target = match &project {
            VirtualProject::Project(project) => {
                if all_packages {
                    DependencyGroupsTarget::Workspace(project.workspace())
                } else {
                    DependencyGroupsTarget::Project(project)
                }
            }
            VirtualProject::NonProject(workspace) => DependencyGroupsTarget::Workspace(workspace),
        };
        target.validate(&dev)?;
    }

    // Determine the default groups to include.
    let defaults = default_dependency_groups(project.pyproject_toml())?;

    // TODO(lucab): improve warning content
    // <https://github.com/astral-sh/uv/issues/7428>
    if project.workspace().pyproject_toml().has_scripts()
        && !project.workspace().pyproject_toml().is_package()
    {
        warn_user!("Skipping installation of entry points (`project.scripts`) because this project is not packaged; to install entry points, set `tool.uv.package = true` or define a `build-system`");
    }

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        &install_mirrors,
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        no_config,
        cache,
        printer,
    )
    .await?;

    // Initialize any shared state.
    let state = UniversalState::default();

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(venv.interpreter())
    } else {
        LockMode::Write(venv.interpreter())
    };

    let lock = match do_safe_lock(
        mode,
        project.workspace().into(),
        settings.as_ref().into(),
        LowerBound::Warn,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
        preview,
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    };

    // Identify the installation target.
    let target = match &project {
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
    };

    let state = state.fork();

    // Perform the sync operation.
    match do_sync(
        target,
        &venv,
        &extras,
        &dev.with_defaults(defaults),
        editable,
        install_options,
        modifications,
        settings.as_ref().into(),
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
        preview,
    )
    .await
    {
        Ok(()) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    }

    Ok(ExitStatus::Success)
}

/// Sync a lockfile with an environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(super) async fn do_sync(
    target: InstallTarget<'_>,
    venv: &PythonEnvironment,
    extras: &ExtrasSpecification,
    dev: &DevGroupsManifest,
    editable: EditableMode,
    install_options: InstallOptions,
    modifications: Modifications,
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
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .allow_insecure_host(allow_insecure_host.to_vec())
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
    let bounds = LowerBound::default();
    let build_constraints = Constraints::default();
    let build_hasher = HashStrategy::default();
    let dry_run = false;

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
        bounds,
        sources,
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

        let Dist::Source(dist) = dist else {
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
            let ResolvedDist::Installable {
                dist:
                    Dist::Source(SourceDist::Directory(DirectorySourceDist {
                        name,
                        install_path,
                        editable: true,
                        r#virtual: false,
                        url,
                    })),
                version,
            } = dist
            else {
                return None;
            };

            Some(ResolvedDist::Installable {
                dist: Dist::Source(SourceDist::Directory(DirectorySourceDist {
                    name: name.clone(),
                    install_path: install_path.clone(),
                    editable: false,
                    r#virtual: false,
                    url: url.clone(),
                })),
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
