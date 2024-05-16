use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;
use tempfile::tempdir_in;
use tokio::process::Command;
use tracing::debug;

use distribution_types::{IndexLocations, Resolution};
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, ConfigSettings, NoBinary, NoBuild, PreviewMode, SetupPyStrategy,
};
use uv_dispatch::BuildDispatch;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_interpreter::PythonEnvironment;
use uv_requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex, OptionsBuilder};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::project::discovery::Project;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    target: Option<String>,
    mut args: Vec<OsString>,
    requirements: Vec<RequirementsSource>,
    python: Option<String>,
    isolated: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv run` is experimental and may change without warning.");
    }

    let command = if let Some(target) = target {
        let target_path = PathBuf::from(&target);
        if target_path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("py"))
            && target_path.exists()
        {
            args.insert(0, target_path.as_os_str().into());
            "python".to_string()
        } else {
            target
        }
    } else {
        "python".to_string()
    };

    // Discover and sync the project.
    let project_env = if isolated {
        None
    } else {
        debug!("Syncing project environment.");

        let Some(project) = Project::find(std::env::current_dir()?)? else {
            return Err(anyhow::anyhow!(
                "Unable to find `pyproject.toml` for project."
            ));
        };

        let venv = project::init(&project, cache, printer)?;

        // Install the project requirements.
        Some(update_environment(venv, &project.requirements(), preview, cache, printer).await?)
    };

    // If necessary, create an environment for the ephemeral requirements.
    let tmpdir;
    let ephemeral_env = if requirements.is_empty() {
        None
    } else {
        debug!("Syncing ephemeral environment.");

        // Discover an interpreter.
        let interpreter = if let Some(project_env) = &project_env {
            project_env.interpreter().clone()
        } else if let Some(python) = python.as_ref() {
            PythonEnvironment::from_requested_python(python, cache)?.into_interpreter()
        } else {
            PythonEnvironment::from_default_python(cache)?.into_interpreter()
        };

        // TODO(charlie): If the environment satisfies the requirements, skip creation.
        // TODO(charlie): Pass the already-installed versions as preferences, or even as the
        // "installed" packages, so that we can skip re-installing them in the ephemeral
        // environment.

        // Create a virtual environment
        // TODO(zanieb): Move this path derivation elsewhere
        let uv_state_path = std::env::current_dir()?.join(".uv");
        fs_err::create_dir_all(&uv_state_path)?;
        tmpdir = tempdir_in(uv_state_path)?;
        let venv = uv_virtualenv::create_venv(
            tmpdir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
        )?;

        // Install the ephemeral requirements.
        Some(update_environment(venv, &requirements, preview, cache, printer).await?)
    };

    // Construct the command
    let mut process = Command::new(&command);
    process.args(&args);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::scripts)
            .into_iter()
            .chain(
                project_env
                    .as_ref()
                    .map(PythonEnvironment::scripts)
                    .into_iter(),
            )
            .map(PathBuf::from)
            .chain(
                std::env::var_os("PATH")
                    .as_ref()
                    .iter()
                    .flat_map(std::env::split_paths),
            ),
    )?;
    process.env("PATH", new_path);

    // Construct the `PYTHONPATH` environment variable.
    let new_python_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::site_packages)
            .into_iter()
            .flatten()
            .chain(
                project_env
                    .as_ref()
                    .map(PythonEnvironment::site_packages)
                    .into_iter()
                    .flatten(),
            )
            .map(PathBuf::from)
            .chain(
                std::env::var_os("PYTHONPATH")
                    .as_ref()
                    .iter()
                    .flat_map(std::env::split_paths),
            ),
    )?;
    process.env("PYTHONPATH", new_python_path);

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let space = if args.is_empty() { "" } else { " " };
    debug!(
        "Running `{command}{space}{}`",
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );
    let mut handle = process.spawn()?;
    let status = handle.wait().await?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}

/// Update a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
async fn update_environment(
    venv: PythonEnvironment,
    requirements: &[RequirementsSource],
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment> {
    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default();

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec = RequirementsSpecification::from_sources(
        requirements,
        &[],
        &[],
        &ExtrasSpecification::None,
        &client_builder,
        preview,
    )
    .await?;

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_executable(&venv)?;

    // If the requirements are already satisfied, we're done.
    if spec.source_trees.is_empty() {
        match site_packages.satisfies(&spec.requirements, &spec.editables, &spec.constraints)? {
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                debug!(
                    "All requirements satisfied: {}",
                    recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                        .join(" | ")
                );
                debug!(
                    "All editables satisfied: {}",
                    spec.editables.iter().map(ToString::to_string).join(", ")
                );
                return Ok(venv);
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

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
    let concurrency = Concurrency::default();

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

    let options = OptionsBuilder::new()
        // TODO(zanieb): Support resolver options
        // .resolution_mode(resolution_mode)
        // .prerelease_mode(prerelease_mode)
        // .dependency_mode(dependency_mode)
        // .exclude_newer(exclude_newer)
        .build();

    // Resolve the requirements.
    let resolution = match project::resolve(
        spec,
        site_packages.clone(),
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
        concurrency,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(err) => return Err(err.into()),
    };

    // Re-initialize the in-flight map.
    let in_flight = InFlight::default();

    // Sync the environment.
    project::install(
        &resolution,
        site_packages,
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
        concurrency,
    )
    .await?;

    Ok(venv)
}
