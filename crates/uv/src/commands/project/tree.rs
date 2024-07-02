use std::fmt::Write;

use anyhow::Result;

use indexmap::IndexMap;
use owo_colors::OwoColorize;
use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_distribution::Workspace;
use uv_toolchain::{ToolchainFetch, ToolchainPreference, ToolchainRequest};
use uv_warnings::warn_user_once;

use crate::commands::pip::tree::DisplayDependencyGraph;
use crate::commands::project::FoundInterpreter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ResolverSettings;

use super::lock::do_lock;
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
    toolchain_preference: ToolchainPreference,
    toolchain_fetch: ToolchainFetch,
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
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
        toolchain_fetch,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Update the lock file.
    let lock = do_lock(
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

    // Read packages from the lockfile.
    let mut packages: IndexMap<_, Vec<_>> = IndexMap::new();
    for dist in lock.into_distributions() {
        let name = dist.name().clone();
        let metadata = dist.into_metadata(workspace.root())?;
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
