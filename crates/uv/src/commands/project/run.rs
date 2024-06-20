use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result};
use itertools::Itertools;
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_distribution::{ProjectWorkspace, Workspace};
use uv_normalize::PackageName;
use uv_requirements::RequirementsSource;
use uv_toolchain::{
    EnvironmentPreference, PythonEnvironment, Toolchain, ToolchainPreference, ToolchainRequest,
};
use uv_warnings::warn_user;

use crate::cli::ExternalCommand;
use crate::commands::pip::operations::Modifications;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run(
    extras: ExtrasSpecification,
    dev: bool,
    command: ExternalCommand,
    requirements: Vec<RequirementsSource>,
    python: Option<String>,
    package: Option<PackageName>,
    settings: ResolverInstallerSettings,
    isolated: bool,
    preview: PreviewMode,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv run` is experimental and may change without warning.");
    }

    // Discover and sync the project.
    let project_env = if isolated {
        // package is `None`, isolated and package are marked as conflicting in clap.
        None
    } else {
        debug!("Syncing project environment.");

        let project = if let Some(package) = package {
            // We need a workspace, but we don't need to have a current package, we can be e.g. in
            // the root of a virtual workspace and then switch into the selected package.
            Workspace::discover(&std::env::current_dir()?, None)
                .await?
                .with_current_project(package.clone())
                .with_context(|| format!("Package `{package}` not found in workspace"))?
        } else {
            ProjectWorkspace::discover(&std::env::current_dir()?, None).await?
        };
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

        // Lock and sync the environment.
        let lock = project::lock::do_lock(
            project.workspace(),
            venv.interpreter(),
            &settings.upgrade,
            &settings.index_locations,
            &settings.index_strategy,
            &settings.keyring_provider,
            &settings.resolution,
            &settings.prerelease,
            &settings.config_setting,
            settings.exclude_newer.as_ref(),
            &settings.link_mode,
            &settings.build_options,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;
        project::sync::do_sync(
            project.project_name(),
            project.workspace().root(),
            &venv,
            &lock,
            extras,
            dev,
            Modifications::Sufficient,
            &settings.reinstall,
            &settings.index_locations,
            &settings.index_strategy,
            &settings.keyring_provider,
            &settings.config_setting,
            &settings.link_mode,
            &settings.compile_bytecode,
            &settings.build_options,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        Some(venv)
    };

    // If necessary, create an environment for the ephemeral requirements.
    let temp_dir;
    let ephemeral_env = if requirements.is_empty() {
        None
    } else {
        debug!("Syncing ephemeral environment.");

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);

        // Discover an interpreter.
        let interpreter = if let Some(project_env) = &project_env {
            project_env.interpreter().clone()
        } else {
            // Note we force preview on during `uv run` for now since the entire interface is in preview
            Toolchain::find_or_fetch(
                python.as_deref().map(ToolchainRequest::parse),
                EnvironmentPreference::Any,
                toolchain_preference,
                client_builder,
                cache,
            )
            .await?
            .into_interpreter()
        };

        // TODO(charlie): If the environment satisfies the requirements, skip creation.
        // TODO(charlie): Pass the already-installed versions as preferences, or even as the
        // "installed" packages, so that we can skip re-installing them in the ephemeral
        // environment.

        // Create a virtual environment
        temp_dir = cache.environment()?;
        let venv = uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
        )?;

        // Install the ephemeral requirements.
        Some(
            project::update_environment(
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
        )
    };

    let (target, args) = command.split();
    let (command, prefix_args) = if let Some(target) = target {
        let target_path = PathBuf::from(&target);
        if target_path
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("py"))
            && target_path.exists()
        {
            (OsString::from("python"), vec![target_path])
        } else {
            (target.clone(), vec![])
        }
    } else {
        (OsString::from("python"), vec![])
    };

    let mut process = Command::new(&command);
    process.args(prefix_args);
    process.args(args);

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
