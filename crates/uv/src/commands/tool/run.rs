use std::ffi::OsString;
use std::fmt::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::{borrow::Cow, fmt::Display};

use anstream::eprint;
use anyhow::{bail, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::{debug, warn};

use distribution_types::{Name, UnresolvedRequirementSpecification};
use pep440_rs::Version;
use pypi_types::Requirement;
use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, PreviewMode};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_tool::{entrypoint_paths, InstalledTools};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations;
use crate::commands::project::{resolve_names, ProjectError};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{
    project::environment::CachedEnvironment, tool::common::matching_packages, tool_list,
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
    settings: ResolverInstallerSettings,
    invocation_source: ToolRunCommand,
    isolated: bool,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`{invocation_source}` is experimental and may change without warning");
    }

    // treat empty command as `uv tool list`
    let Some(command) = command else {
        return tool_list(false, PreviewMode::Enabled, cache, printer).await;
    };

    let (target, args) = command.split();
    let Some(target) = target else {
        return Err(anyhow::anyhow!("No tool command provided"));
    };

    let (target, from) = if let Some(from) = from {
        (Cow::Borrowed(target), Cow::Owned(from))
    } else {
        parse_target(target)?
    };

    // Get or create a compatible environment in which to execute the tool.
    let result = get_or_create_environment(
        &from,
        with,
        show_resolution,
        python.as_deref(),
        &settings,
        isolated,
        preview,
        python_preference,
        python_downloads,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await;

    let (from, environment) = match result {
        Ok(resolution) => resolution,
        Err(ProjectError::Operation(operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            let report =
                miette::Report::msg(format!("{err}")).context(err.header().with_context("tool"));
            eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // TODO(zanieb): Determine the executable command via the package entry points
    let executable = target;

    // Construct the command
    let mut process = Command::new(executable.as_ref());
    process.args(args);

    // Construct the `PATH` environment variable.
    let new_path = std::env::join_paths(
        std::iter::once(environment.scripts().to_path_buf()).chain(
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
    let space = if args.is_empty() { "" } else { " " };
    debug!(
        "Running `{}{space}{}`",
        executable.to_string_lossy(),
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );

    let site_packages = SitePackages::from_environment(&environment)?;

    // We check if the provided command is not part of the executables for the `from` package.
    // If the command is found in other packages, we warn the user about the correct package to use.

    warn_executable_not_provided_by_package(
        &executable.to_string_lossy(),
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
                        executable.to_string_lossy().cyan(),
                    )?;
                    if entrypoints.is_empty() {
                        warn_user!(
                            "Package `{}` does not provide any executables.",
                            from.name.red()
                        );
                    } else {
                        warn_user!(
                            "An executable named `{}` is not provided by package `{}`.",
                            executable.to_string_lossy().cyan(),
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
    .with_context(|| format!("Failed to spawn: `{}`", executable.to_string_lossy()))?;

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
                    .map(distribution_types::Name::name)
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

/// Parse a target into a command name and a requirement.
fn parse_target(target: &OsString) -> anyhow::Result<(Cow<OsString>, Cow<str>)> {
    let Some(target_str) = target.to_str() else {
        return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name."));
    };

    // e.g. uv, no special handling
    let Some((name, version)) = target_str.split_once('@') else {
        return Ok((Cow::Borrowed(target), Cow::Borrowed(target_str)));
    };

    // e.g. `uv@`, warn and treat the whole thing as the command
    if version.is_empty() {
        debug!("Ignoring empty version request in command");
        return Ok((Cow::Borrowed(target), Cow::Borrowed(target_str)));
    }

    // e.g. ignore `git+https://github.com/uv/uv.git@main`
    if PackageName::from_str(name).is_err() {
        debug!("Ignoring non-package name `{name}` in command");
        return Ok((Cow::Borrowed(target), Cow::Borrowed(target_str)));
    }

    // e.g. `uv@0.1.0`, convert to `uv==0.1.0`
    if let Ok(version) = Version::from_str(version) {
        return Ok((
            Cow::Owned(OsString::from(name)),
            Cow::Owned(format!("{name}=={version}")),
        ));
    }

    // e.g. `uv@invalid`, warn and treat the whole thing as the command
    debug!("Ignoring invalid version request `{version}` in command");
    Ok((Cow::Borrowed(target), Cow::Borrowed(target_str)))
}

/// Get or create a [`PythonEnvironment`] in which to run the specified tools.
///
/// If the target tool is already installed in a compatible environment, returns that
/// [`PythonEnvironment`]. Otherwise, gets or creates a [`CachedEnvironment`].
async fn get_or_create_environment(
    from: &str,
    with: &[RequirementsSource],
    show_resolution: bool,
    python: Option<&str>,
    settings: &ResolverInstallerSettings,
    isolated: bool,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<(Requirement, PythonEnvironment), ProjectError> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.map(PythonRequest::parse);

    // Discover an interpreter.
    let interpreter = PythonInstallation::find_or_download(
        python_request.clone(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        &client_builder,
        cache,
        Some(&reporter),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the `from` requirement.
    let from = {
        resolve_names(
            vec![RequirementsSpecification::parse_package(from)?],
            &interpreter,
            settings,
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?
        .pop()
        .unwrap()
    };

    // Read the `--with` requirements.
    let spec = {
        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);
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
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?,
        );
        requirements
    };

    // Check if the tool is already installed in a compatible environment.
    if !isolated && settings.reinstall.is_none() && settings.upgrade.is_none() {
        let installed_tools = InstalledTools::from_settings()?.init()?;
        let _lock = installed_tools.acquire_lock()?;

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
                site_packages.satisfies(&requirements, &constraints),
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
        spec,
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
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok((from, environment.into()))
}
