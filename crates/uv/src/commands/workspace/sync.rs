use anyhow::Result;

use distribution_types::IndexLocations;
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_configuration::{ConfigSettings, NoBinary, NoBuild, PreviewMode, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;
use uv_resolver::{FlatIndex, InMemoryIndex, Lock};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::{workspace, ExitStatus};
use crate::printer::Printer;

/// Sync the project environment.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync(
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv sync` is experimental and may change without warning.");
    }

    // TODO(charlie): If the environment doesn't exist, create it.
    let venv = PythonEnvironment::from_virtualenv(cache)?;
    let markers = venv.interpreter().markers();
    let tags = venv.interpreter().tags()?;

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
        venv.interpreter(),
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

    // Read the lockfile.
    let resolution = {
        // TODO(charlie): Read the project name from disk.
        let root = PackageName::new("project".to_string())?;
        let encoded = fs_err::tokio::read_to_string("uv.lock").await?;
        let lock: Lock = toml::from_str(&encoded)?;
        lock.to_resolution(markers, tags, &root)
    };

    // Sync the environment.
    workspace::install(
        &resolution,
        SitePackages::from_executable(&venv)?,
        &no_binary,
        link_mode,
        &index_locations,
        &hasher,
        tags,
        &client,
        &in_flight,
        &build_dispatch,
        cache,
        &venv,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}
