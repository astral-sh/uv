use anstream::eprint;
use anyhow::Result;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, ConfigSettings, NoBinary, NoBuild, PreviewMode, Reinstall, SetupPyStrategy,
    Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_requirements::{ExtrasSpecification, ProjectWorkspace, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex, Options};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::{pip, project, ExitStatus};
use crate::editables::ResolvedEditables;
use crate::printer::Printer;

/// Resolve the project requirements into a lockfile.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lock(
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv lock` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(std::env::current_dir()?)?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(&project, cache, printer)?;

    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default();

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec = RequirementsSpecification::from_sources(
        &project.requirements(),
        &[],
        &[],
        &ExtrasSpecification::None,
        &client_builder,
        preview,
    )
    .await?;

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let extras = ExtrasSpecification::default();
    let flat_index = FlatIndex::default();
    let hasher = HashStrategy::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let index_locations = IndexLocations::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let options = Options::default();
    let reinstall = Reinstall::default();
    let setup_py = SetupPyStrategy::default();
    let upgrade = Upgrade::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &index,
        &in_flight,
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &no_build,
        &no_binary,
        concurrency,
    );

    // Build all editable distributions. The editables are shared between resolution and
    // installation, and should live for the duration of the command.
    let editables = ResolvedEditables::resolve(
        spec.editables
            .iter()
            .cloned()
            .map(ResolvedEditables::from_requirement),
        &EmptyInstalledPackages,
        &reinstall,
        &hasher,
        &interpreter,
        tags,
        cache,
        &client,
        &build_dispatch,
        concurrency,
        printer,
    )
    .await?;

    // Resolve the requirements.
    let resolution = pip::operations::resolve(
        spec.requirements,
        spec.constraints,
        spec.overrides,
        spec.source_trees,
        spec.project,
        &extras,
        &editables,
        EmptyInstalledPackages,
        &hasher,
        &reinstall,
        &upgrade,
        &interpreter,
        tags,
        markers,
        &client,
        &flat_index,
        &index,
        &build_dispatch,
        concurrency,
        options,
        printer,
    )
    .await;

    let resolution = match resolution {
        Err(pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err))) => {
            let report = miette::Report::msg(format!("{err}"))
                .context("No solution found when resolving dependencies:");
            eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        result => result,
    }?;

    // Write the lockfile to disk.
    let lock = resolution.lock()?;
    let encoded = toml::to_string_pretty(&lock)?;
    fs_err::tokio::write(
        project.workspace().root().join("uv.lock"),
        encoded.as_bytes(),
    )
    .await?;

    Ok(ExitStatus::Success)
}
