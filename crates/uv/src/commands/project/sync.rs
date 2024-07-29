use anyhow::Result;

use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, ExtrasSpecification, HashCheckingMode, PreviewMode, SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::DEV_DEPENDENCIES;
use uv_fs::CWD;
use uv_installer::SitePackages;
use uv_python::{PythonEnvironment, PythonFetch, PythonPreference, PythonRequest};
use uv_resolver::{FlatIndex, Lock};
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject};

use crate::commands::pip::operations::Modifications;
use crate::commands::project::lock::do_safe_lock;
use crate::commands::project::{ProjectError, SharedState};
use crate::commands::{pip, project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{InstallerSettingsRef, ResolverInstallerSettings};

/// Sync the project environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn sync(
    locked: bool,
    frozen: bool,
    extras: ExtrasSpecification,
    dev: bool,
    modifications: Modifications,
    python: Option<String>,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    settings: ResolverInstallerSettings,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv sync` is experimental and may change without warning");
    }

    // Identify the project
    let project = VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await?;

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_fetch,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Initialize any shared state.
    let state = SharedState::default();

    let lock = match do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.as_ref().into(),
        &state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
    {
        Ok(lock) => lock,
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            let report = miette::Report::msg(format!("{err}")).context(err.header());
            anstream::eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // Perform the sync operation.
    do_sync(
        &project,
        &venv,
        &lock.lock,
        &extras,
        dev,
        modifications,
        settings.as_ref().into(),
        &state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}

/// Sync a lockfile with an environment.
pub(super) async fn do_sync(
    project: &VirtualProject,
    venv: &PythonEnvironment,
    lock: &Lock,
    extras: &ExtrasSpecification,
    dev: bool,
    modifications: Modifications,
    settings: InstallerSettingsRef<'_>,
    state: &SharedState,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<(), ProjectError> {
    // Extract the project settings.
    let InstallerSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        config_setting,
        exclude_newer,
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
    } = settings;

    // Validate that the Python version is supported by the lockfile.
    if let Some(requires_python) = lock.requires_python() {
        if !requires_python.contains(venv.interpreter().python_version()) {
            return Err(ProjectError::LockedPythonIncompatibility(
                venv.interpreter().python_version().clone(),
                requires_python.clone(),
            ));
        }
    }

    // Include development dependencies, if requested.
    let dev = if dev {
        vec![DEV_DEPENDENCIES.clone()]
    } else {
        vec![]
    };

    let markers = venv.interpreter().markers();
    let tags = venv.interpreter().tags()?;

    // Read the lockfile.
    let resolution = lock.to_resolution(project, markers, tags, extras, &dev)?;

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_isolation = BuildIsolation::default();
    let dry_run = false;
    let setup_py = SetupPyStrategy::default();

    // Extract the hashes from the lockfile.
    let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        venv.interpreter(),
        index_locations,
        &flat_index,
        &state.index,
        &state.git,
        &state.in_flight,
        index_strategy,
        setup_py,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        exclude_newer,
        concurrency,
        preview,
    );

    let site_packages = SitePackages::from_environment(venv)?;

    // Sync the environment.
    pip::operations::install(
        &resolution,
        site_packages,
        modifications,
        reinstall,
        build_options,
        link_mode,
        compile_bytecode,
        index_locations,
        &hasher,
        tags,
        &client,
        &state.in_flight,
        concurrency,
        &build_dispatch,
        cache,
        venv,
        dry_run,
        printer,
        preview,
    )
    .await?;

    Ok(())
}
