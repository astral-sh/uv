use std::fmt::Write;

use anyhow::Result;

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, TargetTriple};
use uv_fs::CWD;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest, PythonVersion};
use uv_resolver::TreeDisplay;
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::FoundInterpreter;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
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
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Find the project requirements.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = FoundInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Update the lockfile, if necessary.
    let lock = project::lock::do_safe_lock(
        locked,
        frozen,
        &workspace,
        &interpreter,
        settings.as_ref(),
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
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
        no_dedupe,
        invert,
    );

    write!(printer.stdout(), "{tree}")?;

    Ok(ExitStatus::Success)
}
