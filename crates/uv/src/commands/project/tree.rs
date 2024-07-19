use std::fmt::Write;

use anyhow::Result;

use indexmap::IndexMap;
use owo_colors::OwoColorize;
use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_distribution::Workspace;
use uv_python::{PythonFetch, PythonPreference, PythonRequest};

use uv_warnings::warn_user_once;

use crate::commands::pip::tree::DisplayDependencyGraph;
use crate::commands::project::FoundInterpreter;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

use super::SharedState;

/// Run a command.
pub(crate) async fn tree(
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
        warn_user_once!("`uv run` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let workspace = Workspace::discover(&std::env::current_dir()?, None).await?;

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

    // Read the existing lockfile.
    let existing = project::lock::read(&workspace).await?;

    // Update the lock file, if necessary.
    let lock = project::lock::do_lock(
        &workspace,
        &interpreter,
        existing.as_ref(),
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
    if !existing.is_some_and(|existing| existing == lock) {
        project::lock::commit(&lock, &workspace).await?;
    }

    // Read packages from the lockfile.
    let mut packages: IndexMap<_, Vec<_>> = IndexMap::new();
    for dist in lock.into_distributions() {
        let name = dist.name().clone();
        let metadata = dist.to_metadata(workspace.install_path())?;
        packages.entry(name).or_default().push(metadata);
    }

    // Render the tree.
    let rendered_tree = DisplayDependencyGraph::new(
        depth.into(),
        prune,
        package,
        no_dedupe,
        invert,
        interpreter.markers(),
        packages,
    )
    .render()
    .join("\n");

    writeln!(printer.stdout(), "{rendered_tree}")?;

    if rendered_tree.contains('*') {
        let message = if no_dedupe {
            "(*) Package tree is a cycle and cannot be shown".italic()
        } else {
            "(*) Package tree already displayed".italic()
        };
        writeln!(printer.stdout(), "{message}")?;
    }

    Ok(ExitStatus::Success)
}
