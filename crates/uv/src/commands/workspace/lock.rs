use anstream::eprint;
use anyhow::Result;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{ConfigSettings, NoBinary, NoBuild, PreviewMode, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_interpreter::PythonEnvironment;
use uv_requirements::{ExtrasSpecification, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex, OptionsBuilder};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::workspace::Error;
use crate::commands::{workspace, ExitStatus};
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

    // TODO(charlie): If the environment doesn't exist, create it.
    let venv = PythonEnvironment::from_virtualenv(cache)?;

    // Find the workspace requirements.
    let Some(requirements) = workspace::find_workspace()? else {
        return Err(anyhow::anyhow!(
            "Unable to find `pyproject.toml` for project workspace."
        ));
    };

    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default();

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec = RequirementsSpecification::from_sources(
        &requirements,
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
    let config_settings = ConfigSettings::default();
    let flat_index = FlatIndex::default();
    let hasher = HashStrategy::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let index_locations = IndexLocations::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let setup_py = SetupPyStrategy::default();

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
    );

    let options = OptionsBuilder::new()
        // TODO(zanieb): Support resolver options
        // .resolution_mode(resolution_mode)
        // .prerelease_mode(prerelease_mode)
        // .dependency_mode(dependency_mode)
        // .exclude_newer(exclude_newer)
        .build();

    // Resolve the requirements.
    let resolution = workspace::resolve(
        spec,
        &hasher,
        &interpreter,
        tags,
        markers,
        &client,
        &flat_index,
        &index,
        &build_dispatch,
        options,
        printer,
    )
    .await;

    let resolution = match resolution {
        Err(Error::Resolve(uv_resolver::ResolveError::NoSolution(err))) => {
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
    fs_err::tokio::write("uv.lock", encoded.as_bytes()).await?;

    Ok(ExitStatus::Success)
}
