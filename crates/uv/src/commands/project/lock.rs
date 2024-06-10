use anstream::eprint;

use distribution_types::{IndexLocations, UnresolvedRequirementSpecification};
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::{FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, ConfigSettings, ExtrasSpecification, NoBinary, NoBuild, PreviewMode, Reinstall,
    SetupPyStrategy, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::{Workspace, DEV_DEPENDENCIES};
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_requirements::upgrade::{read_lockfile, LockedRequirements};
use uv_resolver::{ExcludeNewer, FlatIndex, InMemoryIndex, Lock, OptionsBuilder, RequiresPython};
use uv_toolchain::{Interpreter, SystemPython, Toolchain, ToolchainRequest};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::project::ProjectError;
use crate::commands::{pip, project, ExitStatus};
use crate::printer::Printer;

/// Resolve the project requirements into a lockfile.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lock(
    index_locations: IndexLocations,
    upgrade: Upgrade,
    exclude_newer: Option<ExcludeNewer>,
    python: Option<String>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv lock` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&std::env::current_dir()?, None).await?;

    // Find an interpreter for the project
    let interpreter = match project::find_environment(&workspace, cache) {
        Ok(environment) => {
            let interpreter = environment.into_interpreter();
            if let Some(python) = python.as_deref() {
                let request = ToolchainRequest::parse(python);
                if request.satisfied(&interpreter, cache) {
                    interpreter
                } else {
                    Toolchain::find_requested(python, SystemPython::Allowed, preview, cache)?
                        .into_interpreter()
                }
            } else {
                interpreter
            }
        }
        Err(uv_toolchain::Error::NotFound(_)) => {
            Toolchain::find(python.as_deref(), SystemPython::Allowed, preview, cache)?
                .into_interpreter()
        }
        Err(err) => return Err(err.into()),
    };

    // Perform the lock operation.
    let root_project_name = workspace.root_member().and_then(|member| {
        member
            .pyproject_toml()
            .project
            .as_ref()
            .map(|project| project.name.clone())
    });
    match do_lock(
        root_project_name,
        &workspace,
        &interpreter,
        &index_locations,
        upgrade,
        exclude_newer,
        preview,
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
    root_project_name: Option<PackageName>,
    workspace: &Workspace,
    interpreter: &Interpreter,
    index_locations: &IndexLocations,
    upgrade: Upgrade,
    exclude_newer: Option<ExcludeNewer>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<Lock, ProjectError> {
    // When locking, include the project itself (as editable).
    let requirements = workspace
        .members_as_requirements()
        .into_iter()
        .map(UnresolvedRequirementSpecification::from)
        .collect();
    let constraints = vec![];
    let overrides = vec![];
    let dev = vec![DEV_DEPENDENCIES.clone()];

    let source_trees = vec![];

    // Determine the supported Python range. If no range is defined, and warn and default to the
    // current minor version.
    //
    // For a workspace, we compute the union of all workspace requires-python values, ensuring we
    // keep track of `None` vs. a full range.
    let requires_python_workspace =
        RequiresPython::union(workspace.packages().values().filter_map(|member| {
            member
                .pyproject_toml()
                .project
                .as_ref()
                .and_then(|project| project.requires_python.as_ref())
        }))?;

    let requires_python = if let Some(requires_python) = requires_python_workspace {
        requires_python
    } else {
        let requires_python =
            RequiresPython::greater_than_equal_version(interpreter.python_minor_version());
        if let Some(root_project_name) = root_project_name.as_ref() {
            warn_user!(
                "No `requires-python` field found in `{root_project_name}`. Defaulting to `{requires_python}`.",
            );
        } else {
            warn_user!(
                "No `requires-python` field found in workspace. Defaulting to `{requires_python}`.",
            );
        }
        requires_python
    };

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let extras = ExtrasSpecification::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let reinstall = Reinstall::default();
    let setup_py = SetupPyStrategy::default();

    let hasher = HashStrategy::Generate;
    let options = OptionsBuilder::new().exclude_newer(exclude_newer).build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, None, &hasher, &no_build, &no_binary)
    };

    // If an existing lockfile exists, build up a set of preferences.
    let LockedRequirements { preferences, git } = read_lockfile(workspace, &upgrade).await?;

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
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &no_build,
        &no_binary,
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
        root_project_name,
        &extras,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &reinstall,
        &upgrade,
        interpreter,
        None,
        None,
        Some(&requires_python),
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
