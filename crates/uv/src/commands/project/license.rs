use std::path::Path;

use anyhow::{Error, Result};

use futures::StreamExt;
use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DevGroupsSpecification, LowerBound, PreviewMode, TargetTriple,
    TrustedHost,
};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::Index;
use uv_python::{
    PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest, PythonVersion,
};
use uv_resolver::{FlatIndex, LicenseDisplay, PackageMap};
use uv_settings::PythonInstallMirrors;
use uv_types::{BuildIsolation, HashStrategy};
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::lock::{do_safe_lock, LockMode};
use crate::commands::project::{
    default_dependency_groups, DependencyGroupsTarget, ProjectError, ProjectInterpreter,
};
use crate::commands::{diagnostics, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn license(
    project_dir: &Path,
    dev: DevGroupsSpecification,
    locked: bool,
    frozen: bool,
    universal: bool,
    direct_only: bool,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(project_dir, &DiscoveryOptions::default()).await?;

    // Validate that any referenced dependency groups are defined in the workspace.
    if !frozen {
        let target = DependencyGroupsTarget::Workspace(&workspace);
        target.validate(&dev)?;
    }

    // Determine the default groups to include.
    let defaults = default_dependency_groups(workspace.pyproject_toml())?;

    // Find an interpreter for the project, unless `--frozen` and `--universal` are both set.
    let interpreter = if frozen && universal {
        None
    } else {
        Some(
            ProjectInterpreter::discover(
                &workspace,
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_downloads,
                connectivity,
                native_tls,
                allow_insecure_host,
                &install_mirrors,
                no_config,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        )
    };

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = SharedState::default();
    let bounds = LowerBound::Allow;

    // Update the lockfile, if necessary.
    let lock = match do_safe_lock(
        mode,
        (&workspace).into(),
        settings.as_ref(),
        bounds,
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
            return diagnostics::OperationDiagnostic::default()
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

    let ResolverSettings {
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
        upgrade: _,
        build_options,
        sources,
    } = settings;

    // Initialize the registry client.
    let client: uv_client::RegistryClient =
        RegistryClientBuilder::new(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
            .native_tls(native_tls)
            .connectivity(connectivity)
            .keyring(keyring_provider)
            .allow_insecure_host(allow_insecure_host.to_vec())
            .build();
    let environment;
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.as_ref().unwrap().clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.as_ref().unwrap().clone());
        BuildIsolation::SharedPackage(&environment, no_build_isolation_package.as_ref())
    };

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_hasher = HashStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, None, &build_hasher, &build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        Constraints::default(),
        interpreter.as_ref().unwrap(),
        &index_locations,
        &flat_index,
        &dependency_metadata,
        state.clone(),
        index_strategy.clone(),
        &config_setting,
        build_isolation,
        link_mode,
        &build_options,
        &build_hasher,
        exclude_newer,
        bounds,
        sources,
        concurrency,
        preview,
    );
    let database = DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads);

    let mut licenses = PackageMap::default();

    let interpret = interpreter.as_ref().expect("need an interpreter").tags()?;
    let ws = &workspace;
    let db = &database;
    let mut fetches = futures::stream::iter(lock.packages())
        .map(|package| async move {
            let license = package.license(&ws.clone(), interpret, &db).await;
            Ok::<Option<_>, Error>(Some((package, license)))
        })
        .buffer_unordered(concurrency.downloads);
    while let Some(entry) = fetches.next().await.transpose()? {
        let Some((package, license)) = entry else {
            continue;
        };
        match license {
            Some(license) => licenses.insert(package.clone(), license),
            None => continue,
        };
    }

    // Render the license information.
    let display = LicenseDisplay::new(
        &lock,
        markers.as_ref(),
        &licenses,
        direct_only,
        &dev.with_defaults(defaults),
    );

    print!("{display}");

    Ok(ExitStatus::Success)
}
