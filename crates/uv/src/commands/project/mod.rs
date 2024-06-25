use std::fmt::Write;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::Resolution;
use pep440_rs::Version;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::Workspace;
use uv_fs::Simplified;
use uv_git::GitResolver;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::{FlatIndex, InMemoryIndex, OptionsBuilder, PythonRequirement, RequiresPython};
use uv_toolchain::{
    request_from_version_file, EnvironmentPreference, Interpreter, PythonEnvironment, Toolchain,
    ToolchainPreference, ToolchainRequest, VersionRequest,
};
use uv_types::{BuildIsolation, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::pip;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

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

/// Compute the `Requires-Python` bound for the [`Workspace`].
///
/// For a [`Workspace`] with multiple packages, the `Requires-Python` bound is the union of the
/// `Requires-Python` bounds of all the packages.
pub(crate) fn find_requires_python(
    workspace: &Workspace,
) -> Result<Option<RequiresPython>, uv_resolver::RequiresPythonError> {
    RequiresPython::union(workspace.packages().values().filter_map(|member| {
        member
            .pyproject_toml()
            .project
            .as_ref()
            .and_then(|project| project.requires_python.as_ref())
    }))
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
    requested_python: Option<&ToolchainRequest>,
    requires_python: Option<&RequiresPython>,
    cache: &Cache,
) -> bool {
    // `--python` has highest precedence, after that we check `requires_python` from
    // `pyproject.toml`. If `--python` and `requires_python` are mutually incompatible,
    // we'll fail at the build or at last the install step when we aren't able to install
    // the editable wheel for the current project into the venv.
    // TODO(konsti): Do we want to support a workspace python version requirement?
    if let Some(request) = requested_python {
        if request.satisfied(interpreter, cache) {
            debug!("Interpreter meets the requested python {}", request);
            return true;
        }

        debug!("Interpreter does not meet the request {}", request);
        return false;
    };

    if let Some(requires_python) = requires_python {
        if requires_python.contains(interpreter.python_version()) {
            debug!("Interpreter meets the project `Requires-Python` constraint {requires_python}");
            return true;
        }

        debug!(
            "Interpreter does not meet the project `Requires-Python` constraint {requires_python}"
        );
        return false;
    };

    // No requirement to check
    true
}

/// Find the interpreter to use in the current project.
pub(crate) async fn find_interpreter(
    workspace: &Workspace,
    python_request: Option<ToolchainRequest>,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<Interpreter, ProjectError> {
    let requires_python = find_requires_python(workspace)?;

    // (1) Explicit request from user
    let python_request = if let Some(request) = python_request {
        Some(request)
    // (2) Request from `.python-version`
    } else if let Some(request) = request_from_version_file().await? {
        Some(request)
    // (3) `Requires-Python` in `pyproject.toml`
    } else {
        requires_python
            .as_ref()
            .map(RequiresPython::specifiers)
            .map(|specifiers| ToolchainRequest::Version(VersionRequest::Range(specifiers.clone())))
    };

    // Read from the virtual environment first
    match find_environment(workspace, cache) {
        Ok(venv) => {
            if interpreter_meets_requirements(
                venv.interpreter(),
                python_request.as_ref(),
                requires_python.as_ref(),
                cache,
            ) {
                return Ok(venv.into_interpreter());
            }
        }
        Err(uv_toolchain::Error::NotFound(_)) => {}
        Err(e) => return Err(e.into()),
    };

    let client_builder = BaseClientBuilder::default()
        .connectivity(connectivity)
        .native_tls(native_tls);

    // Locate the Python interpreter to use in the environment
    let interpreter = Toolchain::find_or_fetch(
        python_request,
        EnvironmentPreference::OnlySystem,
        toolchain_preference,
        client_builder,
        cache,
    )
    .await?
    .into_interpreter();

    writeln!(
        printer.stderr(),
        "Using Python {} interpreter at: {}",
        interpreter.python_version(),
        interpreter.sys_executable().user_display().cyan()
    )?;

    if let Some(requires_python) = requires_python.as_ref() {
        if !requires_python.contains(interpreter.python_version()) {
            warn_user!(
                "The Python interpreter ({}) is incompatible with the project Python requirement {}",
                interpreter.python_version(),
                requires_python
            );
        }
    }

    Ok(interpreter)
}

/// Initialize a virtual environment for the current project.
pub(crate) async fn init_environment(
    workspace: &Workspace,
    python: Option<ToolchainRequest>,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment, ProjectError> {
    let requires_python = find_requires_python(workspace)?;

    // Check if the environment exists and is sufficient
    match find_environment(workspace, cache) {
        Ok(venv) => {
            if interpreter_meets_requirements(
                venv.interpreter(),
                python.as_ref(),
                requires_python.as_ref(),
                cache,
            ) {
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
    let interpreter = find_interpreter(
        workspace,
        python,
        toolchain_preference,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_environment(
    venv: PythonEnvironment,
    requirements: &[RequirementsSource],
    settings: &ResolverInstallerSettings,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<PythonEnvironment> {
    // Extract the project settings.
    let ResolverInstallerSettings {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        config_setting,
        exclude_newer,
        link_mode,
        compile_bytecode,
        upgrade,
        reinstall,
        build_options,
    } = settings;

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(*keyring_provider);

    // Read all requirements from the provided sources.
    // TODO(zanieb): Consider allowing constraints and extras
    // TODO(zanieb): Allow specifying extras somehow
    let spec =
        RequirementsSpecification::from_sources(requirements, &[], &[], &client_builder).await?;

    // Check if the current environment satisfies the requirements
    let site_packages = SitePackages::from_environment(&venv)?;
    if spec.source_trees.is_empty() && reinstall.is_none() {
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
    let python_requirement = PythonRequirement::from_interpreter(interpreter);

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(*index_strategy)
        .keyring(*keyring_provider)
        .markers(markers)
        .platform(interpreter.platform())
        .build();

    let options = OptionsBuilder::new()
        .resolution_mode(*resolution)
        .prerelease_mode(*prerelease)
        .exclude_newer(*exclude_newer)
        .index_strategy(*index_strategy)
        .build();

    // Initialize any shared state.
    let git = GitResolver::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_isolation = BuildIsolation::default();
    let dev = Vec::default();
    let dry_run = false;
    let extras = ExtrasSpecification::default();
    let hasher = HashStrategy::default();
    let preferences = Vec::default();
    let setup_py = SetupPyStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(tags), &hasher, build_options)
    };

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
        *index_strategy,
        setup_py,
        config_setting,
        build_isolation,
        *link_mode,
        build_options,
        *exclude_newer,
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
        reinstall,
        upgrade,
        Some(tags),
        Some(markers),
        python_requirement,
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
            *index_strategy,
            setup_py,
            config_setting,
            build_isolation,
            *link_mode,
            build_options,
            *exclude_newer,
            concurrency,
            preview,
        )
    };

    // Sync the environment.
    pip::operations::install(
        &resolution,
        site_packages,
        pip::operations::Modifications::Sufficient,
        reinstall,
        build_options,
        *link_mode,
        *compile_bytecode,
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
