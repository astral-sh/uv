use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{IndexLocations, Resolution};
use install_wheel_rs::linker::LinkMode;
use pep440_rs::{Version, VersionSpecifiers};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, ExtrasSpecification, PreviewMode, Reinstall,
    SetupPyStrategy, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::Workspace;
use uv_fs::Simplified;
use uv_git::GitResolver;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex, Options, RequiresPython};
use uv_toolchain::{
    Interpreter, PythonEnvironment, SystemPython, Toolchain, ToolchainRequest, VersionRequest,
};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::pip;
use crate::printer::Printer;

pub(crate) mod add;
pub(crate) mod lock;
pub(crate) mod remove;
pub(crate) mod run;
pub(crate) mod sync;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ProjectError {
    #[error("The current Python version ({0}) is not compatible with the locked Python requirement ({1})")]
    PythonIncompatibility(Version, RequiresPython),

    #[error(transparent)]
    Interpreter(#[from] uv_toolchain::Error),

    #[error(transparent)]
    Virtualenv(#[from] uv_virtualenv::Error),

    #[error(transparent)]
    Tags(#[from] platform_tags::TagsError),

    #[error(transparent)]
    FlatIndex(#[from] uv_client::FlatIndexError),

    #[error(transparent)]
    Lock(#[from] uv_resolver::LockError),

    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serialize(#[from] toml::ser::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    Operation(#[from] pip::operations::Error),

    #[error(transparent)]
    RequiresPython(#[from] uv_resolver::RequiresPythonError),
}

/// Find the virtual environment for the current project.
pub(crate) fn find_environment(
    workspace: &Workspace,
    cache: &Cache,
) -> Result<PythonEnvironment, uv_toolchain::Error> {
    PythonEnvironment::from_root(workspace.venv(), cache)
}

/// Check if the given interpreter satisfies the project's requirements.
pub(crate) fn interpreter_meets_requirements(
    interpreter: &Interpreter,
    requested_python: Option<&str>,
    requires_python: Option<&VersionSpecifiers>,
    cache: &Cache,
) -> bool {
    // `--python` has highest precedence, after that we check `requires_python` from
    // `pyproject.toml`. If `--python` and `requires_python` are mutually incompatible,
    // we'll fail at the build or at last the install step when we aren't able to install
    // the editable wheel for the current project into the venv.
    // TODO(konsti): Do we want to support a workspace python version requirement?
    if let Some(python) = requested_python {
        let request = ToolchainRequest::parse(python);
        if request.satisfied(interpreter, cache) {
            debug!("Interpreter meets the requested python {}", request);
            return true;
        }

        debug!("Interpreter does not meet the request {}", request);
        return false;
    };

    if let Some(requires_python) = requires_python {
        if requires_python.contains(interpreter.python_version()) {
            debug!(
                "Interpreter meets the project `Requires-Python` constraint {}",
                requires_python
            );
            return true;
        }

        debug!(
            "Interpreter does not meet the project `Requires-Python` constraint {}",
            requires_python
        );
        return false;
    };

    // No requirement to check
    true
}

/// Find the interpreter to use in the current project.
pub(crate) fn find_interpreter(
    workspace: &Workspace,
    python: Option<&str>,
    cache: &Cache,
    printer: Printer,
) -> Result<Interpreter, ProjectError> {
    let requires_python = workspace
        .root_member()
        .and_then(|root| root.project().requires_python.as_ref());

    // Read from the virtual environment first
    match find_environment(workspace, cache) {
        Ok(venv) => {
            if interpreter_meets_requirements(venv.interpreter(), python, requires_python, cache) {
                return Ok(venv.into_interpreter());
            }
        }
        Err(uv_toolchain::Error::NotFound(_)) => {}
        Err(e) => return Err(e.into()),
    };

    // Otherwise, find a system interpreter to use
    let interpreter = if let Some(request) = python.map(ToolchainRequest::parse).or(requires_python
        .map(|specifiers| ToolchainRequest::Version(VersionRequest::Range(specifiers.clone()))))
    {
        Toolchain::find_requested(
            &request,
            SystemPython::Required,
            PreviewMode::Enabled,
            cache,
        )
    } else {
        Toolchain::find(None, SystemPython::Required, PreviewMode::Enabled, cache)
    }?
    .into_interpreter();

    if let Some(requires_python) = requires_python {
        if !requires_python.contains(interpreter.python_version()) {
            warn_user!(
                "The Python {} you requested with {} is incompatible with the requirement of the \
                project of {}",
                interpreter.python_version(),
                python.unwrap_or("(default)"),
                requires_python
            );
        }
    }

    writeln!(
        printer.stderr(),
        "Using Python {} interpreter at: {}",
        interpreter.python_version(),
        interpreter.sys_executable().user_display().cyan()
    )?;

    Ok(interpreter)
}

/// Initialize a virtual environment for the current project.
pub(crate) fn init_environment(
    workspace: &Workspace,
    python: Option<&str>,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment, ProjectError> {
    let requires_python = workspace
        .root_member()
        .and_then(|root| root.project().requires_python.as_ref());

    // Check if the environment exists and is sufficient
    match find_environment(workspace, cache) {
        Ok(venv) => {
            if interpreter_meets_requirements(venv.interpreter(), python, requires_python, cache) {
                return Ok(venv);
            }

            // Remove the existing virtual environment if it doesn't meet the requirements
            writeln!(
                printer.stderr(),
                "Removing virtual environment at: {}",
                venv.root().user_display().cyan()
            )?;
            fs_err::remove_dir_all(venv.root())
                .context("Failed to remove existing virtual environment")?;
        }
        Err(uv_toolchain::Error::NotFound(_)) => {}
        Err(e) => return Err(e.into()),
    };

    // Find an interpreter to create the environment with
    let interpreter = find_interpreter(workspace, python, cache, printer)?;

    let venv = workspace.venv();
    writeln!(
        printer.stderr(),
        "Creating virtualenv at: {}",
        venv.user_display().cyan()
    )?;

    Ok(uv_virtualenv::create_venv(
        &venv,
        interpreter,
        uv_virtualenv::Prompt::None,
        false,
        false,
    )?)
}

/// Update a [`PythonEnvironment`] to satisfy a set of [`RequirementsSource`]s.
pub(crate) async fn update_environment(
    venv: PythonEnvironment,
    requirements: &[RequirementsSource],
    index_locations: &IndexLocations,
    connectivity: Connectivity,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<PythonEnvironment> {
    // TODO(zanieb): Support client configuration
    let client_builder = BaseClientBuilder::default().connectivity(connectivity);

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec =
        RequirementsSpecification::from_sources(requirements, &[], &[], &client_builder).await?;

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_executable(&venv)?;
    if spec.source_trees.is_empty() {
        match site_packages.satisfies(&spec.requirements, &spec.constraints)? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                debug!(
                    "All requirements satisfied: {}",
                    recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                        .join(" | ")
                );
                return Ok(venv);
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let compile = false;
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let dry_run = false;
    let extras = ExtrasSpecification::default();
    let flat_index = FlatIndex::default();
    let git = GitResolver::default();
    let dev = Vec::default();
    let hasher = HashStrategy::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let link_mode = LinkMode::default();
    let build_options = BuildOptions::default();
    let options = Options::default();
    let preferences = Vec::default();
    let reinstall = Reinstall::default();
    let setup_py = SetupPyStrategy::default();
    let upgrade = Upgrade::default();

    // Create a build dispatch.
    let resolve_dispatch = BuildDispatch::new(
        &client,
        cache,
        interpreter,
        index_locations,
        &flat_index,
        &index,
        &git,
        &in_flight,
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &build_options,
        concurrency,
        preview,
    );

    // Resolve the requirements.
    let resolution = match pip::operations::resolve(
        spec.requirements,
        spec.constraints,
        spec.overrides,
        dev,
        spec.source_trees,
        spec.project,
        &extras,
        preferences,
        site_packages.clone(),
        &hasher,
        &reinstall,
        &upgrade,
        interpreter,
        Some(tags),
        Some(markers),
        None,
        &client,
        &flat_index,
        &index,
        &resolve_dispatch,
        concurrency,
        options,
        printer,
        preview,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(err) => return Err(err.into()),
    };

    // Re-initialize the in-flight map.
    let in_flight = InFlight::default();

    // If we're running with `--reinstall`, initialize a separate `BuildDispatch`, since we may
    // end up removing some distributions from the environment.
    let install_dispatch = if reinstall.is_none() {
        resolve_dispatch
    } else {
        BuildDispatch::new(
            &client,
            cache,
            interpreter,
            index_locations,
            &flat_index,
            &index,
            &git,
            &in_flight,
            setup_py,
            &config_settings,
            build_isolation,
            link_mode,
            &build_options,
            concurrency,
            preview,
        )
    };

    // Sync the environment.
    pip::operations::install(
        &resolution,
        site_packages,
        pip::operations::Modifications::Sufficient,
        &reinstall,
        &build_options,
        link_mode,
        compile,
        index_locations,
        &hasher,
        tags,
        &client,
        &in_flight,
        concurrency,
        &install_dispatch,
        cache,
        &venv,
        dry_run,
        printer,
        preview,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(venv)
}
