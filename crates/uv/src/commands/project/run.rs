use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write;
use std::io::Read;
use std::path::{Path, PathBuf};

use anstream::eprint;
use anyhow::{anyhow, bail, Context};
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::{debug, warn};
use url::Url;
use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{
    Concurrency, DevMode, EditableMode, ExtrasSpecification, InstallOptions, LowerBound,
    SourceStrategy, TrustedHost,
};
use uv_distribution::LoweredRequirement;
use uv_fs::which::is_executable;
use uv_fs::{PythonExt, Simplified};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;

use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, PythonVersionFile, VersionRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::Lock;
use uv_scripts::Pep723Item;
use uv_static::EnvVars;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, InstallTarget, VirtualProject, Workspace, WorkspaceError};

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations;
use crate::commands::pip::operations::Modifications;
use crate::commands::project::environment::CachedEnvironment;
use crate::commands::project::{
    validate_requires_python, EnvironmentSpecification, ProjectError, PythonRequestSource,
    WorkspacePython,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{diagnostics, project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn run(
    project_dir: &Path,
    script: Option<Pep723Item>,
    command: Option<RunCommand>,
    requirements: Vec<RequirementsSource>,
    show_resolution: bool,
    locked: bool,
    frozen: bool,
    no_sync: bool,
    isolated: bool,
    package: Option<PackageName>,
    no_project: bool,
    no_config: bool,
    extras: ExtrasSpecification,
    dev: DevMode,
    editable: EditableMode,
    python: Option<String>,
    settings: ResolverInstallerSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
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

    // Initialize any shared state.
    let state = SharedState::default();

    // Initialize any output reporters.
    let download_reporter = PythonDownloadReporter::single(printer);

    // Determine whether the command to execute is a PEP 723 script.
    let temp_dir;
    let script_interpreter = if let Some(script) = script {
        match &script {
            Pep723Item::Script(script) => {
                writeln!(
                    printer.stderr(),
                    "Reading inline script metadata from `{}`",
                    script.path.user_display().cyan()
                )?;
            }
            Pep723Item::Stdin(_) => {
                writeln!(
                    printer.stderr(),
                    "Reading inline script metadata from `{}`",
                    "stdin".cyan()
                )?;
            }
            Pep723Item::Remote(_) => {
                writeln!(
                    printer.stderr(),
                    "Reading inline script metadata from {}",
                    "remote URL".cyan()
                )?;
            }
        }

        let (source, python_request) = if let Some(request) = python.as_deref() {
            // (1) Explicit request from user
            let source = PythonRequestSource::UserRequest;
            let request = Some(PythonRequest::parse(request));
            (source, request)
        } else if let Some(file) = PythonVersionFile::discover(&project_dir, false, false).await? {
            // (2) Request from `.python-version`
            let source = PythonRequestSource::DotPythonVersion(file.file_name().to_string());
            let request = file.into_version();
            (source, request)
        } else {
            // (3) `Requires-Python` in the script
            let request = script
                .metadata()
                .requires_python
                .as_ref()
                .map(|requires_python| {
                    PythonRequest::Version(VersionRequest::Range(
                        requires_python.clone(),
                        PythonVariant::Default,
                    ))
                });
            let source = PythonRequestSource::RequiresPython;
            (source, request)
        };

        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());

        let interpreter = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&download_reporter),
        )
        .await?
        .into_interpreter();

        if let Some(requires_python) = script.metadata().requires_python.as_ref() {
            if !requires_python.contains(interpreter.python_version()) {
                let err = match source {
                    PythonRequestSource::UserRequest => {
                        ProjectError::RequestedPythonScriptIncompatibility(
                            interpreter.python_version().clone(),
                            requires_python.clone(),
                        )
                    }
                    PythonRequestSource::DotPythonVersion(file) => {
                        ProjectError::DotPythonVersionScriptIncompatibility(
                            file,
                            interpreter.python_version().clone(),
                            requires_python.clone(),
                        )
                    }
                    PythonRequestSource::RequiresPython => {
                        ProjectError::RequiresPythonScriptIncompatibility(
                            interpreter.python_version().clone(),
                            requires_python.clone(),
                        )
                    }
                };
                warn_user!("{err}");
            }
        }

        // Determine the working directory for the script.
        let script_dir = match &script {
            Pep723Item::Script(script) => std::path::absolute(&script.path)?
                .parent()
                .expect("script path has no parent")
                .to_owned(),
            Pep723Item::Stdin(..) | Pep723Item::Remote(..) => std::env::current_dir()?,
        };
        let script = script.into_metadata();

        // Install the script requirements, if necessary. Otherwise, use an isolated environment.
        if let Some(dependencies) = script.dependencies {
            // Collect any `tool.uv.index` from the script.
            let empty = Vec::default();
            let script_indexes = match settings.sources {
                SourceStrategy::Enabled => script
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.indexes.as_deref())
                    .unwrap_or(&empty),
                SourceStrategy::Disabled => &empty,
            };

            // Collect any `tool.uv.sources` from the script.
            let empty = BTreeMap::default();
            let script_sources = match settings.sources {
                SourceStrategy::Enabled => script
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.sources.as_ref())
                    .unwrap_or(&empty),
                SourceStrategy::Disabled => &empty,
            };

            let requirements = dependencies
                .into_iter()
                .flat_map(|requirement| {
                    LoweredRequirement::from_non_workspace_requirement(
                        requirement,
                        script_dir.as_ref(),
                        script_sources,
                        script_indexes,
                        &settings.index_locations,
                        LowerBound::Allow,
                    )
                    .map_ok(LoweredRequirement::into_inner)
                })
                .collect::<Result<_, _>>()?;
            let spec = RequirementsSpecification::from_requirements(requirements);
            let result = CachedEnvironment::get_or_create(
                EnvironmentSpecification::from(spec),
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
                allow_insecure_host,
                cache,
                printer,
            )
            .await;

            let environment = match result {
                Ok(resolution) => resolution,
                Err(ProjectError::Operation(operations::Error::Resolve(
                    uv_resolver::ResolveError::NoSolution(err),
                ))) => {
                    diagnostics::no_solution_context(&err, "script");
                    return Ok(ExitStatus::Failure);
                }
                Err(ProjectError::Operation(operations::Error::Resolve(
                    uv_resolver::ResolveError::FetchAndBuild(dist, err),
                ))) => {
                    diagnostics::fetch_and_build(dist, err);
                    return Ok(ExitStatus::Failure);
                }
                Err(ProjectError::Operation(operations::Error::Resolve(
                    uv_resolver::ResolveError::Build(dist, err),
                ))) => {
                    diagnostics::build(dist, err);
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
                false,
            )?;

            Some(environment.into_interpreter())
        }
    } else {
        None
    };

    // The lockfile used for the base environment.
    let mut lock: Option<Lock> = None;

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
            warn_user!("Extras are not supported for Python scripts with inline metadata");
        }
        if matches!(dev, DevMode::Exclude) {
            warn_user!("`--no-dev` is not supported for Python scripts with inline metadata");
        }
        if matches!(dev, DevMode::Only) {
            warn_user!("`--only-dev` is not supported for Python scripts with inline metadata");
        }
        if package.is_some() {
            warn_user!(
                "`--package` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if locked {
            warn_user!(
                "`--locked` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if frozen {
            warn_user!(
                "`--frozen` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if no_sync {
            warn_user!(
                "`--no-sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if isolated {
            warn_user!(
                "`--isolated` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }

        script_interpreter
    } else {
        let project = if let Some(package) = package {
            // We need a workspace, but we don't need to have a current package, we can be e.g. in
            // the root of a virtual workspace and then switch into the selected package.
            Some(VirtualProject::Project(
                Workspace::discover(project_dir, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            ))
        } else {
            match VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await {
                Ok(project) => {
                    if no_project {
                        debug!("Ignoring discovered project due to `--no-project`");
                        None
                    } else {
                        Some(project)
                    }
                }
                Err(WorkspaceError::MissingPyprojectToml | WorkspaceError::NonWorkspace(_)) => {
                    // If the user runs with `--no-project` and we can't find a project, warn.
                    if no_project {
                        warn!("`--no-project` was provided, but no project was found");
                    }
                    None
                }
                Err(err) => {
                    // If the user runs with `--no-project`, ignore the error.
                    if no_project {
                        warn!("Ignoring project discovery error due to `--no-project`: {err}");
                        None
                    } else {
                        return Err(err.into());
                    }
                }
            }
        };

        if no_project {
            // If the user ran with `--no-project` and provided a project-only setting, warn.
            if !extras.is_empty() {
                warn_user!("Extras have no effect when used alongside `--no-project`");
            }
            if matches!(dev, DevMode::Exclude) {
                warn_user!("`--no-dev` has no effect when used alongside `--no-project`");
            }
            if matches!(dev, DevMode::Only) {
                warn_user!("`--only-dev` has no effect when used alongside `--no-project`");
            }
            if locked {
                warn_user!("`--locked` has no effect when used alongside `--no-project`");
            }
            if frozen {
                warn_user!("`--frozen` has no effect when used alongside `--no-project`");
            }
            if no_sync {
                warn_user!("`--no-sync` has no effect when used alongside `--no-project`");
            }
        } else if project.is_none() {
            // If we can't find a project and the user provided a project-only setting, warn.
            if !extras.is_empty() {
                warn_user!("Extras have no effect when used outside of a project");
            }
            if matches!(dev, DevMode::Exclude) {
                warn_user!("`--no-dev` has no effect when used outside of a project");
            }
            if matches!(dev, DevMode::Only) {
                warn_user!("`--only-dev` has no effect when used outside of a project");
            }
            if locked {
                warn_user!("`--locked` has no effect when used outside of a project");
            }
            if no_sync {
                warn_user!("`--no-sync` has no effect when used outside of a project");
            }
        }

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
                let client_builder = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls)
                    .allow_insecure_host(allow_insecure_host.to_vec());

                // Resolve the Python request and requirement for the workspace.
                let WorkspacePython {
                    source,
                    python_request,
                    requires_python,
                } = WorkspacePython::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    project.workspace(),
                )
                .await?;

                let interpreter = PythonInstallation::find_or_download(
                    python_request.as_ref(),
                    EnvironmentPreference::Any,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&download_reporter),
                )
                .await?
                .into_interpreter();

                if let Some(requires_python) = requires_python.as_ref() {
                    validate_requires_python(
                        &interpreter,
                        project.workspace(),
                        requires_python,
                        &source,
                    )?;
                }

                // Create a virtual environment
                temp_dir = cache.environment()?;
                uv_virtualenv::create_venv(
                    temp_dir.path(),
                    interpreter,
                    uv_virtualenv::Prompt::None,
                    false,
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
                    allow_insecure_host,
                    cache,
                    printer,
                )
                .await?
            };

            if no_sync {
                debug!("Skipping environment synchronization due to `--no-sync`");

                // If we're not syncing, we should still attempt to respect the locked preferences
                // in any `--with` requirements.
                if !isolated && !requirements.is_empty() {
                    lock = project::lock::read(project.workspace())
                        .await
                        .ok()
                        .flatten();
                }
            } else {
                let result = match project::lock::do_safe_lock(
                    locked,
                    frozen,
                    project.workspace(),
                    venv.interpreter(),
                    settings.as_ref().into(),
                    LowerBound::Allow,
                    &state,
                    if show_resolution {
                        Box::new(DefaultResolveLogger)
                    } else {
                        Box::new(SummaryResolveLogger)
                    },
                    connectivity,
                    concurrency,
                    native_tls,
                    allow_insecure_host,
                    cache,
                    printer,
                )
                .await
                {
                    Ok(result) => result,
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::NoSolution(err),
                    ))) => {
                        diagnostics::no_solution(&err);
                        return Ok(ExitStatus::Failure);
                    }
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::FetchAndBuild(dist, err),
                    ))) => {
                        diagnostics::fetch_and_build(dist, err);
                        return Ok(ExitStatus::Failure);
                    }
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::Build(dist, err),
                    ))) => {
                        diagnostics::build(dist, err);
                        return Ok(ExitStatus::Failure);
                    }
                    Err(err) => return Err(err.into()),
                };

                let install_options = InstallOptions::default();

                project::sync::do_sync(
                    InstallTarget::from(&project),
                    &venv,
                    result.lock(),
                    &extras,
                    dev,
                    editable,
                    install_options,
                    Modifications::Sufficient,
                    settings.as_ref().into(),
                    if show_resolution {
                        Box::new(DefaultInstallLogger)
                    } else {
                        Box::new(SummaryInstallLogger)
                    },
                    connectivity,
                    concurrency,
                    native_tls,
                    allow_insecure_host,
                    cache,
                    printer,
                )
                .await?;

                lock = Some(result.into_lock());
            }

            venv.into_interpreter()
        } else {
            debug!("No project found; searching for Python interpreter");

            let interpreter = {
                let client_builder = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls)
                    .allow_insecure_host(allow_insecure_host.to_vec());

                // (1) Explicit request from user
                let python_request = if let Some(request) = python.as_deref() {
                    Some(PythonRequest::parse(request))
                // (2) Request from `.python-version`
                } else {
                    PythonVersionFile::discover(&project_dir, no_config, false)
                        .await?
                        .and_then(PythonVersionFile::into_version)
                };

                let python = PythonInstallation::find_or_download(
                    python_request.as_ref(),
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
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());

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
                    false,
                )?
            }
            Some(spec) => {
                debug!("Syncing ephemeral requirements");

                let result = CachedEnvironment::get_or_create(
                    EnvironmentSpecification::from(spec).with_lock(lock.as_ref()),
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
                    allow_insecure_host,
                    cache,
                    printer,
                )
                .await;

                let environment = match result {
                    Ok(resolution) => resolution,
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::NoSolution(err),
                    ))) => {
                        diagnostics::no_solution_context(&err, "`--with`");
                        return Ok(ExitStatus::Failure);
                    }
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::FetchAndBuild(dist, err),
                    ))) => {
                        diagnostics::fetch_and_build(dist, err);
                        return Ok(ExitStatus::Failure);
                    }
                    Err(ProjectError::Operation(operations::Error::Resolve(
                        uv_resolver::ResolveError::Build(dist, err),
                    ))) => {
                        diagnostics::build(dist, err);
                        return Ok(ExitStatus::Failure);
                    }
                    Err(ProjectError::Operation(operations::Error::Requirements(err))) => {
                        let err = miette::Report::msg(format!("{err}"))
                            .context("Invalid `--with` requirement");
                        eprint!("{err:?}");
                        return Ok(ExitStatus::Failure);
                    }
                    Err(err) => return Err(err.into()),
                };

                environment.into()
            }
        })
    };

    // If we're running in an ephemeral environment, add a path file to enable loading of
    // the base environment's site packages. Setting `PYTHONPATH` is insufficient, as it doesn't
    // resolve `.pth` files in the base environment.
    // And `sitecustomize.py` would be an alternative but it can be shadowed by an existing such
    // module in the python installation.
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
            ephemeral_site_packages.join("_uv_ephemeral_overlay.pth"),
            format!(
                "import site; site.addsitedir(\"{}\")",
                base_site_packages.escape_for_python()
            ),
        )?;
    }

    // Determine the Python interpreter to use for the command, if necessary.
    let interpreter = ephemeral_env
        .as_ref()
        .map_or_else(|| &base_interpreter, |env| env.interpreter());

    // Check if any run command is given.
    // If not, print the available scripts for the current interpreter.
    let Some(command) = command else {
        writeln!(
            printer.stdout(),
            "Provide a command or script to invoke with `uv run <command>` or `uv run <script>.py`.\n"
        )?;

        #[allow(clippy::map_identity)]
        let commands = interpreter
            .scripts()
            .read_dir()
            .ok()
            .into_iter()
            .flatten()
            .map(|entry| match entry {
                Ok(entry) => Ok(entry),
                Err(err) => {
                    // If we can't read the entry, fail.
                    // This could be a symptom of a more serious problem.
                    warn!("Failed to read entry: {}", err);
                    Err(err)
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| {
                entry
                    .file_type()
                    .is_ok_and(|file_type| file_type.is_file() || file_type.is_symlink())
            })
            .map(|entry| entry.path())
            .filter(|path| is_executable(path))
            .map(|path| {
                if cfg!(windows) {
                    // Remove the extensions.
                    path.with_extension("")
                } else {
                    path
                }
            })
            .map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .filter(|command| {
                !command.starts_with("activate") && !command.starts_with("deactivate")
            })
            .sorted()
            .collect_vec();

        if !commands.is_empty() {
            writeln!(
                printer.stdout(),
                "The following commands are available in the environment:\n"
            )?;
            for command in commands {
                writeln!(printer.stdout(), "- {command}")?;
            }
        }
        let help = format!("See `{}` for more information.", "uv run --help".bold());
        writeln!(printer.stdout(), "\n{help}")?;
        return Ok(ExitStatus::Error);
    };

    debug!("Running `{command}`");
    let mut process = command.as_command(interpreter);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        ephemeral_env
            .as_ref()
            .map(PythonEnvironment::scripts)
            .into_iter()
            .chain(std::iter::once(base_interpreter.scripts()))
            .map(PathBuf::from)
            .chain(
                std::env::var_os(EnvVars::PATH)
                    .as_ref()
                    .iter()
                    .flat_map(std::env::split_paths),
            ),
    )?;
    process.env(EnvVars::PATH, new_path);

    // Ensure `VIRTUAL_ENV` is set.
    if interpreter.is_virtualenv() {
        process.env(EnvVars::VIRTUAL_ENV, interpreter.sys_prefix().as_os_str());
    };

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let mut handle = process
        .spawn()
        .with_context(|| format!("Failed to spawn: `{}`", command.display_executable()))?;

    // Ignore signals in the parent process, deferring them to the child. This is safe as long as
    // the command is the last thing that runs in this process; otherwise, we'd need to restore the
    // signal handlers after the command completes.
    let _handler = tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });

    let status = handle.wait().await.context("Child process disappeared")?;

    // Exit based on the result of the command
    if let Some(code) = status.code() {
        debug!("Command exited with code: {code}");
        if let Ok(code) = u8::try_from(code) {
            Ok(ExitStatus::External(code))
        } else {
            #[allow(clippy::exit)]
            std::process::exit(code);
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            debug!("Command exited with signal: {:?}", status.signal());
        }
        Ok(ExitStatus::Failure)
    }
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

    match site_packages.satisfies(
        &spec.requirements,
        &spec.constraints,
        &base_interpreter.resolver_markers(),
    ) {
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
pub(crate) enum RunCommand {
    /// Execute `python`.
    Python(Vec<OsString>),
    /// Execute a `python` script.
    PythonScript(PathBuf, Vec<OsString>),
    /// Search `sys.path` for the named module and execute its contents as the `__main__` module.
    /// Equivalent to `python -m module`.
    PythonModule(OsString, Vec<OsString>),
    /// Execute a `pythonw` script (Windows only).
    PythonGuiScript(PathBuf, Vec<OsString>),
    /// Execute a Python package containing a `__main__.py` file.
    PythonPackage(PathBuf, Vec<OsString>),
    /// Execute a Python [zipapp].
    /// [zipapp]: <https://docs.python.org/3/library/zipapp.html>
    PythonZipapp(PathBuf, Vec<OsString>),
    /// Execute a `python` script provided via `stdin`.
    PythonStdin(Vec<u8>),
    /// Execute a Python script provided via a remote URL.
    PythonRemote(tempfile::NamedTempFile, Vec<OsString>),
    /// Execute an external command.
    External(OsString, Vec<OsString>),
    /// Execute an empty command (in practice, `python` with no arguments).
    Empty,
}

impl RunCommand {
    /// Return the name of the target executable, for display purposes.
    fn display_executable(&self) -> Cow<'_, str> {
        match self {
            Self::Python(_)
            | Self::PythonScript(..)
            | Self::PythonPackage(..)
            | Self::PythonZipapp(..)
            | Self::PythonRemote(..)
            | Self::Empty => Cow::Borrowed("python"),
            Self::PythonModule(..) => Cow::Borrowed("python -m"),
            Self::PythonGuiScript(..) => Cow::Borrowed("pythonw"),
            Self::PythonStdin(_) => Cow::Borrowed("python -c"),
            Self::External(executable, _) => executable.to_string_lossy(),
        }
    }

    /// Convert a [`RunCommand`] into a [`Command`].
    fn as_command(&self, interpreter: &Interpreter) -> Command {
        match self {
            Self::Python(args) => {
                let mut process = Command::new(interpreter.sys_executable());
                process.args(args);
                process
            }
            Self::PythonScript(target, args)
            | Self::PythonPackage(target, args)
            | Self::PythonZipapp(target, args) => {
                let mut process = Command::new(interpreter.sys_executable());
                process.arg(target);
                process.args(args);
                process
            }
            Self::PythonRemote(target, args) => {
                let mut process = Command::new(interpreter.sys_executable());
                process.arg(target.path());
                process.args(args);
                process
            }
            Self::PythonModule(module, args) => {
                let mut process = Command::new(interpreter.sys_executable());
                process.arg("-m");
                process.arg(module);
                process.args(args);
                process
            }
            Self::PythonGuiScript(target, args) => {
                let python_executable = interpreter.sys_executable();

                // Use `pythonw.exe` if it exists, otherwise fall back to `python.exe`.
                // See `install-wheel-rs::get_script_executable`.gd
                let pythonw_executable = python_executable
                    .file_name()
                    .map(|name| {
                        let new_name = name.to_string_lossy().replace("python", "pythonw");
                        python_executable.with_file_name(new_name)
                    })
                    .filter(|path| path.is_file())
                    .unwrap_or_else(|| python_executable.to_path_buf());

                let mut process = Command::new(&pythonw_executable);
                process.arg(target);
                process.args(args);
                process
            }
            Self::PythonStdin(script) => {
                let mut process = Command::new(interpreter.sys_executable());
                process.arg("-c");

                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStringExt;
                    process.arg(OsString::from_vec(script.clone()));
                }

                #[cfg(not(unix))]
                {
                    let script = String::from_utf8(script.clone()).expect("script is valid UTF-8");
                    process.arg(script);
                }

                process
            }
            Self::External(executable, args) => {
                let mut process = Command::new(executable);
                process.args(args);
                process
            }
            Self::Empty => Command::new(interpreter.sys_executable()),
        }
    }
}

impl std::fmt::Display for RunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python(args) => {
                write!(f, "python")?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::PythonScript(target, args)
            | Self::PythonPackage(target, args)
            | Self::PythonZipapp(target, args) => {
                write!(f, "python {}", target.display())?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::PythonModule(module, args) => {
                write!(f, "python -m")?;
                write!(f, " {}", module.to_string_lossy())?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::PythonGuiScript(target, args) => {
                write!(f, "pythonw {}", target.display())?;
                for arg in args {
                    write!(f, " {}", arg.to_string_lossy())?;
                }
                Ok(())
            }
            Self::PythonStdin(..) | Self::PythonRemote(..) => {
                write!(f, "python -c")?;
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

impl RunCommand {
    /// Determine the [`RunCommand`] for a given set of arguments.
    pub(crate) async fn from_args(
        command: &ExternalCommand,
        module: bool,
        script: bool,
        connectivity: Connectivity,
        native_tls: bool,
        allow_insecure_host: &[TrustedHost],
    ) -> anyhow::Result<Self> {
        let (target, args) = command.split();
        let Some(target) = target else {
            return Ok(Self::Empty);
        };

        let target_path = PathBuf::from(target);

        // Determine whether the user provided a remote script.
        if target_path.starts_with("http://") || target_path.starts_with("https://") {
            // Only continue if we are absolutely certain no local file exists.
            //
            // We don't do this check on Windows since the file path would
            // be invalid anyway, and thus couldn't refer to a local file.
            if !cfg!(unix) || matches!(target_path.try_exists(), Ok(false)) {
                let url = Url::parse(&target.to_string_lossy())?;

                let file_stem = url
                    .path_segments()
                    .and_then(Iterator::last)
                    .and_then(|segment| segment.strip_suffix(".py"))
                    .unwrap_or("script");
                let file = tempfile::Builder::new()
                    .prefix(file_stem)
                    .suffix(".py")
                    .tempfile()?;

                let client = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls)
                    .allow_insecure_host(allow_insecure_host.to_vec())
                    .build();
                let response = client.for_host(&url).get(url.clone()).send().await?;

                // Stream the response to the file.
                let mut writer = file.as_file();
                let mut reader = response.bytes_stream();
                while let Some(chunk) = reader.next().await {
                    use std::io::Write;
                    writer.write_all(&chunk?)?;
                }

                return Ok(Self::PythonRemote(file, args.to_vec()));
            }
        }

        if module {
            return Ok(Self::PythonModule(target.clone(), args.to_vec()));
        } else if script {
            return Ok(Self::PythonScript(target.clone().into(), args.to_vec()));
        }

        let metadata = target_path.metadata();
        let is_file = metadata.as_ref().map_or(false, std::fs::Metadata::is_file);
        let is_dir = metadata.as_ref().map_or(false, std::fs::Metadata::is_dir);

        if target.eq_ignore_ascii_case("-") {
            let mut buf = Vec::with_capacity(1024);
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(Self::PythonStdin(buf))
        } else if target.eq_ignore_ascii_case("python") {
            Ok(Self::Python(args.to_vec()))
        } else if target_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyc"))
            && is_file
        {
            Ok(Self::PythonScript(target_path, args.to_vec()))
        } else if cfg!(windows)
            && target_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pyw"))
            && is_file
        {
            Ok(Self::PythonGuiScript(target_path, args.to_vec()))
        } else if is_dir && target_path.join("__main__.py").is_file() {
            Ok(Self::PythonPackage(target_path, args.to_vec()))
        } else if is_file && is_python_zipapp(&target_path) {
            Ok(Self::PythonZipapp(target_path, args.to_vec()))
        } else {
            Ok(Self::External(
                target.clone(),
                args.iter().map(std::clone::Clone::clone).collect(),
            ))
        }
    }
}

/// Returns `true` if the target is a ZIP archive containing a `__main__.py` file.
fn is_python_zipapp(target: &Path) -> bool {
    if let Ok(file) = fs_err::File::open(target) {
        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            return archive
                .by_name("__main__.py")
                .map_or(false, |f| f.is_file());
        }
    }
    false
}
