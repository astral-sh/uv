use anyhow::Result;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_configuration::{
    Concurrency, ConfigSettings, ExtrasSpecification, NoBinary, NoBuild, PreviewMode, Reinstall,
    SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::ProjectWorkspace;
use uv_git::GitResolver;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_resolver::{FlatIndex, InMemoryIndex, Lock};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::pip::operations::Modifications;
use crate::commands::project::ProjectError;
use crate::commands::{pip, project, ExitStatus};
use crate::printer::Printer;

/// Sync the project environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync(
    extras: ExtrasSpecification,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv sync` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(std::env::current_dir()?, None).await?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(&project, preview, cache, printer)?;

    // Read the lockfile.
    let lock: Lock = {
        let encoded =
            fs_err::tokio::read_to_string(project.workspace().root().join("uv.lock")).await?;
        toml::from_str(&encoded)?
    };

    // Perform the sync operation.
    do_sync(&project, &venv, &lock, extras, preview, cache, printer).await?;

    Ok(ExitStatus::Success)
}

/// Sync a lockfile with an environment.
pub(super) async fn do_sync(
    project: &ProjectWorkspace,
    venv: &PythonEnvironment,
    lock: &Lock,
    extras: ExtrasSpecification,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<(), ProjectError> {
    let markers = venv.interpreter().markers();
    let tags = venv.interpreter().tags()?;

    // Read the lockfile.
    let extras = match extras {
        ExtrasSpecification::None => vec![],
        ExtrasSpecification::All => project.project_extras().to_vec(),
        ExtrasSpecification::Some(extras) => extras,
    };
    let resolution = lock.to_resolution(markers, tags, project.project_name(), &extras);

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let compile = false;
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let dry_run = false;
    let flat_index = FlatIndex::default();
    let git = GitResolver::default();
    let hasher = HashStrategy::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let index_locations = IndexLocations::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let reinstall = Reinstall::default();
    let setup_py = SetupPyStrategy::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        venv.interpreter(),
        &index_locations,
        &flat_index,
        &index,
        &git,
        &in_flight,
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &no_build,
        &no_binary,
        concurrency,
        preview,
    );

    let site_packages = SitePackages::from_executable(venv)?;

    // Sync the environment.
    pip::operations::install(
        &resolution,
        site_packages,
        Modifications::Sufficient,
        &reinstall,
        &no_binary,
        link_mode,
        compile,
        &index_locations,
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
