use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::debug;

use pypi_types::Requirement;
use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_fs::{PythonExt, Simplified, CWD};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    request_from_version_file, EnvironmentPreference, Interpreter, PythonEnvironment, PythonFetch,
    PythonInstallation, PythonPreference, PythonRequest, VersionRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace, WorkspaceError};

use crate::commands::pip::operations::Modifications;
use crate::commands::project::environment::CachedEnvironment;
use crate::commands::project::ProjectError;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{pip, project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn run(
    command: ExternalCommand,
    requirements: Vec<RequirementsSource>,
    show_resolution: bool,
    locked: bool,
    frozen: bool,
    isolated: bool,
    package: Option<PackageName>,
    no_project: bool,
    extras: ExtrasSpecification,
    dev: bool,
    python: Option<String>,
    settings: ResolverInstallerSettings,
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
        warn_user_once!("`uv run` is experimental and may change without warning");
    }

    // These cases seem quite complex because (in theory) they should change the "current package".
    // Let's ban them entirely for now.
    for source in &requirements {
        match source {
            RequirementsSource::PyprojectToml(_) => {
                bail!("Adding requirements from a `pyproject.toml` is not supported in `uv run`");
            }
            RequirementsSource::SetupPy(_) => {
                bail!("Adding requirements from a `setup.py` is not supported in `uv run`");
            }
            RequirementsSource::SetupCfg(_) => {
                bail!("Adding requirements from a `setup.cfg` is not supported in `uv run`");
            }
            RequirementsSource::RequirementsTxt(path) => {
                if path == Path::new("-") {
                    bail!("Reading requirements from stdin is not supported in `uv run`");
                }
            }
            _ => {}
        }
    }

    // Parse the input command.
    let command = RunCommand::from(command);

    // Initialize any shared state.
    let state = SharedState::default();

    let reporter = PythonDownloadReporter::single(printer.filter(show_resolution));

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
            } else if let Some(request) = request_from_version_file(&CWD).await? {
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
            let spec = RequirementsSpecification::from_requirements(requirements);
            let environment = CachedEnvironment::get_or_create(
                spec,
                interpreter,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer.filter(show_resolution),
            )
            .await?;

            Some(environment.into_interpreter())
        } else {
            None
        }
    } else {
        None
    };

    let temp_dir;

    // Discover and sync the base environment.
    let base_interpreter = if let Some(script_interpreter) = script_interpreter {
        Some(script_interpreter)
    } else if no_project {
        // package is `None` (`--no-project` and `--package` are marked as conflicting in Clap).
        None
    } else {
        let project = if let Some(package) = package {
            // We need a workspace, but we don't need to have a current package, we can be e.g. in
            // the root of a virtual workspace and then switch into the selected package.
            Some(VirtualProject::Project(
                Workspace::discover(&CWD, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            ))
        } else {
            match VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await {
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

            let venv = if isolated {
                // If we're isolating the environment, use an ephemeral virtual environment as the
                // base environment for the project.
                let interpreter = {
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

                // Create a virtual environment
                temp_dir = cache.environment()?;
                uv_virtualenv::create_venv(
                    temp_dir.path(),
                    interpreter,
                    uv_virtualenv::Prompt::None,
                    false,
                    false,
                    false,
                )?
            } else {
                // If we're not isolating the environment, reuse the base environment for the
                // project.
                project::get_or_init_environment(
                    project.workspace(),
                    python.as_deref().map(PythonRequest::parse),
                    python_preference,
                    python_fetch,
                    connectivity,
                    native_tls,
                    cache,
                    printer.filter(show_resolution),
                )
                .await?
            };

            let lock = match project::lock::do_safe_lock(
                locked,
                frozen,
                project.workspace(),
                venv.interpreter(),
                settings.as_ref().into(),
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer.filter(show_resolution),
            )
            .await
            {
                Ok(lock) => lock,
                Err(ProjectError::Operation(pip::operations::Error::Resolve(
                    uv_resolver::ResolveError::NoSolution(err),
                ))) => {
                    let report = miette::Report::msg(format!("{err}")).context(err.header());
                    anstream::eprint!("{report:?}");
                    return Ok(ExitStatus::Failure);
                }
                Err(err) => return Err(err.into()),
            };

            project::sync::do_sync(
                &project,
                &venv,
                &lock.lock,
                &extras,
                dev,
                Modifications::Sufficient,
                settings.as_ref().into(),
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer.filter(show_resolution),
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

    // Read the requirements.
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
        // No additional requirements.
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
            false,
        )?;

        match spec {
            None => Some(venv),
            Some(spec) if spec.is_empty() => Some(venv),
            Some(spec) => {
                debug!("Syncing ephemeral requirements");
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
                        printer.filter(show_resolution),
                    )
                    .await?,
                )
            }
        }
    };

    // If we're running in an ephemeral environment, add a `sitecustomize.py` to enable loading of
    // the base environment's site packages. Setting `PYTHONPATH` is insufficient, as it doesn't
    // resolve `.pth` files in the base environment.
    if let Some(ephemeral_env) = ephemeral_env.as_ref() {
        if let Some(base_interpreter) = base_interpreter.as_ref() {
            let ephemeral_site_packages = ephemeral_env
                .site_packages()
                .next()
                .ok_or_else(|| anyhow!("Ephemeral environment has no site packages directory"))?;
            let base_site_packages = base_interpreter
                .site_packages()
                .next()
                .ok_or_else(|| anyhow!("Base environment has no site packages directory"))?;

            fs_err::write(
                ephemeral_site_packages.join("sitecustomize.py"),
                format!(
                    "import site; site.addsitedir(\"{}\")",
                    base_site_packages.escape_for_python()
                ),
            )?;
        }
    }

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

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let mut handle = process.spawn().with_context(|| {
        format!(
            "Failed to spawn: `{}`",
            command.executable().to_string_lossy()
        )
    })?;

    // Ignore signals in the parent process, deferring them to the child. This is safe as long as
    // the command is the last thing that runs in this process; otherwise, we'd need to restore the
    // signal handlers after the command completes.
    let _handler = tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });

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
