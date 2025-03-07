use std::collections::BTreeMap;
use std::fmt::Display;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anstream::eprint;
use anyhow::{bail, Context};
use console::Term;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tracing::{debug, warn};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::ExternalCommand;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, PreviewMode};
use uv_distribution_types::{
    IndexUrl, Name, NameRequirementSpecification, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_pypi_types::{Requirement, RequirementSource};
use uv_python::VersionRequest;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_shell::runnable::WindowsRunnable;
use uv_static::EnvVars;
use uv_tool::{entrypoint_paths, InstalledTools};
use uv_warnings::warn_user;
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::project::{
    resolve_names, EnvironmentSpecification, PlatformState, ProjectError,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::run::run_to_completion;
use crate::commands::tool::common::{matching_packages, refine_interpreter};
use crate::commands::tool::{Target, ToolRequest};
use crate::commands::ExitStatus;
use crate::commands::{diagnostics, project::environment::CachedEnvironment};
use crate::printer::Printer;
use crate::settings::NetworkSettings;
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
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn run(
    command: Option<ExternalCommand>,
    from: Option<String>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    show_resolution: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    invocation_source: ToolRunCommand,
    isolated: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: Cache,
    printer: Printer,
    preview: PreviewMode,
) -> anyhow::Result<ExitStatus> {
    /// Whether or not a path looks like a Python script based on the file extension.
    fn has_python_script_ext(path: &Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyw"))
    }

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
        return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name"));
    };

    if let Some(ref from) = from {
        if has_python_script_ext(Path::new(from)) {
            let package_name = PackageName::from_str(from)?;
            return Err(anyhow::anyhow!(
                "It looks you provided a Python script to `--from`, which is not supported\n\n{}{} If you meant to run a command from the `{}` package, use the normalized package name instead to disambiguate, e.g., `{}`",
                "hint".bold().cyan(),
                ":".bold(),
                package_name.cyan(),
                format!("{} --from {} {}", invocation_source, package_name.cyan(), target).green(),
            ));
        }
    } else {
        let target_path = Path::new(target);

        // If the user tries to invoke `uvx script.py`, hint them towards `uv run`.
        if has_python_script_ext(target_path) {
            return if target_path.try_exists()? {
                Err(anyhow::anyhow!(
                    "It looks you tried to run a Python script at `{}`, which is not supported by `{}`\n\n{}{} Use `{}` instead",
                    target_path.user_display(),
                    invocation_source,
                    "hint".bold().cyan(),
                    ":".bold(),
                    format!("uv run {}", target_path.user_display().cyan()),
                ))
            } else {
                let package_name = PackageName::from_str(target)?;
                Err(anyhow::anyhow!(
                    "It looks you provided a Python script to run, which is not supported supported by `{}`\n\n{}{} We did not find a script at the requested path. If you meant to run a command from the `{}` package, pass the normalized package name to `--from` to disambiguate, e.g., `{}`",
                    invocation_source,
                    "hint".bold().cyan(),
                    ":".bold(),
                    package_name.cyan(),
                    format!("{invocation_source} --from {package_name} {target}").green(),
                ))
            };
        }
    }

    // If the user tries to invoke `uvx run ruff`, hint them towards `uvx ruff`, but only if
    // the `run` package is guaranteed to come from PyPI.
    let (mut target, mut args) = (target, args);
    if from.is_none()
        && invocation_source == ToolRunCommand::Uvx
        && target == "run"
        && settings
            .resolver
            .index_locations
            .indexes()
            .all(|index| matches!(index.url, IndexUrl::Pypi(..)))
    {
        let term = Term::stderr();
        if term.is_term() {
            let rest = args.iter().map(|s| s.to_string_lossy()).join(" ");
            let prompt = format!(
                "`{}` invokes the `{}` package. Did you mean `{}`?",
                format!("uvx run {rest}").green(),
                "run".cyan(),
                format!("uvx {rest}").green()
            );
            let confirmation = uv_console::confirm(&prompt, &term, true)?;
            if confirmation {
                let Some((next_target, next_args)) = args.split_first() else {
                    return Err(anyhow::anyhow!("No tool command provided"));
                };
                let Some(next_target) = next_target.to_str() else {
                    return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name"));
                };
                target = next_target;
                args = next_args;
            }
        }
    }

    let request = ToolRequest::parse(target, from.as_deref());

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if request.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Get or create a compatible environment in which to execute the tool.
    let result = get_or_create_environment(
        &request,
        with,
        constraints,
        overrides,
        show_resolution,
        python.as_deref(),
        install_mirrors,
        options,
        &settings,
        &network_settings,
        isolated,
        python_preference,
        python_downloads,
        installer_metadata,
        concurrency,
        &cache,
        printer,
        preview,
    )
    .await;

    let (from, environment) = match result {
        Ok(resolution) => resolution,
        Err(ProjectError::Operation(err)) => {
            // If the user ran `uvx run ...`, the `run` is likely a mistake. Show a dedicated hint.
            if from.is_none() && invocation_source == ToolRunCommand::Uvx && target == "run" {
                let rest = args.iter().map(|s| s.to_string_lossy()).join(" ");
                return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                    .with_hint(format!(
                        "`{}` invokes the `{}` package. Did you mean `{}`?",
                        format!("uvx run {rest}").green(),
                        "run".cyan(),
                        format!("uvx {rest}").green()
                    ))
                    .with_context("tool")
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }

            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .with_context("tool")
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
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
    let executable = from.executable();

    // Construct the command
    let mut process = if cfg!(windows) {
        WindowsRunnable::from_script_path(environment.scripts(), executable.as_ref()).into()
    } else {
        Command::new(executable)
    };

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
    match &from {
        ToolRequirement::Python => {}
        ToolRequirement::Package {
            requirement: from, ..
        } => {
            warn_executable_not_provided_by_package(
                executable,
                &from.name,
                &site_packages,
                invocation_source,
            );
        }
    }

    let handle = match process.spawn() {
        Ok(handle) => Ok(handle),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if let Some(exit_status) = hint_on_not_found(
                executable,
                &from,
                &site_packages,
                invocation_source,
                printer,
            )? {
                return Ok(exit_status);
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
    .with_context(|| format!("Failed to spawn: `{executable}`"))?;

    run_to_completion(handle).await
}

/// Show a hint when a command fails due to a missing executable.
///
/// Returns an exit status if the caller should exit after hinting.
fn hint_on_not_found(
    executable: &str,
    from: &ToolRequirement,
    site_packages: &SitePackages,
    invocation_source: ToolRunCommand,
    printer: Printer,
) -> anyhow::Result<Option<ExitStatus>> {
    let from = match from {
        ToolRequirement::Python => return Ok(None),
        ToolRequirement::Package {
            requirement: from, ..
        } => from,
    };
    match get_entrypoints(&from.name, site_packages) {
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
            Ok(Some(ExitStatus::Failure))
        }
        Err(err) => {
            warn!("Failed to get entrypoints for `{from}`: {err}");
            Ok(None)
        }
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

// Clippy isn't happy about the difference in size between these variants, but
// [`ToolRequirement::Package`] is the more common case and it seems annoying to box it.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ToolRequirement {
    Python,
    Package {
        executable: String,
        requirement: Requirement,
    },
}

impl ToolRequirement {
    fn executable(&self) -> &str {
        match self {
            ToolRequirement::Python => "python",
            ToolRequirement::Package { executable, .. } => executable,
        }
    }
}

impl std::fmt::Display for ToolRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolRequirement::Python => write!(f, "python"),
            ToolRequirement::Package { requirement, .. } => write!(f, "{requirement}"),
        }
    }
}

/// Get or create a [`PythonEnvironment`] in which to run the specified tools.
///
/// If the target tool is already installed in a compatible environment, returns that
/// [`PythonEnvironment`]. Otherwise, gets or creates a [`CachedEnvironment`].
#[allow(clippy::fn_params_excessive_bools)]
async fn get_or_create_environment(
    request: &ToolRequest<'_>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    show_resolution: bool,
    python: Option<&str>,
    install_mirrors: PythonInstallMirrors,
    options: ResolverInstallerOptions,
    settings: &ResolverInstallerSettings,
    network_settings: &NetworkSettings,
    isolated: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<(ToolRequirement, PythonEnvironment), ProjectError> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    let reporter = PythonDownloadReporter::single(printer);

    // Check if the target is `python`
    let python_request = if request.is_python() {
        let target_request = match &request.target {
            Target::Unspecified(_) => None,
            Target::Version(_, _, _, version) => Some(PythonRequest::Version(
                VersionRequest::from_str(&version.to_string()).map_err(anyhow::Error::from)?,
            )),
            // TODO(zanieb): Add `PythonRequest::Latest`
            Target::Latest(_, _, _) => {
                return Err(anyhow::anyhow!(
                    "Requesting the 'latest' Python version is not yet supported"
                )
                .into())
            }
        };

        if let Some(target_request) = &target_request {
            if let Some(python) = python {
                return Err(anyhow::anyhow!(
                    "Received multiple Python version requests: `{}` and `{}`",
                    python.to_string().cyan(),
                    target_request.to_canonical_string().cyan(),
                )
                .into());
            }
        }

        target_request.or_else(|| python.map(PythonRequest::parse))
    } else {
        python.map(PythonRequest::parse)
    };

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
    let state = PlatformState::default();
    let workspace_cache = WorkspaceCache::default();

    let from = if request.is_python() {
        ToolRequirement::Python
    } else {
        let (executable, requirement) = match &request.target {
            // Ex) `ruff>=0.6.0`
            Target::Unspecified(requirement) => {
                let spec = RequirementsSpecification::parse_package(requirement)?;

                // Extract the verbatim executable name, if possible.
                let name = match &spec.requirement {
                    UnresolvedRequirement::Named(..) => {
                        // Identify the package name from the PEP 508 specifier.
                        //
                        // For example, given `ruff>=0.6.0`, extract `ruff`, to use as the executable name.
                        let content = requirement.trim();
                        let index = content
                            .find(|c| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.'))
                            .unwrap_or(content.len());
                        Some(&content[..index])
                    }
                    UnresolvedRequirement::Unnamed(..) => None,
                };

                if let UnresolvedRequirement::Named(requirement) = &spec.requirement {
                    if requirement.name.as_str() == "python" {
                        return Err(anyhow::anyhow!(
                            "Using `{}` is not supported. Use `{}` instead.",
                            "--from python<specifier>".cyan(),
                            "python@<version>".cyan(),
                        )
                        .into());
                    }
                }

                let requirement = resolve_names(
                    vec![spec],
                    &interpreter,
                    settings,
                    network_settings,
                    &state,
                    concurrency,
                    cache,
                    &workspace_cache,
                    printer,
                    preview,
                )
                .await?
                .pop()
                .unwrap();

                // Prefer, in order:
                // 1. The verbatim executable provided by the user, independent of the requirement (as in: `uvx --from package executable`).
                // 2. The verbatim executable provided by the user as a named requirement (as in: `uvx change_wheel_version`).
                // 3. The resolved package name (as in: `uvx git+https://github.com/pallets/flask`).
                let executable = request
                    .executable
                    .map(ToString::to_string)
                    .or_else(|| name.map(ToString::to_string))
                    .unwrap_or_else(|| requirement.name.to_string());

                (executable, requirement)
            }
            // Ex) `ruff@0.6.0`
            Target::Version(executable, name, extras, version) => {
                let executable = request
                    .executable
                    .map(ToString::to_string)
                    .unwrap_or_else(|| (*executable).to_string());
                let requirement = Requirement {
                    name: name.clone(),
                    extras: extras.clone(),
                    groups: vec![],
                    marker: MarkerTree::default(),
                    source: RequirementSource::Registry {
                        specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(
                            version.clone(),
                        )),
                        index: None,
                        conflict: None,
                    },
                    origin: None,
                };

                (executable, requirement)
            }
            // Ex) `ruff@latest`
            Target::Latest(executable, name, extras) => {
                let executable = request
                    .executable
                    .map(ToString::to_string)
                    .unwrap_or_else(|| (*executable).to_string());
                let requirement = Requirement {
                    name: name.clone(),
                    extras: extras.clone(),
                    groups: vec![],
                    marker: MarkerTree::default(),
                    source: RequirementSource::Registry {
                        specifier: VersionSpecifiers::empty(),
                        index: None,
                        conflict: None,
                    },
                    origin: None,
                };

                (executable, requirement)
            }
        };

        ToolRequirement::Package {
            executable,
            requirement,
        }
    };

    // Read the `--with` requirements.
    let spec = RequirementsSpecification::from_sources(
        with,
        constraints,
        overrides,
        BTreeMap::default(),
        &client_builder,
    )
    .await?;

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        match &from {
            ToolRequirement::Python => {}
            ToolRequirement::Package { requirement, .. } => requirements.push(requirement.clone()),
        }
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                settings,
                network_settings,
                &state,
                concurrency,
                cache,
                &workspace_cache,
                printer,
                preview,
            )
            .await?,
        );
        requirements
    };

    // Resolve the constraints.
    let constraints = spec
        .constraints
        .clone()
        .into_iter()
        .map(|constraint| constraint.requirement)
        .collect::<Vec<_>>();

    // Resolve the overrides.
    let overrides = resolve_names(
        spec.overrides.clone(),
        &interpreter,
        settings,
        network_settings,
        &state,
        concurrency,
        cache,
        &workspace_cache,
        printer,
        preview,
    )
    .await?;

    // Check if the tool is already installed in a compatible environment.
    if !isolated && !request.is_latest() {
        let installed_tools = InstalledTools::from_settings()?.init()?;
        let _lock = installed_tools.lock().await?;

        if let ToolRequirement::Package { requirement, .. } = &from {
            let existing_environment = installed_tools
                .get_environment(&requirement.name, cache)?
                .filter(|environment| {
                    python_request.as_ref().is_none_or(|python_request| {
                        python_request.satisfied(environment.interpreter(), cache)
                    })
                });

            // Check if the installed packages meet the requirements.
            if let Some(environment) = existing_environment {
                if installed_tools
                    .get_tool_receipt(&requirement.name)
                    .ok()
                    .flatten()
                    .is_some_and(|receipt| ToolOptions::from(options) == *receipt.options())
                {
                    // Check if the installed packages meet the requirements.
                    let site_packages = SitePackages::from_environment(&environment)?;
                    if matches!(
                        site_packages.satisfies_requirements(
                            requirements.iter(),
                            constraints.iter(),
                            overrides.iter(),
                            &interpreter.resolver_marker_environment()
                        ),
                        Ok(SatisfiesResult::Fresh { .. })
                    ) {
                        debug!("Using existing tool `{}`", requirement.name);
                        return Ok((from, environment));
                    }
                }
            }
        }
    }

    // Create a `RequirementsSpecification` from the resolved requirements, to avoid re-resolving.
    let spec = EnvironmentSpecification::from(RequirementsSpecification {
        requirements: requirements
            .into_iter()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        constraints: constraints
            .into_iter()
            .map(NameRequirementSpecification::from)
            .collect(),
        overrides: overrides
            .into_iter()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    });

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool.
    // TODO(zanieb): Determine if we should layer on top of the project environment if it is present.

    let result = CachedEnvironment::from_spec(
        spec.clone(),
        &interpreter,
        settings,
        network_settings,
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
        installer_metadata,
        concurrency,
        cache,
        printer,
        preview,
    )
    .await;

    let environment = match result {
        Ok(environment) => environment,
        Err(err) => match err {
            ProjectError::Operation(err) => {
                // If the resolution failed due to the discovered interpreter not satisfying the
                // `requires-python` constraint, we can try to refine the interpreter.
                //
                // For example, if we discovered a Python 3.8 interpreter on the user's machine,
                // but the tool requires Python 3.10 or later, we can try to download a
                // Python 3.10 interpreter and re-resolve.
                let Some(interpreter) = refine_interpreter(
                    &interpreter,
                    python_request.as_ref(),
                    &err,
                    &client_builder,
                    &reporter,
                    &install_mirrors,
                    python_preference,
                    python_downloads,
                    cache,
                )
                .await
                .ok()
                .flatten() else {
                    return Err(err.into());
                };

                debug!(
                    "Re-resolving with Python {} (`{}`)",
                    interpreter.python_version(),
                    interpreter.sys_executable().display()
                );

                CachedEnvironment::from_spec(
                    spec,
                    &interpreter,
                    settings,
                    network_settings,
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
                    installer_metadata,
                    concurrency,
                    cache,
                    printer,
                    preview,
                )
                .await?
            }
            err => return Err(err),
        },
    };

    // Clear any existing overlay.
    environment.clear_overlay()?;
    environment.clear_system_site_packages()?;

    Ok((from, environment.into()))
}
