use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use anstream::eprint;
use anyhow::{anyhow, bail, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, ExtrasSpecification};
use uv_distribution::LoweredRequirement;
use uv_fs::{PythonExt, Simplified, CWD};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    request_from_version_file, EnvironmentPreference, Interpreter, PythonDownloads,
    PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest, VersionRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_scripts::{Pep723Error, Pep723Script};
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace, WorkspaceError};

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations;
use crate::commands::pip::operations::Modifications;
use crate::commands::project::environment::CachedEnvironment;
use crate::commands::project::{ProjectError, WorkspacePython};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn run(
    script: Option<Pep723Script>,
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

    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
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
    let command = RunCommand::from(&command);

    // Initialize any shared state.
    let state = SharedState::default();

    // Initialize any output reporters.
    let download_reporter = PythonDownloadReporter::single(printer);

    // Determine whether the command to execute is a PEP 723 script.
    let temp_dir;
    let script_interpreter = if let Some(script) = script {
        writeln!(
            printer.stderr(),
            "Reading inline script metadata from: {}",
            script.path.user_display().cyan()
        )?;

        // (1) Explicit request from user
        let python_request = if let Some(request) = python.as_deref() {
            Some(PythonRequest::parse(request))
            // (2) Request from `.python-version`
        } else if let Some(request) = request_from_version_file(&CWD).await? {
            Some(request)
            // (3) `Requires-Python` in `pyproject.toml`
        } else {
            script.metadata.requires_python.map(|requires_python| {
                PythonRequest::Version(VersionRequest::Range(requires_python))
            })
        };

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);

        let interpreter = PythonInstallation::find_or_download(
            python_request,
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&download_reporter),
        )
        .await?
        .into_interpreter();

        // Install the script requirements, if necessary. Otherwise, use an isolated environment.
        if let Some(dependencies) = script.metadata.dependencies {
            // // Collect any `tool.uv.sources` from the script.
            let empty = BTreeMap::default();
            let script_sources = script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .unwrap_or(&empty);
            let script_dir = script.path.parent().expect("script path has no parent");

            let requirements = dependencies
                .into_iter()
                .map(|requirement| {
                    LoweredRequirement::from_non_workspace_requirement(
                        requirement,
                        script_dir,
                        script_sources,
                    )
                    .map(LoweredRequirement::into_inner)
                })
                .collect::<Result<_, _>>()?;
            let spec = RequirementsSpecification::from_requirements(requirements);
            let result = CachedEnvironment::get_or_create(
                spec,
                interpreter,
                &settings,
                &state,
                if show_resolution {
                    Box::new(DefaultResolveLogger)
                } else {
                    Box::new(SummaryResolveLogger)
                },
                if show_resolution {
                    Box::new(DefaultInstallLogger)
                } else {
                    Box::new(SummaryInstallLogger)
                },
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await;

            let environment = match result {
                Ok(resolution) => resolution,
                Err(ProjectError::Operation(operations::Error::Resolve(
                    uv_resolver::ResolveError::NoSolution(err),
                ))) => {
                    let report = miette::Report::msg(format!("{err}"))
                        .context(err.header().with_context("script"));
                    eprint!("{report:?}");
                    return Ok(ExitStatus::Failure);
                }
                Err(err) => return Err(err.into()),
            };

            Some(environment.into_interpreter())
        } else {
            // Create a virtual environment.
            temp_dir = cache.environment()?;
            let environment = uv_virtualenv::create_venv(
                temp_dir.path(),
                interpreter,
                uv_virtualenv::Prompt::None,
                false,
                false,
                false,
            )?;

            Some(environment.into_interpreter())
        }
    } else {
        None
    };

    // Discover and sync the base environment.
    let temp_dir;
    let base_interpreter = if let Some(script_interpreter) = script_interpreter {
        // If we found a PEP 723 script and the user provided a project-only setting, warn.
        if no_project {
            debug!(
                "`--no-project` is a no-op for Python scripts with inline metadata; ignoring..."
            );
        }
        if !extras.is_empty() {
            warn_user_once!("Extras are not supported for Python scripts with inline metadata");
        }
        if !dev {
            warn_user_once!("`--no-dev` is not supported for Python scripts with inline metadata");
        }
        if package.is_some() {
            warn_user_once!(
                "`--package` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if locked {
            warn_user_once!(
                "`--locked` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if frozen {
            warn_user_once!(
                "`--frozen` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if isolated {
            warn_user_once!(
                "`--isolated` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }

        script_interpreter
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

        let project = if no_project {
            // If the user runs with `--no-project` and we can't find a project, warn.
            if project.is_none() {
                debug!("`--no-project` was provided, but no project was found; ignoring...");
            }

            // If the user ran with `--no-project` and provided a project-only setting, warn.
            if !extras.is_empty() {
                warn_user_once!("Extras have no effect when used alongside `--no-project`");
            }
            if !dev {
                warn_user_once!("`--no-dev` has no effect when used alongside `--no-project`");
            }
            if locked {
                warn_user_once!("`--locked` has no effect when used alongside `--no-project`");
            }
            if frozen {
                warn_user_once!("`--frozen` has no effect when used alongside `--no-project`");
            }

            None
        } else {
            // If we can't find a project and the user provided a project-only setting, warn.
            if project.is_none() {
                if !extras.is_empty() {
                    warn_user_once!("Extras have no effect when used outside of a project");
                }
                if !dev {
                    warn_user_once!("`--no-dev` has no effect when used outside of a project");
                }
                if locked {
                    warn_user_once!("`--locked` has no effect when used outside of a project");
                }
                if frozen {
                    warn_user_once!("`--frozen` has no effect when used outside of a project");
                }
            }

            project
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
                debug!("Creating isolated virtual environment");

                // If we're isolating the environment, use an ephemeral virtual environment as the
                // base environment for the project.
                let interpreter = {
                    let client_builder = BaseClientBuilder::new()
                        .connectivity(connectivity)
                        .native_tls(native_tls);

                    // Resolve the Python request and requirement for the workspace.
                    let WorkspacePython { python_request, .. } = WorkspacePython::from_request(
                        python.as_deref().map(PythonRequest::parse),
                        project.workspace(),
                    )
                    .await?;

                    PythonInstallation::find_or_download(
                        python_request,
                        EnvironmentPreference::Any,
                        python_preference,
                        python_downloads,
                        &client_builder,
                        cache,
                        Some(&download_reporter),
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
                    python_downloads,
                    connectivity,
                    native_tls,
                    cache,
                    printer,
                )
                .await?
            };

            let result = match project::lock::do_safe_lock(
                locked,
                frozen,
                project.workspace(),
                venv.interpreter(),
                settings.as_ref().into(),
                if show_resolution {
                    Box::new(DefaultResolveLogger)
                } else {
                    Box::new(SummaryResolveLogger)
                },
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await
            {
                Ok(result) => result,
                Err(ProjectError::Operation(operations::Error::Resolve(
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
                result.lock(),
                &extras,
                dev,
                Modifications::Sufficient,
                settings.as_ref().into(),
                &state,
                if show_resolution {
                    Box::new(DefaultInstallLogger)
                } else {
                    Box::new(SummaryInstallLogger)
                },
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

            let interpreter = {
                let client_builder = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls);

                let python = PythonInstallation::find_or_download(
                    python.as_deref().map(PythonRequest::parse),
                    // No opt-in is required for system environments, since we are not mutating it.
                    EnvironmentPreference::Any,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&download_reporter),
                )
                .await?;

                python.into_interpreter()
            };

            if isolated {
                debug!("Creating isolated virtual environment");

                // If we're isolating the environment, use an ephemeral virtual environment.
                temp_dir = cache.environment()?;
                let venv = uv_virtualenv::create_venv(
                    temp_dir.path(),
                    interpreter,
                    uv_virtualenv::Prompt::None,
                    false,
                    false,
                    false,
                )?;
                venv.into_interpreter()
            } else {
                interpreter
            }
        };

        interpreter
    };

    debug!(
        "Using Python {} interpreter at: {}",
        base_interpreter.python_version(),
        base_interpreter.sys_executable().display()
    );

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

    // If necessary, create an environment for the ephemeral requirements or command.
    let temp_dir;
    let ephemeral_env = if can_skip_ephemeral(spec.as_ref(), &base_interpreter, &settings) {
        None
    } else {
        debug!("Creating ephemeral environment");

        Some(match spec.filter(|spec| !spec.is_empty()) {
            None => {
                // Create a virtual environment
                temp_dir = cache.environment()?;
                uv_virtualenv::create_venv(
                    temp_dir.path(),
                    base_interpreter.clone(),
                    uv_virtualenv::Prompt::None,
                    false,
                    false,
                    false,
                )?
            }
            Some(spec) => {
                debug!("Syncing ephemeral requirements");

                let result = CachedEnvironment::get_or_create(
                    spec,
                    base_interpreter.clone(),
                    &settings,
                    &state,
                    if show_resolution {
                        Box::new(DefaultResolveLogger)
                    } else {
                        Box::new(SummaryResolveLogger)
                    },
                    if show_resolution {
                        Box::new(DefaultInstallLogger)
                    } else {
                        Box::new(SummaryInstallLogger)
                    },
                    connectivity,
                    concurrency,
                    native_tls,
                    cache,
                    printer,
                )
                .await;

                let environment = match result {
                    Ok(resolution) => resolution,
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::NoSolution(err),
                    ))) => {
                        let report = miette::Report::msg(format!("{err}"))
                            .context(err.header().with_context("`--with`"));
                        eprint!("{report:?}");
                        return Ok(ExitStatus::Failure);
                    }
                    Err(err) => return Err(err.into()),
                };

                environment.into()
            }
        })
    };

    // If we're running in an ephemeral environment, add a `sitecustomize.py` to enable loading of
    // the base environment's site packages. Setting `PYTHONPATH` is insufficient, as it doesn't
    // resolve `.pth` files in the base environment.
    if let Some(ephemeral_env) = ephemeral_env.as_ref() {
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

    debug!("Running `{command}`");
    let mut process = Command::from(&command);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::scripts)
            .into_iter()
            .chain(std::iter::once(base_interpreter.scripts()))
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

/// Read a [`Pep723Script`] from the given command.
pub(crate) async fn parse_script(
    command: &ExternalCommand,
) -> Result<Option<Pep723Script>, Pep723Error> {
    // Parse the input command.
    let command = RunCommand::from(command);

    let RunCommand::Python(target, _) = &command else {
        return Ok(None);
    };

    // Read the PEP 723 `script` metadata from the target script.
    Pep723Script::read(&target).await
}

/// Returns `true` if we can skip creating an additional ephemeral environment in `uv run`.
fn can_skip_ephemeral(
    spec: Option<&RequirementsSpecification>,
    base_interpreter: &Interpreter,
    settings: &ResolverInstallerSettings,
) -> bool {
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
            debug!(
                "At least one requirement is not satisfied in the base environment: {requirement}"
            );
            false
        }
        Err(err) => {
            debug!("Failed to check requirements against base environment: {err}");
            false
        }
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

impl From<&ExternalCommand> for RunCommand {
    fn from(command: &ExternalCommand) -> Self {
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
