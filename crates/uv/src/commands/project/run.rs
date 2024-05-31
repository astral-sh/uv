use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result};
use itertools::Itertools;
use tempfile::tempdir_in;
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{ExtrasSpecification, PreviewMode, Upgrade};
use uv_distribution::ProjectWorkspace;
use uv_interpreter::{PythonEnvironment, SystemPython};
use uv_requirements::RequirementsSource;
use uv_resolver::ExcludeNewer;
use uv_warnings::warn_user;

use crate::commands::{project, ExitStatus};
use crate::printer::Printer;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    extras: ExtrasSpecification,
    target: Option<String>,
    mut args: Vec<OsString>,
    requirements: Vec<RequirementsSource>,
    python: Option<String>,
    upgrade: Upgrade,
    exclude_newer: Option<ExcludeNewer>,
    isolated: bool,
    preview: PreviewMode,
    connectivity: Connectivity,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv run` is experimental and may change without warning.");
    }

    // Discover and sync the project.
    let project_env = if isolated {
        None
    } else {
        debug!("Syncing project environment.");

        let project = ProjectWorkspace::discover(std::env::current_dir()?, None).await?;
        let venv = project::init_environment(&project, preview, cache, printer)?;

        // Lock and sync the environment.
        let lock = project::lock::do_lock(
            &project,
            &venv,
            upgrade,
            exclude_newer,
            preview,
            cache,
            printer,
        )
        .await?;
        project::sync::do_sync(&project, &venv, &lock, extras, preview, cache, printer).await?;

        Some(venv)
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
            PythonEnvironment::from_requested_python(python, SystemPython::Allowed, preview, cache)?
                .into_interpreter()
        } else {
            PythonEnvironment::from_default_python(preview, cache)?.into_interpreter()
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
        Some(
            project::update_environment(venv, &requirements, connectivity, cache, printer, preview)
                .await?,
        )
    };

    // Construct the command
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
