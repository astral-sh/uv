use anstream::eprint;
use anyhow::Result;

use distribution_types::{IndexLocations, UnresolvedRequirementSpecification};
use install_wheel_rs::linker::LinkMode;
use uv_cache::Cache;
use uv_client::RegistryClientBuilder;
use uv_configuration::{
    Concurrency, ConfigSettings, ExtrasSpecification, NoBinary, NoBuild, PreviewMode, Reinstall,
    SetupPyStrategy, Upgrade,
};
use uv_dispatch::BuildDispatch;
use uv_distribution::ProjectWorkspace;
use uv_git::GitResolver;
use uv_interpreter::PythonEnvironment;
use uv_requirements::upgrade::{read_lockfile, LockedRequirements};
use uv_resolver::{ExcludeNewer, FlatIndex, InMemoryIndex, Lock, OptionsBuilder};
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
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv lock` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(&std::env::current_dir()?, None).await?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(&project, preview, cache, printer)?;

    // Perform the lock operation.
    match do_lock(
        &project,
        &venv,
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
    project: &ProjectWorkspace,
    venv: &PythonEnvironment,
    index_locations: &IndexLocations,
    upgrade: Upgrade,
    exclude_newer: Option<ExcludeNewer>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<Lock, ProjectError> {
    // When locking, include the project itself (as editable).
    let requirements = project
        .requirements()
        .into_iter()
        .map(UnresolvedRequirementSpecification::from)
        .collect::<Vec<_>>();
    let constraints = vec![];
    let overrides = vec![];
    let source_trees = vec![];
    let project_name = project.project_name().clone();

    // Determine the tags, markers, and interpreter to use for resolution.
    let interpreter = venv.interpreter();
    let tags = venv.interpreter().tags()?;
    let markers = venv.interpreter().markers();
    let requires_python = project
        .current_project()
        .pyproject_toml()
        .project
        .as_ref()
        .and_then(|project| project.requires_python.as_ref());

    // Initialize the registry client.
    // TODO(zanieb): Support client options e.g. offline, tls, etc.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .markers(markers)
        .platform(venv.interpreter().platform())
        .build();

    // TODO(charlie): Respect project configuration.
    let build_isolation = BuildIsolation::default();
    let concurrency = Concurrency::default();
    let config_settings = ConfigSettings::default();
    let extras = ExtrasSpecification::default();
    let flat_index = FlatIndex::default();
    let in_flight = InFlight::default();
    let index = InMemoryIndex::default();
    let link_mode = LinkMode::default();
    let no_binary = NoBinary::default();
    let no_build = NoBuild::default();
    let reinstall = Reinstall::default();
    let setup_py = SetupPyStrategy::default();

    let hasher = HashStrategy::Generate;
    let options = OptionsBuilder::new().exclude_newer(exclude_newer).build();

    // If an existing lockfile exists, build up a set of preferences.
    let LockedRequirements { preferences, git } = read_lockfile(project, &upgrade).await?;

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
        source_trees,
        Some(project_name),
        &extras,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &reinstall,
        &upgrade,
        interpreter,
        tags,
        None,
        requires_python,
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
    fs_err::tokio::write(
        project.workspace().root().join("uv.lock"),
        encoded.as_bytes(),
    )
    .await?;

    Ok(lock)
}
