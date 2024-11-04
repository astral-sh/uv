use std::path::Path;

use anstream::print;
use anyhow::Result;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{
    Concurrency, DevGroupsSpecification, LowerBound, TargetTriple, TrustedHost,
};
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest, PythonVersion};
use uv_resolver::TreeDisplay;
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::lock::LockMode;
use crate::commands::project::{
    default_dependency_groups, DependencyGroupsTarget, ProjectInterpreter,
};
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
    project_dir: &Path,
    dev: DevGroupsSpecification,
    locked: bool,
    frozen: bool,
    universal: bool,
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python: Option<String>,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(project_dir, &DiscoveryOptions::default()).await?;

    // Validate that any referenced dependency groups are defined in the workspace.
    if !frozen {
        let target = DependencyGroupsTarget::Workspace(&workspace);
        target.validate(&dev)?;
    }

    // Determine the default groups to include.
    let defaults = default_dependency_groups(workspace.pyproject_toml())?;

    // Find an interpreter for the project, unless `--frozen` and `--universal` are both set.
    let interpreter = if frozen && universal {
        None
    } else {
        Some(
            ProjectInterpreter::discover(
                &workspace,
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_downloads,
                connectivity,
                native_tls,
                allow_insecure_host,
                no_config,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        )
    };

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = SharedState::default();

    // Update the lockfile, if necessary.
    let lock = project::lock::do_safe_lock(
        mode,
        &workspace,
        settings.as_ref(),
        LowerBound::Allow,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_lock();

    // Determine the markers to use for resolution.
    let markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    // Render the tree.
    let tree = TreeDisplay::new(
        &lock,
        markers.as_ref(),
        depth.into(),
        &prune,
        &package,
        &dev.with_defaults(defaults),
        no_dedupe,
        invert,
    );

    print!("{tree}");

    Ok(ExitStatus::Success)
}
