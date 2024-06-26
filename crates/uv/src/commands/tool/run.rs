use std::path::PathBuf;

use anyhow::{Context, Result};
use itertools::Itertools;
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_requirements::RequirementsSource;
use uv_toolchain::{
    EnvironmentPreference, PythonEnvironment, Toolchain, ToolchainPreference, ToolchainRequest,
};
use uv_warnings::warn_user_once;

use crate::commands::project::update_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    command: ExternalCommand,
    python: Option<String>,
    from: Option<String>,
    with: Vec<String>,
    settings: ResolverInstallerSettings,
    _isolated: bool,
    preview: PreviewMode,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool run` is experimental and may change without warning.");
    }

    let (target, args) = command.split();
    let Some(target) = target else {
        return Err(anyhow::anyhow!("No tool command provided"));
    };

    let from = if let Some(from) = from {
        from
    } else {
        let Some(target) = target.to_str() else {
            return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name."));
        };
        target.to_string()
    };

    let requirements = [RequirementsSource::from_package(from)]
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
        &python
            .as_deref()
            .map(ToolchainRequest::parse)
            .unwrap_or_default(),
        EnvironmentPreference::OnlySystem,
        toolchain_preference,
        cache,
    )?
    .into_interpreter();

    // Create a virtual environment.
    let temp_dir = cache.environment()?;
    let venv = uv_virtualenv::create_venv(
        temp_dir.path(),
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
            &settings,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?,
    );

    // TODO(zanieb): Determine the command via the package entry points
    let command = target;

    // Construct the command
    let mut process = Command::new(command);
    process.args(args);

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
        "Running `{}{space}{}`",
        command.to_string_lossy(),
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );
    let mut handle = process
        .spawn()
        .with_context(|| format!("Failed to spawn: `{}`", command.to_string_lossy()))?;
    let status = handle.wait().await.context("Child process disappeared")?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}
