use anyhow::Result;
use std::str::FromStr;
use uv_distribution::pyproject_mut::PyProjectTomlMut;

use distribution_types::IndexLocations;
use pep508_rs::Requirement;
use uv_cache::Cache;
use uv_configuration::{ExtrasSpecification, PreviewMode, Upgrade};
use uv_distribution::ProjectWorkspace;
use uv_warnings::warn_user;

use crate::commands::{project, ExitStatus};
use crate::printer::Printer;

/// Add one or more packages to the project requirements.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn add(
    requirements: Vec<String>,
    python: Option<String>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv add` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(&std::env::current_dir()?, None).await?;

    let mut pyproject = PyProjectTomlMut::from_toml(project.current_project().pyproject_toml())?;
    for req in requirements {
        let req = Requirement::from_str(&req)?;
        pyproject.add_dependency(&req)?;
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(
        project.workspace(),
        python.as_deref(),
        preview,
        cache,
        printer,
    )?;

    let index_locations = IndexLocations::default();
    let upgrade = Upgrade::default();
    let exclude_newer = None;

    // Lock and sync the environment.
    let root_project_name = project
        .current_project()
        .pyproject_toml()
        .project
        .as_ref()
        .map(|project| project.name.clone());

    let lock = project::lock::do_lock(
        root_project_name,
        project.workspace(),
        venv.interpreter(),
        &index_locations,
        upgrade,
        exclude_newer,
        preview,
        cache,
        printer,
    )
    .await?;

    // Perform a full sync, because we don't know what exactly is affected by the removal.
    // TODO(ibraheem): Should we accept CLI overrides for this? Should we even sync here?
    let extras = ExtrasSpecification::All;
    let dev = true;

    project::sync::do_sync(
        project.project_name(),
        project.workspace().root(),
        &venv,
        &lock,
        &index_locations,
        extras,
        dev,
        preview,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}
