use std::fmt::Write;

use anyhow::Result;

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_fs::CWD;
use uv_python::{PythonFetch, PythonPreference, PythonRequest};
use uv_resolver::TreeDisplay;
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, Workspace};

use crate::commands::project::FoundInterpreter;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

use super::SharedState;

/// Run a command.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn tree(
    locked: bool,
    frozen: bool,
    filter: bool,
    depth: u8,
    prune: Vec<PackageName>,
    package: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    python: Option<String>,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tree` is experimental and may change without warning");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&CWD, &DiscoveryOptions::default()).await?;

    // Find an interpreter for the project
    let interpreter = FoundInterpreter::discover(
        &workspace,
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_fetch,
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
        &SharedState::default(),
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Render the tree.
    let tree = TreeDisplay::new(
        &lock.lock,
        filter.then(|| interpreter.markers()),
        depth.into(),
        prune,
        package,
        no_dedupe,
        invert,
    );

    write!(printer.stdout(), "{tree}")?;

    Ok(ExitStatus::Success)
}
