use std::path::Path;

use anstream::print;
use anyhow::Result;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, DevMode, LowerBound, TargetTriple, TrustedHost};
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest, PythonVersion};
use uv_resolver::TreeDisplay;
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::ProjectInterpreter;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
    project_dir: &Path,
    dev: DevMode,
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
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(project_dir, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = ProjectInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Update the lockfile, if necessary.
    let lock = project::lock::do_safe_lock(
        locked,
        frozen,
        &workspace,
        &interpreter,
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
    let markers = resolution_markers(
        python_version.as_ref(),
        python_platform.as_ref(),
        &interpreter,
    );

    // Render the tree.
    let tree = TreeDisplay::new(
        &lock,
        (!universal).then_some(&markers),
        depth.into(),
        prune,
        package,
        dev,
        no_dedupe,
        invert,
    );

    print!("{tree}");

    Ok(ExitStatus::Success)
}
