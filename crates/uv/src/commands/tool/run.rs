use std::fmt::Display;
use std::fmt::Write;
use std::path::PathBuf;
use std::str::FromStr;

use anstream::eprint;
use anyhow::{bail, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::{debug, warn};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, TrustedHost};
use uv_distribution_types::{Name, UnresolvedRequirementSpecification};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_pypi_types::{Requirement, RequirementSource};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_tool::{entrypoint_paths, InstalledTools};
use uv_warnings::warn_user;

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::project::{resolve_names, EnvironmentSpecification, ProjectError};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::Target;
use crate::commands::{
    diagnostics, project::environment::CachedEnvironment, tool::common::matching_packages,
};
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// The user-facing command used to invoke a tool run.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ToolRunCommand {
    /// via the `uvx` alias
    Uvx,
    /// via `uv tool run`
    ToolRun,
}

impl Display for ToolRunCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolRunCommand::Uvx => write!(f, "uvx"),
            ToolRunCommand::ToolRun => write!(f, "uv tool run"),
        }
    }
}

/// Run a command.
pub(crate) async fn run(
    command: Option<ExternalCommand>,
    from: Option<String>,
    with: &[RequirementsSource],
    show_resolution: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    invocation_source: ToolRunCommand,
    isolated: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let Some(command) = command else {
        // When a command isn't provided, we'll show a brief help including available tools
        show_help(invocation_source, &cache, printer).await?;
        // Exit as Clap would after displaying help
        return Ok(ExitStatus::Error);
    };

    let (target, args) = command.split();
    let Some(target) = target else {
        return Err(anyhow::anyhow!("No tool command provided"));
    };

    let Some(target) = target.to_str() else {
        return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name."));
    };

    let target = Target::parse(target, from.as_deref());

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if target.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Get or create a compatible environment in which to execute the tool.
    let result = get_or_create_environment(
        &target,
        with,
        show_resolution,
        python.as_deref(),
        install_mirrors,
        &settings,
        isolated,
        python_preference,
        python_downloads,
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        &cache,
        printer,
    )
    .await;

    let (from, environment) = match result {
        Ok(resolution) => resolution,
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::with_context("tool")
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(ProjectError::Requirements(err)) => {
            let err = miette::Report::msg(format!("{err}"))
                .context("Failed to resolve `--with` requirement");
            eprint!("{err:?}");
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // TODO(zanieb): Determine the executable command via the package entry points
    let executable = target.executable();

    // Construct the command
    let mut process = Command::new(executable);
    process.args(args);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        std::iter::once(environment.scripts().to_path_buf()).chain(
            std::env::var_os(EnvVars::PATH)
                .as_ref()
                .iter()
                .flat_map(std::env::split_paths),
        ),
    )?;
    process.env(EnvVars::PATH, new_path);

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let space = if args.is_empty() { "" } else { " " };
    debug!(
        "Running `{}{space}{}`",
        executable,
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );

    let site_packages = SitePackages::from_environment(&environment)?;

    // We check if the provided command is not part of the executables for the `from` package.
    // If the command is found in other packages, we warn the user about the correct package to use.
    warn_executable_not_provided_by_package(
        executable,
        &from.name,
        &site_packages,
        invocation_source,
    );

    let mut handle = match process.spawn() {
        Ok(handle) => Ok(handle),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            match get_entrypoints(&from.name, &site_packages) {
                Ok(entrypoints) => {
                    writeln!(
                        printer.stdout(),
                        "The executable `{}` was not found.",
                        executable.cyan(),
                    )?;
                    if entrypoints.is_empty() {
                        warn_user!(
                            "Package `{}` does not provide any executables.",
                            from.name.red()
                        );
                    } else {
                        warn_user!(
                            "An executable named `{}` is not provided by package `{}`.",
                            executable.cyan(),
                            from.name.red()
                        );
                        writeln!(
                            printer.stdout(),
                            "The following executables are provided by `{}`:",
                            from.name.green()
                        )?;
                        for (name, _) in entrypoints {
                            writeln!(printer.stdout(), "- {}", name.cyan())?;
                        }
                        let suggested_command = format!(
                            "{} --from {} <EXECUTABLE_NAME>",
                            invocation_source, from.name
                        );
                        writeln!(
                            printer.stdout(),
                            "Consider using `{}` instead.",
                            suggested_command.green()
                        )?;
                    }
                    return Ok(ExitStatus::Failure);
                }
                Err(err) => {
                    warn!("Failed to get entrypoints for `{from}`: {err}");
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
    .with_context(|| format!("Failed to spawn: `{executable}`"))?;

    // Ignore signals in the parent process, deferring them to the child. This is safe as long as
    // the command is the last thing that runs in this process; otherwise, we'd need to restore the
    // signal handlers after the command completes.
    let _handler = tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });

    // Exit based on the result of the command.
    #[cfg(unix)]
    let status = {
        use tokio::select;
        use tokio::signal::unix::{signal, SignalKind};

        let mut term_signal = signal(SignalKind::terminate())?;
        loop {
            select! {
                result = handle.wait() => {
                    break result;
                },

                // `SIGTERM`
                _ = term_signal.recv() => {
                    let _ = terminate_process(&mut handle);
                }
            };
        }
    }?;

    #[cfg(not(unix))]
    let status = handle.wait().await?;

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

#[cfg(unix)]
fn terminate_process(child: &mut tokio::process::Child) -> anyhow::Result<()> {
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;

    let pid = child.id().context("Failed to get child process ID")?;
    signal::kill(Pid::from_raw(pid.try_into()?), Signal::SIGTERM).context("Failed to send SIGTERM")
}

/// Return the entry points for the specified package.
fn get_entrypoints(
    from: &PackageName,
    site_packages: &SitePackages,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let installed = site_packages.get_packages(from);
    let Some(installed_dist) = installed.first().copied() else {
        bail!("Expected at least one requirement")
    };

    Ok(entrypoint_paths(
        site_packages,
        installed_dist.name(),
        installed_dist.version(),
    )?)
}

/// Display a list of tools that provide the executable.
///
/// If there is no package providing the executable, we will display a message to how to install a package.
async fn show_help(
    invocation_source: ToolRunCommand,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    let help = format!(
        "See `{}` for more information.",
        format!("{invocation_source} --help").bold()
    );

    writeln!(
        printer.stdout(),
        "Provide a command to run with `{}`.\n",
        format!("{invocation_source} <command>").bold()
    )?;

    let installed_tools = InstalledTools::from_settings()?;
    let _lock = match installed_tools.lock().await {
        Ok(lock) => lock,
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            writeln!(printer.stdout(), "{help}")?;
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };

    let tools = installed_tools
        .tools()?
        .into_iter()
        // Skip invalid tools
        .filter_map(|(name, tool)| {
            tool.ok().and_then(|_| {
                installed_tools
                    .version(&name, cache)
                    .ok()
                    .map(|version| (name, version))
            })
        })
        .sorted_by(|(name1, ..), (name2, ..)| name1.cmp(name2))
        .collect::<Vec<_>>();

    // No tools installed or they're all malformed
    if tools.is_empty() {
        writeln!(printer.stdout(), "{help}")?;
        return Ok(());
    }

    // Display the tools
    writeln!(printer.stdout(), "The following tools are installed:\n")?;
    for (name, version) in tools {
        writeln!(
            printer.stdout(),
            "- {} v{version}",
            format!("{name}").bold()
        )?;
    }

    writeln!(printer.stdout(), "\n{help}")?;

    Ok(())
}

/// Display a warning if an executable is not provided by package.
///
/// If found in a dependency of the requested package instead of the requested package itself, we will hint to use that instead.
fn warn_executable_not_provided_by_package(
    executable: &str,
    from_package: &PackageName,
    site_packages: &SitePackages,
    invocation_source: ToolRunCommand,
) {
    let packages = matching_packages(executable, site_packages);
    if !packages
        .iter()
        .any(|package| package.name() == from_package)
    {
        match packages.as_slice() {
            [] => {}
            [package] => {
                let suggested_command = format!(
                    "{invocation_source} --from {} {}",
                    package.name(),
                    executable
                );
                warn_user!(
                    "An executable named `{}` is not provided by package `{}` but is available via the dependency `{}`. Consider using `{}` instead.",
                    executable.cyan(),
                    from_package.cyan(),
                    package.name().cyan(),
                    suggested_command.green()
                );
            }
            packages => {
                let suggested_command = format!("{invocation_source} --from PKG {executable}");
                let provided_by = packages
                    .iter()
                    .map(uv_distribution_types::Name::name)
                    .map(|name| format!("- {}", name.cyan()))
                    .join("\n");
                warn_user!(
                    "An executable named `{}` is not provided by package `{}` but is available via the following dependencies:\n- {}\nConsider using `{}` instead.",
                    executable.cyan(),
                    from_package.cyan(),
                    provided_by,
                    suggested_command.green(),
                );
            }
        }
    }
}

/// Get or create a [`PythonEnvironment`] in which to run the specified tools.
///
/// If the target tool is already installed in a compatible environment, returns that
/// [`PythonEnvironment`]. Otherwise, gets or creates a [`CachedEnvironment`].
async fn get_or_create_environment(
    target: &Target<'_>,
    with: &[RequirementsSource],
    show_resolution: bool,
    python: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverInstallerSettings,
    isolated: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<(Requirement, PythonEnvironment), ProjectError> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec());

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.map(PythonRequest::parse);

    // Discover an interpreter.
    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        &client_builder,
        cache,
        Some(&reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the `--from` requirement.
    let from = match target {
        // Ex) `ruff`
        Target::Unspecified(name) => Requirement {
            name: PackageName::from_str(name)?,
            extras: vec![],
            marker: MarkerTree::default(),
            source: RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: None,
                conflict: None,
            },
            origin: None,
        },
        // Ex) `ruff@0.6.0`
        Target::Version(name, version) | Target::FromVersion(_, name, version) => Requirement {
            name: PackageName::from_str(name)?,
            extras: vec![],
            marker: MarkerTree::default(),
            source: RequirementSource::Registry {
                specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(
                    version.clone(),
                )),
                index: None,
                conflict: None,
            },
            origin: None,
        },
        // Ex) `ruff@latest`
        Target::Latest(name) | Target::FromLatest(_, name) => Requirement {
            name: PackageName::from_str(name)?,
            extras: vec![],
            marker: MarkerTree::default(),
            source: RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: None,
                conflict: None,
            },
            origin: None,
        },
        // Ex) `ruff>=0.6.0`
        Target::From(_, from) => resolve_names(
            vec![RequirementsSpecification::parse_package(from)?],
            &interpreter,
            settings,
            &state,
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?
        .pop()
        .unwrap(),
    };

    // Read the `--with` requirements.
    let spec = {
        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls)
            .allow_insecure_host(allow_insecure_host.to_vec());
        RequirementsSpecification::from_simple_sources(with, &client_builder).await?
    };

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(from.clone());
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                settings,
                &state,
                connectivity,
                concurrency,
                native_tls,
                allow_insecure_host,
                cache,
                printer,
            )
            .await?,
        );
        requirements
    };

    // Check if the tool is already installed in a compatible environment.
    if !isolated && !target.is_latest() {
        let installed_tools = InstalledTools::from_settings()?.init()?;
        let _lock = installed_tools.lock().await?;

        let existing_environment =
            installed_tools
                .get_environment(&from.name, cache)?
                .filter(|environment| {
                    python_request.as_ref().map_or(true, |python_request| {
                        python_request.satisfied(environment.interpreter(), cache)
                    })
                });
        if let Some(environment) = existing_environment {
            // Check if the installed packages meet the requirements.
            let site_packages = SitePackages::from_environment(&environment)?;

            let requirements = requirements
                .iter()
                .cloned()
                .map(UnresolvedRequirementSpecification::from)
                .collect::<Vec<_>>();
            let constraints = [];

            if matches!(
                site_packages.satisfies(
                    &requirements,
                    &constraints,
                    &interpreter.resolver_marker_environment()
                ),
                Ok(SatisfiesResult::Fresh { .. })
            ) {
                debug!("Using existing tool `{}`", from.name);
                return Ok((from, environment));
            }
        }
    }

    // Create a `RequirementsSpecification` from the resolved requirements, to avoid re-resolving.
    let spec = RequirementsSpecification {
        requirements: requirements
            .into_iter()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    };

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool.
    // TODO(zanieb): Determine if we should layer on top of the project environment if it is present.

    let environment = CachedEnvironment::get_or_create(
        EnvironmentSpecification::from(spec),
        interpreter,
        settings,
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
    .await?;

    Ok((from, environment.into()))
}
