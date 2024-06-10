use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result};
use distribution_types::IndexLocations;
use itertools::Itertools;
use tempfile::tempdir_in;
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_requirements::RequirementsSource;
use uv_toolchain::{PythonEnvironment, SystemPython, Toolchain};
use uv_warnings::warn_user;

use crate::commands::project::update_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    target: String,
    args: Vec<OsString>,
    python: Option<String>,
    from: Option<String>,
    with: Vec<String>,
    _isolated: bool,
    preview: PreviewMode,
    index_locations: IndexLocations,
    connectivity: Connectivity,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv tool run` is experimental and may change without warning.");
    }

    let requirements = [RequirementsSource::from_package(
        from.unwrap_or_else(|| target.clone()),
    )]
    .into_iter()
    .chain(with.into_iter().map(RequirementsSource::from_package))
    .collect::<Vec<_>>();

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool.
    // TODO(zanieb): Determine if we should layer on top of the project environment if it is present.

    // If necessary, create an environment for the ephemeral requirements.
    debug!("Syncing ephemeral environment.");

    // Discover an interpreter.
    // Note we force preview on during `uv tool run` for now since the entire interface is in preview
    let interpreter = Toolchain::find(
        python.as_deref(),
        SystemPython::Allowed,
        PreviewMode::Enabled,
        cache,
    )?
    .into_interpreter();

    // Create a virtual environment1
    // TODO(zanieb): Move this path derivation elsewhere
    let uv_state_path = std::env::current_dir()?.join(".uv");
    fs_err::create_dir_all(&uv_state_path)?;
    let tmpdir = tempdir_in(uv_state_path)?;
    let venv = uv_virtualenv::create_venv(
        tmpdir.path(),
        interpreter,
        uv_virtualenv::Prompt::None,
        false,
        false,
    )?;

    // Install the ephemeral requirements.
    let ephemeral_env = Some(
        update_environment(
            venv,
            &requirements,
            &index_locations,
            connectivity,
            cache,
            printer,
            preview,
        )
        .await?,
    );

    // TODO(zanieb): Determine the command via the package entry points
    let command = target;

    // Construct the command
    let mut process = Command::new(&command);
    process.args(&args);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::scripts)
            .into_iter()
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
    let mut handle = process
        .spawn()
        .with_context(|| format!("Failed to spawn: `{command}`"))?;
    let status = handle.wait().await.context("Child process disappeared")?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}
