use std::path::Path;

use anyhow::Result;

use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::{ProjectWorkspace, DEV_DEPENDENCIES};
use uv_git::GitResolver;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_resolver::{FlatIndex, InMemoryIndex, Lock};
use uv_toolchain::{PythonEnvironment, ToolchainPreference, ToolchainRequest};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user_once;

use crate::commands::pip::operations::Modifications;
use crate::commands::project::ProjectError;
use crate::commands::{pip, project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{InstallerSettings, InstallerSettingsRef};

/// Sync the project environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync(
    extras: ExtrasSpecification,
    dev: bool,
    modifications: Modifications,
    python: Option<String>,
    toolchain_preference: ToolchainPreference,
    settings: InstallerSettings,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv sync` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(&std::env::current_dir()?, None).await?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(
        project.workspace(),
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Read the lockfile.
    let lock: Lock = {
        let encoded =
            fs_err::tokio::read_to_string(project.workspace().root().join("uv.lock")).await?;
        toml::from_str(&encoded)?
    };

    // Perform the sync operation.
    do_sync(
        project.project_name(),
        project.workspace().root(),
        &venv,
        &lock,
        extras,
        dev,
        modifications,
        settings.as_ref(),
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
#[allow(clippy::too_many_arguments)]
pub(super) async fn do_sync(
    project_name: &PackageName,
    workspace_root: &Path,
    venv: &PythonEnvironment,
    lock: &Lock,
    extras: ExtrasSpecification,
    dev: bool,
    modifications: Modifications,
    settings: InstallerSettingsRef<'_>,
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
        link_mode,
        compile_bytecode,
        reinstall,
        build_options,
    } = settings;

    // Validate that the Python version is supported by the lockfile.
    if let Some(requires_python) = lock.requires_python() {
        if !requires_python.contains(venv.interpreter().python_version()) {
            return Err(ProjectError::PythonIncompatibility(
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
    let resolution =
        lock.to_resolution(workspace_root, markers, tags, project_name, &extras, &dev)?;

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

    // Initialize any shared state.
    let git = GitResolver::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_isolation = BuildIsolation::default();
    let dry_run = false;
    let exclude_newer = None;
    let hasher = HashStrategy::default();
    let setup_py = SetupPyStrategy::default();

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
        &index,
        &git,
        &in_flight,
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
        &in_flight,
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
