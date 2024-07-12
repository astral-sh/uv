use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::debug;

use pypi_types::Requirement;
use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_distribution::{VirtualProject, Workspace, WorkspaceError};
use uv_fs::Simplified;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    request_from_version_file, EnvironmentPreference, Interpreter, PythonEnvironment, PythonFetch,
    PythonInstallation, PythonPreference, PythonRequest, VersionRequest,
};

use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_warnings::warn_user_once;

use crate::commands::pip::operations::Modifications;
use crate::commands::project::environment::CachedEnvironment;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
pub(crate) async fn run(
    command: ExternalCommand,
    requirements: Vec<RequirementsSource>,
    package: Option<PackageName>,
    extras: ExtrasSpecification,
    dev: bool,
    python: Option<String>,
    settings: ResolverInstallerSettings,
    isolated: bool,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv run` is experimental and may change without warning.");
    }

    // Parse the input command.
    let command = RunCommand::from(command);

    // Initialize any shared state.
    let state = SharedState::default();

    let reporter = PythonDownloadReporter::single(printer);

    // Determine whether the command to execute is a PEP 723 script.
    let script_interpreter = if let RunCommand::Python(target, _) = &command {
        if let Some(metadata) = uv_scripts::read_pep723_metadata(&target).await? {
            writeln!(
                printer.stderr(),
                "Reading inline script metadata from: {}",
                target.user_display().cyan()
            )?;

            // (1) Explicit request from user
            let python_request = if let Some(request) = python.as_deref() {
                Some(PythonRequest::parse(request))
                // (2) Request from `.python-version`
            } else if let Some(request) = request_from_version_file().await? {
                Some(request)
                // (3) `Requires-Python` in `pyproject.toml`
            } else {
                metadata.requires_python.map(|requires_python| {
                    PythonRequest::Version(VersionRequest::Range(requires_python))
                })
            };

            let client_builder = BaseClientBuilder::new()
                .connectivity(connectivity)
                .native_tls(native_tls);

            let interpreter = PythonInstallation::find_or_fetch(
                python_request,
                EnvironmentPreference::Any,
                python_preference,
                python_fetch,
                &client_builder,
                cache,
                Some(&reporter),
            )
            .await?
            .into_interpreter();

            // Install the script requirements.
            let requirements = metadata
                .dependencies
                .into_iter()
                .map(Requirement::from)
                .collect();
            let environment = CachedEnvironment::get_or_create(
                requirements,
                interpreter,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            Some(environment.into_interpreter())
        } else {
            None
        }
    } else {
        None
    };

    // Discover and sync the base environment.
    let base_interpreter = if let Some(script_interpreter) = script_interpreter {
        Some(script_interpreter)
    } else if isolated {
        // package is `None`, isolated and package are marked as conflicting in clap.
        None
    } else {
        let project = if let Some(package) = package {
            // We need a workspace, but we don't need to have a current package, we can be e.g. in
            // the root of a virtual workspace and then switch into the selected package.
            Some(VirtualProject::Project(
                Workspace::discover(&std::env::current_dir()?, None)
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            ))
        } else {
            match VirtualProject::discover(&std::env::current_dir()?, None).await {
                Ok(project) => Some(project),
                Err(WorkspaceError::MissingPyprojectToml) => None,
                Err(WorkspaceError::NonWorkspace(_)) => None,
                Err(err) => return Err(err.into()),
            }
        };

        let interpreter = if let Some(project) = project {
            if let Some(project_name) = project.project_name() {
                debug!(
                    "Discovered project `{project_name}` at: {}",
                    project.workspace().install_path().display()
                );
            } else {
                debug!(
                    "Discovered virtual workspace at: {}",
                    project.workspace().install_path().display()
                );
            }

            let venv = project::get_or_init_environment(
                project.workspace(),
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_fetch,
                connectivity,
                native_tls,
                cache,
                printer,
            )
            .await?;

            // Read the existing lockfile.
            let existing = project::lock::read(project.workspace()).await?;

            // Lock and sync the environment.
            let lock = project::lock::do_lock(
                project.workspace(),
                venv.interpreter(),
                existing.as_ref(),
                settings.as_ref().into(),
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            if !existing.is_some_and(|existing| existing == lock) {
                project::lock::commit(&lock, project.workspace()).await?;
            }

            project::sync::do_sync(
                &project,
                &venv,
                &lock,
                extras,
                dev,
                Modifications::Sufficient,
                settings.as_ref().into(),
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            venv.into_interpreter()
        } else {
            debug!("No project found; searching for Python interpreter");

            let client_builder = BaseClientBuilder::new()
                .connectivity(connectivity)
                .native_tls(native_tls);

            let python = PythonInstallation::find_or_fetch(
                python.as_deref().map(PythonRequest::parse),
                // No opt-in is required for system environments, since we are not mutating it.
                EnvironmentPreference::Any,
                python_preference,
                python_fetch,
                &client_builder,
                cache,
                Some(&reporter),
            )
            .await?;

            python.into_interpreter()
        };

        Some(interpreter)
    };

    if let Some(base_interpreter) = &base_interpreter {
        debug!(
            "Using Python {} interpreter at: {}",
            base_interpreter.python_version(),
            base_interpreter.sys_executable().display()
        );
    }

    // Read the `--with` requirements.
    let spec = if requirements.is_empty() {
        None
    } else {
        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);

        let spec =
            RequirementsSpecification::from_simple_sources(&requirements, &client_builder).await?;

        Some(spec)
    };

    // Determine whether the base environment satisfies the ephemeral requirements. If we don't have
    // any `--with` requirements, and we already have a base environment, then there's no need to
    // create an additional environment.
    let skip_ephemeral = base_interpreter.as_ref().is_some_and(|base_interpreter| {
        let Some(spec) = spec.as_ref() else {
            return true;
        };

        let Ok(site_packages) = SitePackages::from_interpreter(base_interpreter) else {
            return false;
        };

        if !(settings.reinstall.is_none() && settings.reinstall.is_none()) {
            return false;
        }

        match site_packages.satisfies(&spec.requirements, &spec.constraints) {
            // If the requirements are already satisfied, we're done.
            Ok(SatisfiesResult::Fresh {
                recursive_requirements,
            }) => {
                debug!(
                    "Base environment satisfies requirements: {}",
                    recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                        .join(" | ")
                );
                true
            }
            Ok(SatisfiesResult::Unsatisfied(requirement)) => {
                debug!("At least one requirement is not satisfied in the base environment: {requirement}");
                false
            }
            Err(err) => {
                debug!("Failed to check requirements against base environment: {err}");
                false
            }
        }
    });

    // If necessary, create an environment for the ephemeral requirements or command.
    let temp_dir;
    let ephemeral_env = if skip_ephemeral {
        None
    } else {
        debug!("Creating ephemeral environment");

        // Discover an interpreter.
        let interpreter = if let Some(base_interpreter) = &base_interpreter {
            base_interpreter.clone()
        } else {
            let client_builder = BaseClientBuilder::new()
                .connectivity(connectivity)
                .native_tls(native_tls);

            // Note we force preview on during `uv run` for now since the entire interface is in preview
            PythonInstallation::find_or_fetch(
                python.as_deref().map(PythonRequest::parse),
                EnvironmentPreference::Any,
                python_preference,
                python_fetch,
                &client_builder,
                cache,
                Some(&reporter),
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

        if requirements.is_empty() {
            Some(venv)
        } else {
            debug!("Syncing ephemeral requirements");

            let client_builder = BaseClientBuilder::new()
                .connectivity(connectivity)
                .native_tls(native_tls);

            let spec =
                RequirementsSpecification::from_simple_sources(&requirements, &client_builder)
                    .await?;

            // Install the ephemeral requirements.
            Some(
                project::update_environment(
                    venv,
                    spec,
                    &settings,
                    &state,
                    preview,
                    connectivity,
                    concurrency,
                    native_tls,
                    cache,
                    printer,
                )
                .await?,
            )
        }
    };

    debug!("Running `{command}`");
    let mut process = Command::from(&command);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::scripts)
            .into_iter()
            .chain(
                base_interpreter
                    .as_ref()
                    .map(Interpreter::scripts)
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
                base_interpreter
                    .as_ref()
                    .map(Interpreter::site_packages)
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
    let mut handle = process.spawn().with_context(|| {
        format!(
            "Failed to spawn: `{}`",
            command.executable().to_string_lossy()
        )
    })?;
    let status = handle.wait().await.context("Child process disappeared")?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}

#[derive(Debug)]
enum RunCommand {
    /// Execute a `python` script.
    Python(PathBuf, Vec<OsString>),
    /// Execute an external command.
    External(OsString, Vec<OsString>),
    /// Execute an empty command (in practice, `python` with no arguments).
    Empty,
}

impl RunCommand {
    /// Return the name of the target executable.
    fn executable(&self) -> Cow<'_, OsString> {
        match self {
            Self::Python(_, _) | Self::Empty => Cow::Owned(OsString::from("python")),
            Self::External(executable, _) => Cow::Borrowed(executable),
        }
    }
}

impl std::fmt::Display for RunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python(target, args) => {
                write!(f, "python {}", target.display())?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::External(executable, args) => {
                write!(f, "{}", executable.to_string_lossy())?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::Empty => {
                write!(f, "python")?;
                Ok(())
            }
        }
    }
}

impl From<ExternalCommand> for RunCommand {
    fn from(command: ExternalCommand) -> Self {
        let (target, args) = command.split();

        let Some(target) = target else {
            return Self::Empty;
        };

        let target_path = PathBuf::from(&target);
        if target_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py"))
            && target_path.exists()
        {
            Self::Python(target_path, args.to_vec())
        } else {
            Self::External(
                target.clone(),
                args.iter().map(std::clone::Clone::clone).collect(),
            )
        }
    }
}

impl From<&RunCommand> for Command {
    fn from(command: &RunCommand) -> Self {
        match command {
            RunCommand::Python(target, args) => {
                let mut process = Command::new("python");
                process.arg(target);
                process.args(args);
                process
            }
            RunCommand::External(executable, args) => {
                let mut process = Command::new(executable);
                process.args(args);
                process
            }
            RunCommand::Empty => Command::new("python"),
        }
    }
}
