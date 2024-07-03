use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use itertools::Itertools;
use tokio::process::Command;
use tracing::debug;

use cache_key::digest;
use distribution_types::UnresolvedRequirementSpecification;
use pep440_rs::Version;
use uv_cache::{Cache, CacheBucket};
use uv_cli::ExternalCommand;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, PreviewMode};
use uv_installer::{SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonEnvironment, PythonFetch, PythonInstallation, PythonPreference,
    PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_resolver::Lock;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::pip::operations::Modifications;
use crate::commands::project::{resolve_environment, sync_environment};
use crate::commands::tool::common::resolve_requirements;
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Run a command.
pub(crate) async fn run(
    command: ExternalCommand,
    from: Option<String>,
    with: Vec<String>,
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
        warn_user_once!("`uv tool run` is experimental and may change without warning.");
    }

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
    let environment = get_or_create_environment(
        &from,
        &with,
        python.as_deref(),
        &settings,
        isolated,
        preview,
        python_preference,
        python_fetch,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // TODO(zanieb): Determine the command via the package entry points
    let command = target;

    // Construct the command
    let mut process = Command::new(command.as_ref());
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

    // Construct the `PYTHONPATH` environment variable.
    let new_python_path = std::env::join_paths(
        environment.site_packages().map(PathBuf::from).chain(
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

/// Get or create a [`PythonEnvironment`] in which to run the specified tools.
///
/// If the target tool is already installed in a compatible environment, returns that
/// [`PythonEnvironment`]. Otherwise, creates an ephemeral environment.
async fn get_or_create_environment(
    from: &str,
    with: &[String],
    python: Option<&str>,
    settings: &ResolverInstallerSettings,
    isolated: bool,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let python_request = python.map(PythonRequest::parse);

    // Discover an interpreter.
    let interpreter = PythonInstallation::find_or_fetch(
        python_request.clone(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_fetch,
        &client_builder,
        cache,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the `from` requirement.
    let from = {
        resolve_requirements(
            std::iter::once(from),
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

    // Combine the `from` and `with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(from.clone());
        requirements.extend(
            resolve_requirements(
                with.iter().map(String::as_str),
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
    if !isolated {
        let installed_tools = InstalledTools::from_settings()?;

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
                return Ok(environment);
            }
        }
    }

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool.
    // TODO(zanieb): Determine if we should layer on top of the project environment if it is present.

    // If necessary, create an environment for the ephemeral requirements.
    debug!("Syncing ephemeral environment.");

    let spec = RequirementsSpecification::from_requirements(requirements.clone());

    // Resolve the requirements with the interpreter.
    let resolution = resolve_environment(
        &interpreter,
        spec,
        settings,
        &state,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Hash the resolution by hashing the generated lockfile.
    // TODO(charlie): If the resolution contains any mutable metadata (like a path or URL
    // dependency), skip this step.
    let lock = Lock::from_resolution_graph(&resolution)?;
    let toml = lock.to_toml()?;
    let resolution_hash = digest(&toml);

    // Hash the interpreter by hashing the sysconfig data.
    // TODO(charlie): Come up with a robust hash for the interpreter.
    let interpreter_hash = digest(&interpreter.sys_executable());

    // Search in the content-addressed cache. It's first addressed by interpreter metadata, and then
    // by the requirements. Alternatively, we could just address by the requirements... and then
    // check if the interpreter is compatible with the request.
    let cache_entry = cache.entry(CacheBucket::Tools, interpreter_hash, resolution_hash);

    match PythonEnvironment::from_root(cache_entry.path(), cache) {
        Ok(environment) => {
            // TODO(charlie): Validate that the environment is complete (with something like a tool
            // receipt).
            debug!(
                "Using ephemeral environment at: `{}`",
                cache_entry.path().display()
            );
            Ok(environment)
        }
        Err(uv_python::Error::MissingEnvironment(_)) => {
            // TODO(charlie): Lock the cache entry. (We have to build tool environments in-place, as
            // tools contain absolute paths to the interpreter. Otherwise, we'd build in a separate
            // directory and then move the environment into the cache.)
            debug!(
                "Creating ephemeral environment at: `{}`",
                cache_entry.path().display()
            );

            // Create a virtual environment in the cache.
            fs_err::create_dir_all(cache_entry.path())?;

            let venv = uv_virtualenv::create_venv(
                cache_entry.path(),
                interpreter,
                uv_virtualenv::Prompt::None,
                false,
                false,
            )?;

            // Install the ephemeral requirements.
            let venv = sync_environment(
                venv,
                &resolution.into(),
                Modifications::Exact,
                settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            Ok(venv)
        }
        Err(err) => Err(err.into()),
    }
}

/// Parse a target into a command name and a requirement.
fn parse_target(target: &OsString) -> Result<(Cow<OsString>, Cow<str>)> {
    let Some(target_str) = target.to_str() else {
        return Err(anyhow::anyhow!("Tool command could not be parsed as UTF-8 string. Use `--from` to specify the package name."));
    };

    // e.g. `uv`, no special handling
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
        debug!("Ignoring non-package name `{}` in command", name);
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
    debug!("Ignoring invalid version request `{}` in command", version);
    Ok((Cow::Borrowed(target), Cow::Borrowed(target_str)))
}
