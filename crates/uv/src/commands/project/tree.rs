use std::path::Path;

use anstream::print;
use anyhow::{Error, Result};
use futures::StreamExt;
use tokio::sync::Semaphore;
use uv_auth::UrlAuthPolicies;
use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::RegistryClientBuilder;
use uv_configuration::{Concurrency, DependencyGroups, PreviewMode, TargetTriple};
use uv_distribution_types::IndexCapabilities;
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest, PythonVersion};
use uv_resolver::{PackageMap, TreeDisplay};
use uv_scripts::{Pep723ItemRef, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    default_dependency_groups, ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState,
};
use crate::commands::reporters::LatestVersionReporter;
use crate::commands::{diagnostics, ExitStatus};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverSettings};

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
    project_dir: &Path,
    dev: DependencyGroups,
    locked: bool,
    frozen: bool,
    universal: bool,
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    outdated: bool,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    network_settings: &NetworkSettings,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace_cache = WorkspaceCache::default();
    let workspace;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        workspace =
            Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
                .await?;
        LockTarget::Workspace(&workspace)
    };

    // Determine the default groups to include.
    let defaults = match target {
        LockTarget::Workspace(workspace) => default_dependency_groups(workspace.pyproject_toml())?,
        LockTarget::Script(_) => vec![],
    };

    let native_tls = network_settings.native_tls;

    // Find an interpreter for the project, unless `--frozen` and `--universal` are both set.
    let interpreter = if frozen && universal {
        None
    } else {
        Some(match target {
            LockTarget::Script(script) => ScriptInterpreter::discover(
                Pep723ItemRef::Script(script),
                python.as_deref().map(PythonRequest::parse),
                network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => ProjectInterpreter::discover(
                workspace,
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        })
    };

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(interpreter.as_ref().unwrap())
    } else if matches!(target, LockTarget::Script(_)) && !target.lock_path().is_file() {
        // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
        LockMode::DryRun(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Update the lockfile, if necessary.
    let lock = match LockOperation::new(
        mode,
        &settings,
        network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute(target)
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

    // Determine the markers to use for resolution.
    let markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    // If necessary, look up the latest version of each package.
    let latest = if outdated {
        // Filter to packages that are derived from a registry.
        let packages = lock
            .packages()
            .iter()
            .filter_map(|package| {
                let index = match package.index(target.install_path()) {
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
                no_build_isolation: _,
                no_build_isolation_package: _,
                exclude_newer: _,
                link_mode: _,
                upgrade: _,
                build_options: _,
                sources: _,
            } = &settings;

            let capabilities = IndexCapabilities::default();

            // Initialize the registry client.
            let client = RegistryClientBuilder::new(
                cache.clone().with_refresh(Refresh::All(Timestamp::now())),
            )
            .native_tls(network_settings.native_tls)
            .connectivity(network_settings.connectivity)
            .allow_insecure_host(network_settings.allow_insecure_host.clone())
            .url_auth_policies(UrlAuthPolicies::from(index_locations))
            .keyring(*keyring_provider)
            .build();
            let download_concurrency = Semaphore::new(concurrency.downloads);

            // Initialize the client to fetch the latest version of each package.
            let client = LatestClient {
                client: &client,
                capabilities: &capabilities,
                prerelease: lock.prerelease_mode(),
                exclude_newer: lock.exclude_newer(),
                requires_python: lock.requires_python(),
                tags: None,
            };

            let reporter = LatestVersionReporter::from(printer).with_length(packages.len() as u64);

            // Fetch the latest version for each package.
            let download_concurrency = &download_concurrency;
            let mut fetches = futures::stream::iter(packages)
                .map(|(package, index)| async move {
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
        &lock,
        markers.as_ref(),
        &latest,
        depth.into(),
        &prune,
        &package,
        &dev.with_defaults(defaults),
        no_dedupe,
        invert,
    );

    print!("{tree}");

    Ok(ExitStatus::Success)
}
