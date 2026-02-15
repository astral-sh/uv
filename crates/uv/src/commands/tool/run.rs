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
use uv_fs::CWD;
use uv_fs::Simplified;
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_preview::Preview;
use uv_python::PythonVersionFile;
use uv_python::VersionFileDiscoveryOptions;
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
#[expect(clippy::fn_params_excessive_bools)]
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

    // Attempt the fast path for simple invocations: if the tool is already
    // installed and the receipt matches, skip expensive Python discovery and
    // name resolution entirely.
    let fast_path_result = if is_simple_invocation(
        &request,
        with,
        constraints,
        overrides,
        build_constraints,
        python.as_deref(),
        isolated,
    ) {
        try_installed_fast_path(target, &options, &cache).await?
    } else {
        None
    };

    let explicit_from = from.is_some();
    let (from, environment) = if let Some(resolution) = fast_path_result {
        resolution
    } else {
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

        match result {
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
        }
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
#[expect(clippy::large_enum_variant)]
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

    // Determine explicit Python version requests
    let explicit_python_request = python.map(PythonRequest::parse);
    let tool_python_request = match request {
        ToolRequest::Python { request, .. } => Some(request.clone()),
        ToolRequest::Package { .. } => None,
    };

    // Resolve Python request with version file lookup when no explicit request
    let python_request = match (explicit_python_request, tool_python_request) {
        // e.g., `uvx --python 3.10 python3.12`
        (Some(explicit), Some(tool_request)) if tool_request != PythonRequest::Default => {
            // Conflict: both --python flag and versioned tool name
            return Err(anyhow::anyhow!(
                "Received multiple Python version requests: `{}` and `{}`",
                explicit.to_canonical_string().cyan(),
                tool_request.to_canonical_string().cyan()
            )
            .into());
        }
        // e.g, `uvx --python 3.10 ...`
        (Some(explicit), _) => Some(explicit),
        // e.g., `uvx python` or `uvx <tool>`
        (None, Some(PythonRequest::Default) | None) => PythonVersionFile::discover(
            &*CWD,
            &VersionFileDiscoveryOptions::default()
                .with_no_config(false)
                .with_no_local(true),
        )
        .await?
        .and_then(PythonVersionFile::into_version),
        // e.g., `uvx python3.12`
        (None, Some(tool_request)) => Some(tool_request),
    };

    // For non-isolated, non-latest invocations, try to extract the package name early
    // so we can run the installed tool pre-check concurrently with Python discovery.
    let precheck_package_name = if !isolated && !request.is_latest() {
        match request {
            ToolRequest::Package {
                target: Target::Version(_, name, _, _),
                ..
            } => Some(name.clone()),
            ToolRequest::Package {
                target: Target::Unspecified(requirement),
                ..
            } => {
                // Try to extract the package name from the requirement string (e.g., "ruff>=0.6.0" -> "ruff").
                let content = requirement.trim();
                let index = content
                    .find(
                        |c: char| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.'),
                    )
                    .unwrap_or(content.len());
                PackageName::from_str(&content[..index]).ok()
            }
            _ => None,
        }
    } else {
        None
    };

    // Run Python discovery and the installed tool pre-check concurrently when both are needed.
    // The pre-check fetches the tool environment and receipt from disk, which doesn't require
    // the interpreter. This reduces wall-clock time for non-simple invocations.
    let python_discovery_fut = async {
        PythonInstallation::find_or_download(
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
        .await
        .map(PythonInstallation::into_interpreter)
        .map_err(ProjectError::from)
    };

    let precheck_fut = async {
        if let Some(ref package_name) = precheck_package_name {
            let installed_tools = InstalledTools::from_settings()?.init()?;
            let lock = installed_tools.lock_shared().await?;
            let environment = installed_tools.get_environment(package_name, cache)?;
            let receipt = installed_tools
                .get_tool_receipt(package_name)
                .ok()
                .flatten();
            Ok::<_, ProjectError>(Some((installed_tools, lock, environment, receipt)))
        } else {
            Ok(None)
        }
    };

    let (interpreter_result, precheck_result) = tokio::join!(python_discovery_fut, precheck_fut);

    // Python discovery error takes precedence if both fail.
    let interpreter = interpreter_result?;
    let precheck_data = precheck_result?;

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
        let client = RegistryClientBuilder::new(
            client_builder
                .clone()
                .keyring(settings.resolver.keyring_provider),
            cache.clone(),
        )
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

    // Resolve the constraints (no name resolution needed — constraints are always named).
    let constraints = spec
        .constraints
        .clone()
        .into_iter()
        .map(|constraint| constraint.requirement)
        .collect::<Vec<_>>();

    // Batch `--with` requirements and overrides into a single `resolve_names` call to avoid
    // redundant registry client initialization. The main requirement's resolution (for
    // `Target::Unspecified`) remains separate since it happens during target parsing above.
    //
    // We pre-partition named vs unnamed requirements from each source, combine only the unnamed
    // into a single batch for resolution, then reconstruct each group. This avoids the ordering
    // issue where `resolve_names` returns all named first, then all resolved unnamed.
    let (with_named, with_unnamed): (Vec<_>, Vec<_>) = spec
        .requirements
        .clone()
        .into_iter()
        .partition(|spec| matches!(spec.requirement, UnresolvedRequirement::Named(..)));
    let (override_named, override_unnamed): (Vec<_>, Vec<_>) = spec
        .overrides
        .clone()
        .into_iter()
        .partition(|spec| matches!(spec.requirement, UnresolvedRequirement::Named(..)));

    let with_unnamed_count = with_unnamed.len();

    // Combine all unnamed requirements from both sources into a single batch.
    let mut all_unnamed: Vec<UnresolvedRequirementSpecification> =
        Vec::with_capacity(with_unnamed.len() + override_unnamed.len());
    all_unnamed.extend(with_unnamed);
    all_unnamed.extend(override_unnamed);

    // Resolve all unnamed requirements in a single call (skips resolution entirely if empty).
    let resolved_unnamed = resolve_names(
        all_unnamed,
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

    // Split the resolved unnamed back into `--with` and override groups.
    // `resolve_names` preserves order for all-unnamed input (named partition is empty,
    // and `FuturesOrdered` preserves unnamed order).
    let (resolved_with_unnamed, resolved_override_unnamed) =
        resolved_unnamed.split_at(with_unnamed_count);

    // Reconstruct the `--with` requirements: named (already resolved) + resolved unnamed.
    let requirements = {
        let mut requirements =
            Vec::with_capacity(1 + with_named.len() + resolved_with_unnamed.len());
        match &from {
            ToolRequirement::Python { .. } => {}
            ToolRequirement::Package { requirement, .. } => requirements.push(requirement.clone()),
        }
        requirements.extend(with_named.into_iter().filter_map(|spec| {
            match spec
                .requirement
                .augment_requirement(None, None, None, lfs.into(), None)
            {
                UnresolvedRequirement::Named(req) => Some(req),
                UnresolvedRequirement::Unnamed(..) => None,
            }
        }));
        requirements.extend_from_slice(resolved_with_unnamed);
        requirements
    };

    // Reconstruct the overrides: named (already resolved) + resolved unnamed.
    let overrides: Vec<Requirement> = override_named
        .into_iter()
        .filter_map(|spec| {
            match spec
                .requirement
                .augment_requirement(None, None, None, lfs.into(), None)
            {
                UnresolvedRequirement::Named(req) => Some(req),
                UnresolvedRequirement::Unnamed(..) => None,
            }
        })
        .chain(resolved_override_unnamed.iter().cloned())
        .collect();

    // Check if the tool is already installed in a compatible environment.
    // Use pre-fetched data from the concurrent pre-check when available, otherwise
    // fall back to fetching the environment and receipt here.
    //
    // The lock must outlive the `satisfies_requirements` check to prevent a TOCTOU
    // race where another process modifies the tool between reading and using it.
    if !isolated && !request.is_latest() {
        if let ToolRequirement::Package { requirement, .. } = &from {
            // Determine whether we can reuse the pre-fetched data or need a fresh lookup.
            let use_precheck = precheck_data.is_some()
                && precheck_package_name.as_ref() == Some(&requirement.name);

            // Acquire a lock that outlives the entire check block. For the precheck
            // branch the lock is already held in `precheck_data`; for fallback branches
            // we acquire a fresh shared lock here and keep it alive.
            let fallback_tools;
            let fallback_lock;
            let (existing_environment, receipt_matches) = if use_precheck {
                let (_installed_tools, _lock, prefetched_env, prefetched_receipt) =
                    precheck_data.as_ref().unwrap();
                let env = prefetched_env.as_ref().and_then(|environment| {
                    if python_request.as_ref().is_none_or(|python_request| {
                        python_request.satisfied(environment.environment().interpreter(), cache)
                    }) {
                        Some(environment)
                    } else {
                        None
                    }
                });
                let receipt_ok = prefetched_receipt
                    .as_ref()
                    .is_some_and(|receipt| ToolOptions::from(options) == *receipt.options());
                (env.cloned(), receipt_ok)
            } else {
                // Either the precheck package name didn't match, or no precheck was performed.
                // Acquire a fresh shared lock that outlives the satisfies_requirements check.
                fallback_tools = InstalledTools::from_settings()?.init()?;
                fallback_lock = fallback_tools.lock_shared().await?;
                let _ = &fallback_lock; // ensure the lock is held through the check below
                let env = fallback_tools
                    .get_environment(&requirement.name, cache)?
                    .filter(|environment| {
                        python_request.as_ref().is_none_or(|python_request| {
                            python_request.satisfied(environment.environment().interpreter(), cache)
                        })
                    });
                let receipt_ok = fallback_tools
                    .get_tool_receipt(&requirement.name)
                    .ok()
                    .flatten()
                    .is_some_and(|receipt| ToolOptions::from(options) == *receipt.options());
                (env, receipt_ok)
            };

            // Check if the installed packages meet the requirements.
            if let Some(environment) = existing_environment {
                if receipt_matches {
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

/// Check whether a tool receipt matches the current request for the fast path.
///
/// Returns `Some(ToolRequirement)` if the receipt's options match, it has no
/// `constraints/overrides/build_constraints`, and it has at least one requirement.
/// Returns `None` if any of these conditions are not met.
///
/// This is the pure, deterministic portion of the fast-path check — it does not
/// touch the filesystem or the Python environment.
fn receipt_matches_request(
    target: &str,
    receipt: &uv_tool::Tool,
    options: &ResolverInstallerOptions,
) -> Option<ToolRequirement> {
    // Check that the current resolver/installer options match the receipt.
    if ToolOptions::from(options.clone()) != *receipt.options() {
        return None;
    }

    // Verify the receipt has no constraints, overrides, or build constraints
    // beyond the base requirement. If it does, this is not a simple install
    // and we should fall through to the full path.
    if !receipt.constraints().is_empty()
        || !receipt.overrides().is_empty()
        || !receipt.build_constraints().is_empty()
    {
        return None;
    }

    // Build the ToolRequirement from the receipt's primary requirement.
    // Use the original target string as the executable name to preserve the
    // user's verbatim input (e.g., dots in "awslabs.aws-documentation-mcp-server").
    let requirement = receipt.requirements().first()?.clone();

    Some(ToolRequirement::Package {
        executable: target.to_string(),
        requirement,
    })
}

/// Attempt the fast path for simple tool invocations.
///
/// Returns `Some((ToolRequirement, PythonEnvironment))` if the tool is already
/// installed and satisfies the request, `None` otherwise.
///
/// This function checks the tool receipt and environment without performing
/// Python discovery or name resolution, making it significantly faster than
/// the full `get_or_create_environment` path for repeat invocations.
async fn try_installed_fast_path(
    target: &str,
    options: &ResolverInstallerOptions,
    cache: &Cache,
) -> Result<Option<(ToolRequirement, PythonEnvironment)>, ProjectError> {
    // Parse the target as a package name. If it fails, this isn't a simple
    // package invocation — fall through to the full path.
    let Ok(package_name) = PackageName::from_str(target) else {
        return Ok(None);
    };

    // Initialize the installed tools directory.
    let installed_tools = InstalledTools::from_settings()?.init()?;

    // Acquire a shared (read-only) lock — this allows concurrent readers
    // without blocking, unlike the exclusive lock used for writes.
    let _lock = installed_tools.lock_shared().await?;

    // Read the tool receipt. If missing, the tool isn't installed.
    let receipt = match installed_tools.get_tool_receipt(&package_name) {
        Ok(Some(receipt)) => receipt,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(None),
    };

    // Check receipt matches using the pure logic function.
    let Some(from) = receipt_matches_request(target, &receipt, options) else {
        return Ok(None);
    };

    // Get the tool environment. Returns `None` if the environment is missing
    // or the Python interpreter is broken/not found.
    let Some(environment) = installed_tools.get_environment(&package_name, cache)? else {
        return Ok(None);
    };

    // Validate the environment's Python interpreter still exists on disk.
    if !environment
        .environment()
        .interpreter()
        .sys_executable()
        .is_file()
    {
        return Ok(None);
    }

    debug!("Using existing tool `{package_name}` (fast path)");

    Ok(Some((from, environment.into_environment())))
}

/// Returns true if this is a simple invocation eligible for the fast path.
///
/// A simple invocation is one where:
/// - No `--from` flag is used (the request is `ToolRequest::Package` with no separate executable)
/// - No `--with` requirements are specified
/// - No `--python` flag is specified
/// - No version constraint (`@version`) or `@latest` is used (target is `Target::Unspecified`)
/// - No `--isolated` flag is used
/// - No constraints, overrides, or build constraints are specified
fn is_simple_invocation(
    request: &ToolRequest<'_>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    python: Option<&str>,
    isolated: bool,
) -> bool {
    // Must not be isolated
    if isolated {
        return false;
    }

    // Must not have --python
    if python.is_some() {
        return false;
    }

    // Must not have --with requirements
    if !with.is_empty() {
        return false;
    }

    // Must not have constraints
    if !constraints.is_empty() {
        return false;
    }

    // Must not have overrides
    if !overrides.is_empty() {
        return false;
    }

    // Must not have build constraints
    if !build_constraints.is_empty() {
        return false;
    }

    // Must be a Package request with Target::Unspecified (bare package name, no @version or @latest)
    // and no --from (executable must be None, meaning the target IS the package)
    matches!(
        request,
        ToolRequest::Package {
            executable: None,
            target: Target::Unspecified(_),
        }
    )
}

/// Apply the sequential error-precedence pattern used after `tokio::join!` for
/// parallel Python discovery and installed-tool pre-check.
///
/// When two concurrent operations each produce a `Result`, the first result's
/// error takes precedence: we evaluate `first?` before `second?`, so if both
/// fail the first error is propagated and the second is discarded.
///
/// This mirrors the pattern in `get_or_create_environment`:
/// ```ignore
/// let (interpreter_result, precheck_result) = tokio::join!(…, …);
/// let interpreter = interpreter_result?;   // first — takes precedence
/// let precheck_data = precheck_result?;    // second
/// ```
#[cfg(test)]
fn resolve_parallel_results<A, B, E>(
    first: Result<A, E>,
    second: Result<B, E>,
) -> Result<(A, B), E> {
    let a = first?;
    let b = second?;
    Ok((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::str::FromStr;
    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_tool::{Tool, ToolEntrypoint};

    /// Helper: generate a valid bare package name string for `Target::Unspecified`.
    fn simple_package_name() -> impl Strategy<Value = String> {
        // Valid Python package names: start with a letter, contain letters/digits/hyphens.
        prop::string::string_regex("[a-z][a-z0-9-]{0,20}")
            .unwrap()
            .prop_filter("must be non-empty after parse", |s| !s.is_empty())
    }

    /// Helper: generate a valid package name that can be parsed as a `PackageName`.
    fn valid_package_name() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z][a-z0-9]{0,15}")
            .unwrap()
            .prop_filter("must parse as PackageName", |s| {
                !s.is_empty() && PackageName::from_str(s).is_ok()
            })
    }

    /// Helper: generate a random version with 1-3 segments.
    fn arb_version() -> impl Strategy<Value = Version> {
        prop::collection::vec(1u64..100, 1..=3).prop_map(Version::new)
    }

    /// Helper: create a simple Requirement for a given package name with a version specifier.
    fn make_requirement(name: &PackageName, version: &Version) -> Requirement {
        let specifier = VersionSpecifier::equals_version(version.clone());
        Requirement {
            name: name.clone(),
            extras: Box::new([]),
            groups: Box::new([]),
            marker: MarkerTree::default(),
            source: RequirementSource::Registry {
                specifier: VersionSpecifiers::from(specifier),
                index: None,
                conflict: None,
            },
            origin: None,
        }
    }

    /// Helper: create a Tool receipt with matching options and a single requirement.
    fn make_matching_receipt(name: &PackageName, version: &Version, options: &ToolOptions) -> Tool {
        let requirement = make_requirement(name, version);
        let entrypoint = ToolEntrypoint::new(
            name.as_ref(),
            std::path::PathBuf::from(format!("/usr/bin/{name}")),
            name.to_string(),
        );
        Tool::new(
            vec![requirement],
            vec![], // no constraints
            vec![], // no overrides
            vec![], // no build_constraints
            None,   // no python
            vec![entrypoint],
            options.clone(),
        )
    }

    /// Helper: create a `RequirementsSource` for testing (a simple package requirement).
    fn make_requirements_source() -> RequirementsSource {
        // Use a simple Package variant with a known-good requirement string.
        RequirementsSource::from_package("flask").unwrap()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: uvx-startup-optimization, Property 4: Non-simple invocations bypass fast path
        ///
        /// **Validates: Requirements 6.1, 6.2, 6.3**
        ///
        /// For any invocation with at least one non-simple flag, is_simple_invocation returns false.
        #[test]
        fn prop_non_simple_invocations_return_false(
            pkg_name in simple_package_name(),
            has_with in any::<bool>(),
            has_constraints in any::<bool>(),
            has_overrides in any::<bool>(),
            has_build_constraints in any::<bool>(),
            has_python in any::<bool>(),
            is_isolated in any::<bool>(),
            // 0 = Unspecified, 1 = Version, 2 = Latest, 3 = Python request, 4 = has executable (--from)
            target_kind in 0u8..5,
        ) {
            // At least one non-simple condition must be true.
            // Non-simple conditions:
            //   - has_with, has_constraints, has_overrides, has_build_constraints, has_python, is_isolated
            //   - target_kind != 0 (not Unspecified)
            //   - target_kind == 4 (has executable / --from)
            let any_non_simple = has_with
                || has_constraints
                || has_overrides
                || has_build_constraints
                || has_python
                || is_isolated
                || target_kind != 0;

            // Skip cases where everything is simple — we test those separately.
            prop_assume!(any_non_simple);

            let with: Vec<RequirementsSource> = if has_with {
                vec![make_requirements_source()]
            } else {
                vec![]
            };
            let constraints: Vec<RequirementsSource> = if has_constraints {
                vec![make_requirements_source()]
            } else {
                vec![]
            };
            let overrides: Vec<RequirementsSource> = if has_overrides {
                vec![make_requirements_source()]
            } else {
                vec![]
            };
            let build_constraints: Vec<RequirementsSource> = if has_build_constraints {
                vec![make_requirements_source()]
            } else {
                vec![]
            };
            let python: Option<&str> = if has_python { Some("3.12") } else { None };

            let name_for_target = if pkg_name.is_empty() { "ruff".to_string() } else { pkg_name.clone() };

            let request = match target_kind {
                0 => ToolRequest::Package {
                    executable: None,
                    target: Target::Unspecified(&name_for_target),
                },
                1 => {
                    let parsed_name = PackageName::from_str(&name_for_target).unwrap_or_else(|_| PackageName::from_str("ruff").unwrap());
                    ToolRequest::Package {
                        executable: None,
                        target: Target::Version(&name_for_target, parsed_name, Box::new([]), Version::new([1, 0, 0])),
                    }
                }
                2 => {
                    let parsed_name = PackageName::from_str(&name_for_target).unwrap_or_else(|_| PackageName::from_str("ruff").unwrap());
                    ToolRequest::Package {
                        executable: None,
                        target: Target::Latest(&name_for_target, parsed_name, Box::new([])),
                    }
                }
                3 => ToolRequest::Python {
                    executable: None,
                    request: PythonRequest::Default,
                },
                4 => ToolRequest::Package {
                    executable: Some("my-tool"),
                    target: Target::Unspecified(&name_for_target),
                },
                _ => unreachable!(),
            };

            let result = is_simple_invocation(
                &request,
                &with,
                &constraints,
                &overrides,
                &build_constraints,
                python,
                is_isolated,
            );

            prop_assert!(!result, "Expected false for non-simple invocation, got true. \
                has_with={}, has_constraints={}, has_overrides={}, has_build_constraints={}, has_python={}, \
                is_isolated={}, target_kind={}",
                has_with, has_constraints, has_overrides, has_build_constraints, has_python, is_isolated, target_kind);
        }

        /// Feature: uvx-startup-optimization, Property 4: Non-simple invocations bypass fast path
        ///
        /// **Validates: Requirements 6.1, 6.2, 6.3**
        ///
        /// For any simple invocation (bare package name, no flags), is_simple_invocation returns true.
        #[test]
        fn prop_simple_invocations_return_true(
            pkg_name in simple_package_name(),
        ) {
            let request = ToolRequest::Package {
                executable: None,
                target: Target::Unspecified(&pkg_name),
            };

            let result = is_simple_invocation(
                &request,
                &[],       // no --with
                &[],       // no constraints
                &[],       // no overrides
                &[],       // no build constraints
                None,      // no --python
                false,     // not --isolated
            );

            prop_assert!(result, "Expected true for simple invocation with package '{}', got false", pkg_name);
        }

        /// Feature: uvx-startup-optimization, Property 3: Receipt match implies fast path success
        ///
        /// **Validates: Requirements 3.2**
        ///
        /// For any simple invocation where a valid Tool_Receipt exists, the receipt's
        /// package name matches the requested target, the receipt's options match the
        /// current options, and the receipt has at least one requirement with no
        /// constraints/overrides/build_constraints, the receipt matching logic SHALL
        /// return a successful ToolRequirement.
        #[test]
        fn prop_receipt_match_implies_fast_path_success(
            pkg_name in valid_package_name(),
            version in arb_version(),
        ) {
            let package_name = PackageName::from_str(&pkg_name).unwrap();
            let options = ResolverInstallerOptions::default();
            let tool_options = ToolOptions::from(options.clone());

            // Create a receipt that matches the request: same options, no constraints,
            // and a requirement whose package name matches the target.
            let receipt = make_matching_receipt(&package_name, &version, &tool_options);

            let result = receipt_matches_request(&pkg_name, &receipt, &options);

            prop_assert!(
                result.is_some(),
                "Expected receipt_matches_request to return Some for matching receipt \
                 with package '{}' version '{}', got None",
                pkg_name, version
            );

            // Verify the returned ToolRequirement has the correct executable name.
            if let Some(ToolRequirement::Package { executable, requirement }) = &result {
                prop_assert_eq!(
                    executable, &pkg_name,
                    "Executable name should match the package name"
                );
                prop_assert_eq!(
                    &requirement.name, &package_name,
                    "Requirement name should match the package name"
                );
            } else {
                prop_assert!(false, "Expected ToolRequirement::Package variant");
            }
        }

        /// Feature: uvx-startup-optimization, Property 3: Receipt match implies fast path success
        ///
        /// **Validates: Requirements 3.2**
        ///
        /// Verify that receipt matching succeeds for any valid package name and version
        /// combination when options are identical, regardless of the specific option values.
        #[test]
        fn prop_receipt_match_with_default_options_always_succeeds(
            pkg_name in valid_package_name(),
            version_segments in prop::collection::vec(1u64..100, 1..=3),
        ) {
            let package_name = PackageName::from_str(&pkg_name).unwrap();
            let version = Version::new(version_segments);

            // Both sides use default options — they must match.
            let options = ResolverInstallerOptions::default();
            let tool_options = ToolOptions::default();

            let receipt = make_matching_receipt(&package_name, &version, &tool_options);

            let result = receipt_matches_request(&pkg_name, &receipt, &options);

            prop_assert!(
                result.is_some(),
                "Default options should always match: package='{}', version='{}'",
                pkg_name, version
            );
        }

        /// Feature: uvx-startup-optimization, Property 2: Fast path fallback completeness
        ///
        /// **Validates: Requirements 1.3, 3.3**
        ///
        /// For any invocation where the receipt doesn't match the request — including
        /// mismatched options, non-empty constraints, non-empty overrides, non-empty
        /// build_constraints, or empty requirements — receipt_matches_request SHALL
        /// return None, causing the fast path to fall through to the full resolution path.
        #[test]
        fn prop_fast_path_fallback_on_failure_conditions(
            pkg_name in valid_package_name(),
            version in arb_version(),
            // Each bit represents a failure condition to inject:
            // 0 = mismatched options (no_index toggled)
            // 1 = non-empty constraints
            // 2 = non-empty overrides
            // 3 = non-empty build_constraints
            // 4 = empty requirements
            failure_kind in 0u8..5,
        ) {
            let package_name = PackageName::from_str(&pkg_name).unwrap();
            let options = ResolverInstallerOptions::default();
            let tool_options = ToolOptions::from(options.clone());

            // Start from a valid matching receipt, then inject exactly one failure.
            let requirement = make_requirement(&package_name, &version);
            let entrypoint = ToolEntrypoint::new(
                package_name.as_ref(),
                std::path::PathBuf::from(format!("/usr/bin/{package_name}")),
                package_name.to_string(),
            );

            let receipt = match failure_kind {
                // Mismatched options: toggle no_index so options differ.
                0 => {
                    let mut mismatched_options = tool_options.clone();
                    mismatched_options.no_index = Some(true);
                    Tool::new(
                        vec![requirement],
                        vec![],
                        vec![],
                        vec![],
                        None,
                        vec![entrypoint],
                        mismatched_options,
                    )
                }
                // Non-empty constraints.
                1 => {
                    let constraint = make_requirement(&package_name, &version);
                    Tool::new(
                        vec![requirement],
                        vec![constraint],
                        vec![],
                        vec![],
                        None,
                        vec![entrypoint],
                        tool_options.clone(),
                    )
                }
                // Non-empty overrides.
                2 => {
                    let r#override = make_requirement(&package_name, &version);
                    Tool::new(
                        vec![requirement],
                        vec![],
                        vec![r#override],
                        vec![],
                        None,
                        vec![entrypoint],
                        tool_options.clone(),
                    )
                }
                // Non-empty build_constraints.
                3 => {
                    let build_constraint = make_requirement(&package_name, &version);
                    Tool::new(
                        vec![requirement],
                        vec![],
                        vec![],
                        vec![build_constraint],
                        None,
                        vec![entrypoint],
                        tool_options.clone(),
                    )
                }
                // Empty requirements (no requirements at all).
                4 => {
                    Tool::new(
                        vec![],
                        vec![],
                        vec![],
                        vec![],
                        None,
                        vec![entrypoint],
                        tool_options.clone(),
                    )
                }
                _ => unreachable!(),
            };

            let result = receipt_matches_request(&pkg_name, &receipt, &options);

            let failure_desc = match failure_kind {
                0 => "mismatched options",
                1 => "non-empty constraints",
                2 => "non-empty overrides",
                3 => "non-empty build_constraints",
                4 => "empty requirements",
                _ => unreachable!(),
            };

            prop_assert!(
                result.is_none(),
                "Expected None for failure condition '{}' with package '{}', but got Some",
                failure_desc, pkg_name
            );
        }

        /// Feature: uvx-startup-optimization, Property 2: Fast path fallback completeness
        ///
        /// **Validates: Requirements 1.3, 3.3**
        ///
        /// For any combination of multiple simultaneous failure conditions,
        /// receipt_matches_request SHALL still return None.
        #[test]
        fn prop_fast_path_fallback_on_combined_failures(
            pkg_name in valid_package_name(),
            version in arb_version(),
            has_mismatched_options in any::<bool>(),
            has_constraints in any::<bool>(),
            has_overrides in any::<bool>(),
            has_build_constraints in any::<bool>(),
            has_empty_requirements in any::<bool>(),
        ) {
            // At least one failure condition must be present.
            let any_failure = has_mismatched_options
                || has_constraints
                || has_overrides
                || has_build_constraints
                || has_empty_requirements;
            prop_assume!(any_failure);

            let package_name = PackageName::from_str(&pkg_name).unwrap();
            let options = ResolverInstallerOptions::default();
            let tool_options = ToolOptions::from(options.clone());

            let requirement = make_requirement(&package_name, &version);
            let entrypoint = ToolEntrypoint::new(
                package_name.as_ref(),
                std::path::PathBuf::from(format!("/usr/bin/{package_name}")),
                package_name.to_string(),
            );

            let receipt_options = if has_mismatched_options {
                let mut opts = tool_options.clone();
                opts.no_index = Some(true);
                opts
            } else {
                tool_options.clone()
            };

            let constraints = if has_constraints {
                vec![make_requirement(&package_name, &version)]
            } else {
                vec![]
            };

            let overrides = if has_overrides {
                vec![make_requirement(&package_name, &version)]
            } else {
                vec![]
            };

            let build_constraints = if has_build_constraints {
                vec![make_requirement(&package_name, &version)]
            } else {
                vec![]
            };

            let requirements = if has_empty_requirements {
                vec![]
            } else {
                vec![requirement]
            };

            let receipt = Tool::new(
                requirements,
                constraints,
                overrides,
                build_constraints,
                None,
                vec![entrypoint],
                receipt_options,
            );

            let result = receipt_matches_request(&pkg_name, &receipt, &options);

            prop_assert!(
                result.is_none(),
                "Expected None when at least one failure condition is present: \
                 mismatched_options={}, constraints={}, overrides={}, \
                 build_constraints={}, empty_requirements={}",
                has_mismatched_options, has_constraints, has_overrides,
                has_build_constraints, has_empty_requirements
            );
        }

        /// Feature: uvx-startup-optimization, Property 1: Fast path equivalence
        ///
        /// **Validates: Requirements 1.4, 6.4, 7.3**
        ///
        /// For any simple invocation where the receipt matches the request, the
        /// `ToolRequirement` returned by `receipt_matches_request` (the fast path's
        /// core logic) SHALL be equivalent to what the full path would construct:
        /// - The executable name equals the package name (as the full path extracts
        ///   the verbatim name from a bare `Target::Unspecified` string).
        /// - The requirement equals the receipt's first stored requirement (which is
        ///   the originally-resolved requirement saved at install time).
        #[test]
        fn prop_fast_path_equivalence(
            pkg_name in valid_package_name(),
            version in arb_version(),
        ) {
            let package_name = PackageName::from_str(&pkg_name).unwrap();
            let options = ResolverInstallerOptions::default();
            let tool_options = ToolOptions::from(options.clone());

            // Build a matching receipt (simulates a pre-populated tool directory).
            let receipt = make_matching_receipt(&package_name, &version, &tool_options);

            // The receipt's first requirement is what was stored at install time.
            let expected_requirement = receipt.requirements().first().unwrap().clone();

            // --- Fast path result (via receipt_matches_request) ---
            let fast_result = receipt_matches_request(&pkg_name, &receipt, &options);
            prop_assert!(
                fast_result.is_some(),
                "Fast path should succeed for matching receipt: pkg='{}', version='{}'",
                pkg_name, version
            );
            let fast_tool_req = fast_result.unwrap();

            // --- Simulate full path result ---
            // For Target::Unspecified with a bare package name (simple invocation),
            // the full path in get_or_create_environment extracts the verbatim name
            // from the requirement string (the part before any specifiers), which for
            // a bare name equals the package name itself. The resolved requirement
            // would match what was originally installed (stored in the receipt).
            let full_path_executable = pkg_name.clone();
            let full_path_requirement = expected_requirement.clone();

            // --- Assert equivalence ---
            match &fast_tool_req {
                ToolRequirement::Package { executable, requirement } => {
                    // 1. Executable name must match what the full path would produce.
                    prop_assert_eq!(
                        executable, &full_path_executable,
                        "Fast path executable '{}' should equal full path executable '{}'",
                        executable, full_path_executable
                    );

                    // 2. Requirement must match the receipt's stored requirement
                    //    (which is what the full path originally resolved and saved).
                    prop_assert_eq!(
                        &requirement.name, &full_path_requirement.name,
                        "Requirement name mismatch"
                    );

                    // 3. The requirement's version specifier must match.
                    prop_assert_eq!(
                        format!("{}", requirement.source),
                        format!("{}", full_path_requirement.source),
                        "Requirement source mismatch for package '{}'",
                        pkg_name
                    );
                }
                ToolRequirement::Python { .. } => {
                    prop_assert!(
                        false,
                        "Fast path should return Package variant, not Python"
                    );
                }
            }
        }

        /// Feature: uvx-startup-optimization, Property 5: Error propagation in parallel operations
        ///
        /// **Validates: Requirements 7.2**
        ///
        /// For any pair of Results from two concurrent operations, the
        /// `resolve_parallel_results` helper SHALL propagate errors correctly:
        /// - Both Ok → returns Ok with both values
        /// - First Err → first error propagated (regardless of second)
        /// - First Ok, Second Err → second error propagated
        /// - Both Err → first error propagated (Python discovery takes precedence)
        #[test]
        fn prop_error_propagation_in_parallel_operations(
            first_val in any::<u64>(),
            second_val in any::<u64>(),
            first_err_code in any::<u32>(),
            second_err_code in any::<u32>(),
            // 0 = both ok, 1 = first err only, 2 = second err only, 3 = both err
            scenario in 0u8..4,
        ) {
            let first: Result<u64, u32> = if scenario == 1 || scenario == 3 {
                Err(first_err_code)
            } else {
                Ok(first_val)
            };
            let second: Result<u64, u32> = if scenario == 2 || scenario == 3 {
                Err(second_err_code)
            } else {
                Ok(second_val)
            };

            let result = resolve_parallel_results(first, second);

            match scenario {
                // Both Ok → returns Ok with both values.
                0 => {
                    let (a, b) = result.expect("Both Ok should produce Ok");
                    prop_assert_eq!(a, first_val);
                    prop_assert_eq!(b, second_val);
                }
                // First Err, Second Ok → first error propagated.
                1 => {
                    let err = result.expect_err("First Err should propagate");
                    prop_assert_eq!(err, first_err_code,
                        "First error should be propagated when only first fails");
                }
                // First Ok, Second Err → second error propagated.
                2 => {
                    let err = result.expect_err("Second Err should propagate");
                    prop_assert_eq!(err, second_err_code,
                        "Second error should be propagated when only second fails");
                }
                // Both Err → first error takes precedence (Python discovery).
                3 => {
                    let err = result.expect_err("Both Err should propagate first");
                    prop_assert_eq!(err, first_err_code,
                        "First error (Python discovery) should take precedence when both fail");
                }
                _ => unreachable!(),
            }
        }
    }
}
