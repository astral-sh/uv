use std::fmt::Write;
use std::path::Path;

use anstream::print;
use anyhow::{Error, Result, bail};
use futures::StreamExt;

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, TargetTriple, TreeFormat,
};
use uv_dispatch::UniversalState;
use uv_distribution_types::IndexCapabilities;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::{
    EnvironmentError, ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter,
};
use uv_lock::{PackageMap, TreeDisplay, TreeJsonTarget};
use uv_lock_operations::{DiscoveredProject, FrozenWorkspace, LockMode, LockOperation, LockTarget};
use uv_normalize::{DefaultGroups, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::ScriptInterpreter;
use uv_python_types::{
    PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest, PythonVersion,
};
use uv_resolve_operations::latest::LatestClient;
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_resolve_operations::resolution_markers;
use uv_scripts::Pep723Script;
use uv_settings::{FrozenSource, LockCheck, PythonInstallMirrors, ResolverSettings};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, WorkspaceCache};

use uv_resolve_operations::reporters::LatestVersionReporter;

/// A tree reads an existing workspace lock or resolves a project or script manifest.
#[derive(Clone, Copy)]
enum TreeSource<'a> {
    Manifest(LockTarget<'a>),
    Lockfile(&'a FrozenWorkspace),
}

/// Display the dependency tree for a project, script, or frozen workspace.
#[expect(clippy::fn_params_excessive_bools)]
pub async fn tree(
    project_dir: &Path,
    groups: DependencyGroups,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    universal: bool,
    format: TreeFormat,
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    outdated: bool,
    show_sizes: bool,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: &BaseClientBuilder<'_>,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if matches!(format, TreeFormat::Json) && !preview.is_enabled(PreviewFeature::JsonOutput) {
        warn_user!(
            "The `--format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    // Find the project requirements.
    let project;
    let source = if let Some(script) = script.as_ref() {
        TreeSource::Manifest(LockTarget::Script(script))
    } else {
        project = DiscoveredProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            None,
            frozen,
            preview,
            cache,
            workspace_cache,
        )
        .await?;
        match &project {
            DiscoveredProject::Manifest(project) => {
                TreeSource::Manifest(LockTarget::Workspace(project.workspace()))
            }
            DiscoveredProject::Lockfile(workspace) => {
                if outdated {
                    bail!("`--outdated` is not supported without a `pyproject.toml`");
                }
                TreeSource::Lockfile(workspace)
            }
        }
    };

    // Determine the groups to include.
    let groups = match source {
        TreeSource::Manifest(LockTarget::Workspace(workspace)) => {
            groups.with_defaults(workspace.default_groups()?)
        }
        TreeSource::Manifest(LockTarget::Script(_)) => {
            groups.with_defaults(DefaultGroups::default())
        }
        TreeSource::Lockfile(workspace) => workspace
            .resolve_groups(&groups, workspace.lock().root().map(uv_lock::Package::name))?,
    };

    // Find an interpreter for the project, unless `--frozen` and `--universal` are both set.
    let interpreter = if frozen.is_some() && universal {
        None
    } else {
        Some(match source {
            TreeSource::Manifest(LockTarget::Script(script)) => ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                client_builder,
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
            TreeSource::Manifest(LockTarget::Workspace(workspace)) => {
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
                    client_builder,
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
            TreeSource::Lockfile(workspace) => {
                let root = workspace.root();
                let lock = workspace.lock();
                let discovery_dir = if project_dir.starts_with(root) {
                    project_dir
                } else {
                    root
                };

                let target = InstallTarget::Lockfile {
                    root,
                    project_name: lock.root().map(uv_lock::Package::name),
                    selection: PackageSelection::Workspace,
                    lock,
                };

                let project_python = ProjectPythonRequest::from_requirements(
                    python.as_deref().map(PythonRequest::parse),
                    Some(root),
                    Some(target.python_requirement(&groups)?),
                    discovery_dir,
                    config_discovery,
                )
                .await
                .map_err(EnvironmentError::from)?;
                ProjectInterpreter::discover(
                    ProjectEnvironmentTarget::Lockfile { root, lock },
                    project_python,
                    client_builder,
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
        })
    };

    // Update the lockfile, if necessary.
    let resolved_lock;
    let lock = match source {
        TreeSource::Lockfile(workspace) => workspace.lock(),
        TreeSource::Manifest(target) => {
            let mode = if let Some(frozen_source) = frozen {
                LockMode::Frozen(frozen_source.into())
            } else if let LockCheck::Enabled(lock_check) = lock_check {
                LockMode::Locked(interpreter.as_ref().unwrap(), lock_check)
            } else if matches!(target, LockTarget::Script(_)) && !target.lock_path().is_file() {
                // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
                LockMode::DryRun(interpreter.as_ref().unwrap())
            } else {
                LockMode::Write(interpreter.as_ref().unwrap())
            };
            let state = UniversalState::default();
            resolved_lock = match Box::pin(
                LockOperation::new(
                    mode,
                    &settings,
                    client_builder,
                    &state,
                    Box::new(DefaultResolveLogger),
                    &concurrency,
                    cache,
                    workspace_cache,
                    printer,
                    preview,
                )
                .execute(target),
            )
            .await
            {
                Ok(result) => result.into_lock(),
                Err(err) => return Err(UvError::from(err).into()),
            };
            &resolved_lock
        }
    };

    // Determine the markers to use for resolution.
    let markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    // If necessary, look up the latest version of each package.
    let latest = if let TreeSource::Manifest(target) = source
        && outdated
    {
        let install_path = target.install_path();
        // Filter to packages that are derived from a registry.
        let packages = lock
            .packages()
            .iter()
            .filter_map(|package| {
                // TODO(charlie): We would need to know the format here.
                let index = match package.index(install_path) {
                    Ok(Some(index)) => index,
                    Ok(None) => return None,
                    Err(err) => return Some(Err(err)),
                };
                Some(Ok((package, index)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if packages.is_empty() {
            PackageMap::default()
        } else {
            let ResolverSettings {
                index_locations,
                index_strategy: _,
                keyring_provider,
                resolution: _,
                prerelease: _,
                fork_strategy: _,
                dependency_metadata: _,
                config_setting: _,
                config_settings_package: _,
                build_isolation: _,
                extra_build_dependencies: _,
                extra_build_variables: _,
                exclude_newer: _,
                link_mode: _,
                upgrade: _,
                build_options: _,
                sources: _,
                torch_backend: _,
                cuda_driver_version: _,
                amd_gpu_architecture: _,
            } = &settings;

            let capabilities = IndexCapabilities::default();

            // Initialize the registry client.
            let client = RegistryClientBuilder::new(
                client_builder.clone(),
                cache.clone().with_refresh(Refresh::All(Timestamp::now())),
            )
            .index_locations(index_locations.clone())
            .keyring(*keyring_provider)
            .build()?;
            let download_concurrency = concurrency.downloads_semaphore.clone();

            let exclude_newer = lock.exclude_newer();

            // Initialize the client to fetch the latest version of each package.
            let client = LatestClient {
                client: &client,
                capabilities: &capabilities,
                prerelease: lock.prerelease(),
                exclude_newer,
                index_locations,
                requires_python: Some(lock.requires_python()),
                tags: None,
            };

            let reporter = LatestVersionReporter::from(printer).with_length(packages.len() as u64);

            // Fetch the latest version for each package.
            let download_concurrency = &download_concurrency;
            let mut fetches = futures::stream::iter(packages)
                .map(async |(package, index)| {
                    // This probably already doesn't work for `--find-links`?
                    let Some(filename) = client
                        .find_latest(package.name(), Some(&index), download_concurrency)
                        .await?
                    else {
                        return Ok(None);
                    };
                    Ok::<Option<_>, Error>(Some((package, filename.into_version())))
                })
                .buffer_unordered(concurrency.downloads);

            let mut map = PackageMap::default();
            while let Some(entry) = fetches.next().await.transpose()? {
                let Some((package, version)) = entry else {
                    reporter.on_fetch_progress();
                    continue;
                };
                reporter.on_fetch_version(package.name(), &version);
                if package.version().is_some_and(|package| version > *package) {
                    map.insert(package.clone(), version);
                }
            }
            reporter.on_fetch_complete();
            map
        }
    } else {
        PackageMap::default()
    };

    // Render the tree.
    let tree = TreeDisplay::new(
        lock,
        markers.as_ref(),
        &latest,
        depth.into(),
        &prune,
        &package,
        &groups,
        no_dedupe,
        invert,
        show_sizes,
    );

    match format {
        TreeFormat::Text => print!("{tree}"),
        TreeFormat::Json => writeln!(
            printer.stdout_important(),
            "{}",
            tree.to_json(match source {
                TreeSource::Manifest(LockTarget::Workspace(workspace)) => {
                    TreeJsonTarget::Workspace(workspace.install_path())
                }
                TreeSource::Manifest(LockTarget::Script(script)) =>
                    TreeJsonTarget::Script(&script.path),
                TreeSource::Lockfile(workspace) => TreeJsonTarget::Workspace(workspace.root()),
            })?
        )?,
    }

    Ok(ExitStatus::Success)
}
