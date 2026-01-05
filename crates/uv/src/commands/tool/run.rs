use std::fmt::Display;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anstream::eprint;
use anyhow::{Context, bail};
use console::Term;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{Concurrency, Constraints, GitLfsSetting, TargetTriple};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::InstalledDist;
use uv_distribution_types::{
    IndexCapabilities, IndexUrl, Name, NameRequirementSpecification, Requirement,
    RequirementSource, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_preview::Preview;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_shell::runnable::WindowsRunnable;
use uv_static::EnvVars;
use uv_tool::{InstalledTools, entrypoint_paths};
use uv_warnings::warn_user;
use uv_warnings::warn_user_once;
use uv_workspace::WorkspaceCache;

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::pip;
use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryInstallLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations;
use crate::commands::project::{
    EnvironmentSpecification, PlatformState, ProjectError, resolve_names,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::{matching_packages, refine_interpreter};
use crate::commands::tool::{Target, ToolRequest};
use crate::commands::{diagnostics, project::environment::CachedEnvironment};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;
use crate::settings::ResolverSettings;

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
            Self::Uvx => write!(f, "uvx"),
            Self::ToolRun => write!(f, "uv tool run"),
        }
    }
}

/// Check if the given arguments contain a verbose flag (e.g., `--verbose`, `-v`, `-vv`, etc.)
fn find_verbose_flag(args: &[std::ffi::OsString]) -> Option<&str> {
    args.iter().find_map(|arg| {
        let arg_str = arg.to_str()?;
        if arg_str == "--verbose" {
            Some("--verbose")
        } else if arg_str.starts_with("-v") && arg_str.chars().skip(1).all(|c| c == 'v') {
            Some(arg_str)
        } else {
            None
        }
    })
}

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn run(
    command: Option<ExternalCommand>,
    from: Option<String>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    show_resolution: bool,
    lfs: GitLfsSetting,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    invocation_source: ToolRunCommand,
    isolated: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: Cache,
    printer: Printer,
    env_file: Vec<PathBuf>,
    no_env_file: bool,
    preview: Preview,
) -> anyhow::Result<ExitStatus> {
    /// Whether or not a path looks like a Python script based on the file extension.
    fn has_python_script_ext(path: &Path) -> bool {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyw"))
    }

    if settings.resolver.torch_backend.is_some() {
        warn_user_once!(
            "The `--torch-backend` option is experimental and may change without warning."
        );
    }

    // Read from the `.env` file, if necessary.
    if !no_env_file {
        for env_file_path in env_file.iter().rev().map(PathBuf::as_path) {
            match dotenvy::from_path(env_file_path) {
                Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                    bail!(
                        "No environment file found at: `{}`",
                        env_file_path.simplified_display()
                    );
                }
                Err(dotenvy::Error::Io(err)) => {
                    bail!(
                        "Failed to read environment file `{}`: {err}",
                        env_file_path.simplified_display()
                    );
                }
                Err(dotenvy::Error::LineParse(content, position)) => {
                    warn_user!(
                        "Failed to parse environment file `{}` at position {position}: {content}",
                        env_file_path.simplified_display(),
                    );
                }
                Err(err) => {
                    warn_user!(
                        "Failed to parse environment file `{}`: {err}",
                        env_file_path.simplified_display(),
                    );
                }
                Ok(()) => {
                    debug!(
                        "Read environment file at: `{}`",
                        env_file_path.simplified_display()
                    );
                }
            }
        }
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
        return Err(anyhow::anyhow!(
            "Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name"
        ));
    };

    if let Some(ref from) = from {
        if has_python_script_ext(Path::new(from)) {
            let package_name = PackageName::from_str(from)?;
            return Err(anyhow::anyhow!(
                "It looks like you provided a Python script to `--from`, which is not supported\n\n{}{} If you meant to run a command from the `{}` package, use the normalized package name instead to disambiguate, e.g., `{}`",
                "hint".bold().cyan(),
                ":".bold(),
                package_name.cyan(),
                format!(
                    "{} --from {} {}",
                    invocation_source,
                    package_name.cyan(),
                    target
                )
                .green(),
            ));
        }
    } else {
        let target_path = Path::new(target);

        // If the user tries to invoke `uvx script.py`, hint them towards `uv run`.
        if has_python_script_ext(target_path) {
            return if target_path.try_exists()? {
                Err(anyhow::anyhow!(
                    "It looks like you tried to run a Python script at `{}`, which is not supported by `{}`\n\n{}{} Use `{}` instead",
                    target_path.user_display(),
                    invocation_source,
                    "hint".bold().cyan(),
                    ":".bold(),
                    format!("uv run {}", target_path.user_display()).green(),
                ))
            } else {
                let package_name = PackageName::from_str(target)?;
                Err(anyhow::anyhow!(
                    "It looks like you provided a Python script to run, which is not supported supported by `{}`\n\n{}{} We did not find a script at the requested path. If you meant to run a command from the `{}` package, pass the normalized package name to `--from` to disambiguate, e.g., `{}`",
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
                    return Err(anyhow::anyhow!(
                        "Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name"
                    ));
                };
                target = next_target;
                args = next_args;
            }
        }
    }

    let request = ToolRequest::parse(target, from.as_deref())?;

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if request.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Get or create a compatible environment in which to execute the tool.
    let result = Box::pin(get_or_create_environment(
        &request,
        with,
        constraints,
        overrides,
        build_constraints,
        show_resolution,
        python.as_deref(),
        python_platform,
        install_mirrors,
        options,
        &settings,
        &client_builder,
        isolated,
        lfs,
        python_preference,
        python_downloads,
        installer_metadata,
        concurrency,
        &cache,
        printer,
        preview,
    ))
    .await;

    let explicit_from = from.is_some();
    let (from, environment) = match result {
        Ok(resolution) => resolution,
        Err(ProjectError::Operation(err)) => {
            // If the user ran `uvx run ...`, the `run` is likely a mistake. Show a dedicated hint.
            if from.is_none() && invocation_source == ToolRunCommand::Uvx && target == "run" {
                let rest = args.iter().map(|s| s.to_string_lossy()).join(" ");
                return diagnostics::OperationDiagnostic::native_tls(
                    client_builder.is_native_tls(),
                )
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

            let diagnostic =
                diagnostics::OperationDiagnostic::native_tls(client_builder.is_native_tls());
            let diagnostic = if let Some(verbose_flag) = find_verbose_flag(args) {
                diagnostic.with_hint(format!(
                    "You provided `{}` to `{}`. Did you mean to provide it to `{}`? e.g., `{}`",
                    verbose_flag.cyan(),
                    target.cyan(),
                    invocation_source.to_string().cyan(),
                    format!("{invocation_source} {verbose_flag} {target}").green()
                ))
            } else {
                diagnostic.with_context("tool")
            };
            return diagnostic
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
    let site_packages = SitePackages::from_environment(&environment)?;

    // Check if the provided command is not part of the executables for the `from` package,
    // and if it's provided by another package in the environment.
    let provider_hints = match &from {
        ToolRequirement::Python { .. } => None,
        ToolRequirement::Package { requirement, .. } => Some(ExecutableProviderHints::new(
            executable,
            requirement,
            &site_packages,
            invocation_source,
        )),
    };

    if let Some(ref provider_hints) = provider_hints {
        if provider_hints.not_from_any() {
            if !explicit_from {
                // If the user didn't use `--from` and the command isn't in the environment, we're now
                // just invoking an arbitrary executable on the `PATH` and should exit instead.
                writeln!(printer.stderr(), "{provider_hints}")?;
                return Ok(ExitStatus::Failure);
            }
            // In the case where `--from` is used, we'll warn on failure if the command is not found
            // TODO(zanieb): Consider if we should require `--with` instead of `--from` in this case?
            // It'd be a breaking change but would make `uvx` invocations safer.
        } else if provider_hints.not_from_expected() {
            // However, if the user used `--from`, we shouldn't fail because they requested that the
            // package and executable be different. We'll warn if the executable comes from another
            // package though, because that could be confusing
            warn_user_once!("{provider_hints}");
        }
    }

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
    )
    .context("Failed to build new PATH variable")?;
    process.env(EnvVars::PATH, new_path);

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    let space = if args.is_empty() { "" } else { " " };
    debug!(
        "Running `{}{space}{}`",
        executable,
        args.iter().map(|arg| arg.to_string_lossy()).join(" ")
    );

    let handle = match process.spawn() {
        Ok(handle) => Ok(handle),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if let Some(ref provider_hints) = provider_hints {
                if provider_hints.not_from_any() && explicit_from {
                    // We deferred this warning earlier, because `--from` was used and the command
                    // could have come from the `PATH`. Display a more helpful message instead of the
                    // OS error.
                    writeln!(printer.stderr(), "{provider_hints}")?;
                    return Ok(ExitStatus::Failure);
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
    .with_context(|| format!("Failed to spawn: `{executable}`"))?;

    run_to_completion(handle).await
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
        Err(err)
            if err
                .as_io_error()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound) =>
        {
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
                    .get_environment(&name, cache)
                    .ok()
                    .flatten()
                    .and_then(|tool_env| tool_env.version().ok())
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

/// A set of hints about the packages that provide an executable.
#[derive(Debug)]
struct ExecutableProviderHints<'a> {
    /// The requested executable for the command
    executable: &'a str,
    /// The package from which the executable is expected to come from
    from: &'a Requirement,
    /// The packages in the [`PythonEnvironment`] the command will run in
    site_packages: &'a SitePackages,
    /// The packages with matching executable names
    packages: Vec<InstalledDist>,
    /// The source of the invocation, for suggestions to the user
    invocation_source: ToolRunCommand,
}

impl<'a> ExecutableProviderHints<'a> {
    fn new(
        executable: &'a str,
        from: &'a Requirement,
        site_packages: &'a SitePackages,
        invocation_source: ToolRunCommand,
    ) -> Self {
        let packages = matching_packages(executable, site_packages);
        ExecutableProviderHints {
            executable,
            from,
            site_packages,
            packages,
            invocation_source,
        }
    }

    /// If the executable is not provided by the expected package.
    fn not_from_expected(&self) -> bool {
        !self
            .packages
            .iter()
            .any(|package| package.name() == &self.from.name)
    }

    /// If the executable is not provided by any package.
    fn not_from_any(&self) -> bool {
        self.packages.is_empty()
    }
}

impl std::fmt::Display for ExecutableProviderHints<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            executable,
            from,
            site_packages,
            packages,
            invocation_source,
        } = self;

        match packages.as_slice() {
            [] => {
                let entrypoints = match get_entrypoints(&from.name, site_packages) {
                    Ok(entrypoints) => entrypoints,
                    Err(err) => {
                        warn!("Failed to get entrypoints for `{from}`: {err}");
                        return Ok(());
                    }
                };
                if entrypoints.is_empty() {
                    write!(
                        f,
                        "Package `{}` does not provide any executables.",
                        from.name.red()
                    )?;
                    return Ok(());
                }
                writeln!(
                    f,
                    "An executable named `{}` is not provided by package `{}`.",
                    executable.cyan(),
                    from.name.cyan(),
                )?;
                writeln!(f, "The following executables are available:")?;
                for (name, _) in &entrypoints {
                    writeln!(f, "- {}", name.cyan())?;
                }
                let name = match entrypoints.as_slice() {
                    [entrypoint] => entrypoint.0.as_str(),
                    _ => "<EXECUTABLE-NAME>",
                };
                // If the user didn't use `--from`, suggest it
                if *executable == from.name.as_str() {
                    let suggested_command =
                        format!("{} --from {} {name}", invocation_source, from.name);
                    writeln!(f, "\nUse `{}` instead.", suggested_command.green().bold())?;
                }
            }
            [package] if package.name() == &from.name => {
                write!(
                    f,
                    "An executable named `{}` is provided by package `{}`",
                    executable.cyan(),
                    from.name.cyan(),
                )?;
            }
            [package] => {
                let suggested_command = format!(
                    "{invocation_source} --from {} {}",
                    package.name(),
                    executable
                );
                write!(
                    f,
                    "An executable named `{}` is not provided by package `{}` but is available via the dependency `{}`. Consider using `{}` instead.",
                    executable.cyan(),
                    from.name.cyan(),
                    package.name().cyan(),
                    suggested_command.green()
                )?;
            }
            packages => {
                let provided_by = packages
                    .iter()
                    .map(uv_distribution_types::Name::name)
                    .map(|name| format!("- {}", name.cyan()))
                    .join("\n");
                if self.not_from_expected() {
                    let suggested_command = format!("{invocation_source} --from PKG {executable}");
                    write!(
                        f,
                        "An executable named `{}` is not provided by package `{}` but is available via the following dependencies:\n- {}\nConsider using `{}` instead.",
                        executable.cyan(),
                        from.name.cyan(),
                        provided_by,
                        suggested_command.green(),
                    )?;
                } else {
                    write!(
                        f,
                        "An executable named `{}` is provided by package `{}` but is also available via the following dependencies:\n- {}\nUnexpected behavior may occur.",
                        executable.cyan(),
                        from.name.cyan(),
                        provided_by,
                    )?;
                }
            }
        }

        Ok(())
    }
}

// Clippy isn't happy about the difference in size between these variants, but
// [`ToolRequirement::Package`] is the more common case and it seems annoying to box it.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ToolRequirement {
    Python {
        executable: String,
    },
    Package {
        executable: String,
        requirement: Requirement,
    },
}

impl ToolRequirement {
    fn executable(&self) -> &str {
        match self {
            Self::Python { executable, .. } => executable,
            Self::Package { executable, .. } => executable,
        }
    }
}

impl std::fmt::Display for ToolRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python { .. } => write!(f, "python"),
            Self::Package { requirement, .. } => write!(f, "{requirement}"),
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
    build_constraints: &[RequirementsSource],
    show_resolution: bool,
    python: Option<&str>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    options: ResolverInstallerOptions,
    settings: &ResolverInstallerSettings,
    client_builder: &BaseClientBuilder<'_>,
    isolated: bool,
    lfs: GitLfsSetting,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<(ToolRequirement, PythonEnvironment), ProjectError> {
    let reporter = PythonDownloadReporter::single(printer);

    // Figure out what Python we're targeting, either explicitly like `uvx python@3`, or via the
    // -p/--python flag.
    let python_request = match request {
        ToolRequest::Python {
            request: tool_python_request,
            ..
        } => {
            match python {
                None => Some(tool_python_request.clone()),

                // The user is both invoking a python interpreter directly and also supplying the
                // -p/--python flag. Cases like `uvx -p pypy python` are allowed, for two reasons:
                // 1) Previously this was the only way to invoke e.g. PyPy via `uvx`, and it's nice
                // to remain compatible with that. 2) A script might define an alias like `uvx
                // --python $MY_PYTHON ...`, and it's nice to be able to run the interpreter
                // directly while sticking to that alias.
                //
                // However, we want to error out if we see conflicting or redundant versions like
                // `uvx -p python38 python39`.
                //
                // Note that a command like `uvx default` doesn't bring us here. ToolRequest::parse
                // returns ToolRequest::Package rather than ToolRequest::Python in that case. See
                // PythonRequest::try_from_tool_name.
                Some(python_flag) => {
                    if tool_python_request != &PythonRequest::Default {
                        return Err(anyhow::anyhow!(
                            "Received multiple Python version requests: `{}` and `{}`",
                            python_flag.to_string().cyan(),
                            tool_python_request.to_canonical_string().cyan()
                        )
                        .into());
                    }
                    Some(PythonRequest::parse(python_flag))
                }
            }
        }
        ToolRequest::Package { .. } => python.map(PythonRequest::parse),
    };

    // Discover an interpreter.
    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(&reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
        preview,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = PlatformState::default();
    let workspace_cache = WorkspaceCache::default();

    let from = match request {
        ToolRequest::Python {
            executable: request_executable,
            ..
        } => ToolRequirement::Python {
            executable: request_executable.unwrap_or("python").to_string(),
        },
        ToolRequest::Package {
            executable: request_executable,
            target,
        } => {
            let (executable, requirement) = match target {
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

                    let requirement = resolve_names(
                        vec![spec],
                        &interpreter,
                        settings,
                        client_builder,
                        &state,
                        concurrency,
                        cache,
                        &workspace_cache,
                        printer,
                        preview,
                        lfs,
                    )
                    .await?
                    .pop()
                    .unwrap();

                    // Prefer, in order:
                    // 1. The verbatim executable provided by the user, independent of the requirement (as in: `uvx --from package executable`).
                    // 2. The verbatim executable provided by the user as a named requirement (as in: `uvx change_wheel_version`).
                    // 3. The resolved package name (as in: `uvx git+https://github.com/pallets/flask`).
                    let executable = request_executable
                        .map(ToString::to_string)
                        .or_else(|| name.map(ToString::to_string))
                        .unwrap_or_else(|| requirement.name.to_string());

                    (executable, requirement)
                }
                // Ex) `ruff@0.6.0`
                Target::Version(executable, name, extras, version) => {
                    let executable = request_executable
                        .map(ToString::to_string)
                        .unwrap_or_else(|| (*executable).to_string());
                    let requirement = Requirement {
                        name: name.clone(),
                        extras: extras.clone(),
                        groups: Box::new([]),
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
                    let executable = request_executable
                        .map(ToString::to_string)
                        .unwrap_or_else(|| (*executable).to_string());
                    let requirement = Requirement {
                        name: name.clone(),
                        extras: extras.clone(),
                        groups: Box::new([]),
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
        }
    };

    // For `@latest`, fetch the latest version and create a constraint.
    let latest = if let ToolRequest::Package {
        target: Target::Latest(_, name, _),
        ..
    } = &request
    {
        // Build the registry client to fetch the latest version.
        let client = RegistryClientBuilder::new(client_builder.clone(), cache.clone())
            .index_locations(settings.resolver.index_locations.clone())
            .index_strategy(settings.resolver.index_strategy)
            .markers(interpreter.markers())
            .platform(interpreter.platform())
            .build();

        // Initialize the capabilities.
        let capabilities = IndexCapabilities::default();
        let download_concurrency = Semaphore::new(concurrency.downloads);

        // Initialize the client to fetch the latest version.
        let latest_client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease: settings.resolver.prerelease,
            exclude_newer: &settings.resolver.exclude_newer,
            tags: None,
            requires_python: None,
        };

        // Fetch the latest version.
        if let Some(dist_filename) = latest_client
            .find_latest(name, None, &download_concurrency)
            .await?
        {
            let version = dist_filename.version().clone();
            debug!("Resolved `{name}@latest` to `{name}=={version}`");

            // The constraint pins the version during resolution to prevent backtracking.
            Some(Requirement {
                name: name.clone(),
                extras: vec![].into_boxed_slice(),
                groups: Box::new([]),
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(version)),
                    index: None,
                    conflict: None,
                },
                origin: None,
            })
        } else {
            None
        }
    } else {
        None
    };

    // Read the `--with` requirements.
    let spec = RequirementsSpecification::from_sources(
        with,
        constraints,
        overrides,
        &[],
        None,
        client_builder,
    )
    .await?;

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        match &from {
            ToolRequirement::Python { .. } => {}
            ToolRequirement::Package { requirement, .. } => requirements.push(requirement.clone()),
        }
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                settings,
                client_builder,
                &state,
                concurrency,
                cache,
                &workspace_cache,
                printer,
                preview,
                lfs,
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
        client_builder,
        &state,
        concurrency,
        cache,
        &workspace_cache,
        printer,
        preview,
        lfs,
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
                        python_request.satisfied(environment.environment().interpreter(), cache)
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
                    let ResolverInstallerSettings {
                        resolver:
                            ResolverSettings {
                                config_setting,
                                config_settings_package,
                                extra_build_dependencies,
                                extra_build_variables,
                                ..
                            },
                        ..
                    } = settings;

                    // Lower the extra build dependencies, if any.
                    let extra_build_requires = LoweredExtraBuildDependencies::from_non_lowered(
                        extra_build_dependencies.clone(),
                    )
                    .into_inner();

                    // Determine the markers and tags to use for the resolution.
                    let markers =
                        pip::resolution_markers(None, python_platform.as_ref(), &interpreter);
                    let tags = pip::resolution_tags(None, python_platform.as_ref(), &interpreter)?;

                    // Check if the installed packages meet the requirements.
                    let site_packages = SitePackages::from_environment(environment.environment())?;
                    if matches!(
                        site_packages.satisfies_requirements(
                            requirements.iter(),
                            constraints.iter().chain(latest.iter()),
                            overrides.iter(),
                            InstallationStrategy::Permissive,
                            &markers,
                            &tags,
                            config_setting,
                            config_settings_package,
                            &extra_build_requires,
                            extra_build_variables,
                        ),
                        Ok(SatisfiesResult::Fresh { .. })
                    ) {
                        debug!("Using existing tool `{}`", requirement.name);
                        return Ok((from, environment.into_environment()));
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
            .chain(latest.into_iter())
            .map(NameRequirementSpecification::from)
            .collect(),
        overrides: overrides
            .into_iter()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    });

    // Read the `--build-constraints` requirements.
    let build_constraints = Constraints::from_requirements(
        operations::read_constraints(build_constraints, client_builder)
            .await?
            .into_iter()
            .map(|constraint| constraint.requirement),
    );

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool.
    // TODO(zanieb): Determine if we should layer on top of the project environment if it is present.

    let result = CachedEnvironment::from_spec(
        spec.clone(),
        build_constraints.clone(),
        &interpreter,
        python_platform.as_ref(),
        settings,
        client_builder,
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
                    client_builder,
                    &reporter,
                    &install_mirrors,
                    python_preference,
                    python_downloads,
                    cache,
                    preview,
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
                    build_constraints,
                    &interpreter,
                    python_platform.as_ref(),
                    settings,
                    client_builder,
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

    Ok((from, environment.into()))
}
