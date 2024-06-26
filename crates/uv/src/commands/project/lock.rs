use std::collections::Bound;

use anstream::eprint;

use distribution_types::UnresolvedRequirementSpecification;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, Reinstall, SetupPyStrategy};
use uv_dispatch::BuildDispatch;
use uv_distribution::{Workspace, DEV_DEPENDENCIES};
use uv_git::GitResolver;
use uv_requirements::upgrade::{read_lockfile, LockedRequirements};
use uv_resolver::{
    FlatIndex, InMemoryIndex, Lock, OptionsBuilder, PythonRequirement, RequiresPython,
};
use uv_toolchain::{Interpreter, ToolchainPreference, ToolchainRequest};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::project::{find_requires_python, ProjectError};
use crate::commands::{pip, project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{ResolverSettings, ResolverSettingsRef};

/// Resolve the project requirements into a lockfile.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lock(
    python: Option<String>,
    settings: ResolverSettings,
    preview: PreviewMode,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv lock` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&std::env::current_dir()?, None).await?;

    // Find an interpreter for the project
    let interpreter = project::find_interpreter(
        &workspace,
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Perform the lock operation.
    match do_lock(
        &workspace,
        &interpreter,
        settings.as_ref(),
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await
    {
        Ok(_) => Ok(ExitStatus::Success),
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            let report = miette::Report::msg(format!("{err}"))
                .context("No solution found when resolving dependencies:");
            eprint!("{report:?}");
            Ok(ExitStatus::Failure)
        }
        Err(err) => Err(err.into()),
    }
}

/// Lock the project requirements into a lockfile.
#[allow(clippy::too_many_arguments)]
pub(super) async fn do_lock(
    workspace: &Workspace,
    interpreter: &Interpreter,
    settings: ResolverSettingsRef<'_>,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<Lock, ProjectError> {
    // Extract the project settings.
    let ResolverSettingsRef {
        index_locations,
        index_strategy,
        keyring_provider,
        resolution,
        prerelease,
        config_setting,
        exclude_newer,
        link_mode,
        upgrade,
        build_options,
    } = settings;

    // When locking, include the project itself (as editable).
    let requirements = workspace
        .members_as_requirements()
        .into_iter()
        .map(UnresolvedRequirementSpecification::from)
        .collect();
    let overrides = workspace
        .overrides()
        .into_iter()
        .map(UnresolvedRequirementSpecification::from)
        .collect();
    let constraints = vec![];
    let dev = vec![DEV_DEPENDENCIES.clone()];
    let source_trees = vec![];

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    let requires_python = find_requires_python(workspace)?;

    let requires_python = if let Some(requires_python) = requires_python {
        if matches!(requires_python.bound(), Bound::Unbounded) {
            let default =
                RequiresPython::greater_than_equal_version(interpreter.python_minor_version());
            warn_user!("The workspace `requires-python` field does not contain a lower bound: `{requires_python}`. Set a lower bound to indicate the minimum compatible Python version (e.g., `{default}`).");
        }
        requires_python
    } else {
        let default =
            RequiresPython::greater_than_equal_version(interpreter.python_minor_version());
        warn_user!("No `requires-python` field found in the workspace. Defaulting to `{default}`.");
        default
    };

    let python_requirement = PythonRequirement::from_requires_python(interpreter, &requires_python);

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    let options = OptionsBuilder::new()
        .resolution_mode(resolution)
        .prerelease_mode(prerelease)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build();
    let hasher = HashStrategy::Generate;

    // Initialize any shared state.
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();

    // TODO(charlie): These are all default values. We should consider whether we want to make them
    // optional on the downstream APIs.
    let build_isolation = BuildIsolation::default();
    let extras = ExtrasSpecification::default();
    let setup_py = SetupPyStrategy::default();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, None, &hasher, build_options)
    };

    // If an existing lockfile exists, build up a set of preferences.
    let LockedRequirements { preferences, git } = read_lockfile(workspace, upgrade).await?;

    // Create the Git resolver.
    let git = GitResolver::from_refs(git);

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        cache,
        interpreter,
        index_locations,
        &flat_index,
        &index,
        &git,
        &in_flight,
        index_strategy,
        setup_py,
        config_setting,
        build_isolation,
        link_mode,
        build_options,
        exclude_newer,
        concurrency,
        preview,
    );

    // Resolve the requirements.
    let resolution = pip::operations::resolve(
        requirements,
        constraints,
        overrides,
        dev,
        source_trees,
        None,
        &extras,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &Reinstall::default(),
        upgrade,
        None,
        None,
        python_requirement,
        &client,
        &flat_index,
        &index,
        &build_dispatch,
        concurrency,
        options,
        printer,
        preview,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    pip::operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    // Write the lockfile to disk.
    let lock = Lock::from_resolution_graph(&resolution)?;
    let encoded = lock.to_toml()?;
    fs_err::tokio::write(workspace.root().join("uv.lock"), encoded.as_bytes()).await?;

    Ok(lock)
}
