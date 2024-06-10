use anyhow::Result;
use std::str::FromStr;
use uv_distribution::pyproject_mut::PyProjectTomlMut;

use distribution_types::IndexLocations;
use pep508_rs::Requirement;
use pypi_types::LenientRequirement;
use uv_cache::Cache;
use uv_configuration::{ExtrasSpecification, PreviewMode, Upgrade};
use uv_distribution::ProjectWorkspace;
use uv_warnings::warn_user;

use crate::commands::{project, ExitStatus};
use crate::printer::Printer;

/// Remove one or more packages from the project requirements.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn remove(
    requirements: Vec<String>,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv remove` is experimental and may change without warning.");
    }

    // Find the project requirements.
    let project = ProjectWorkspace::discover(&std::env::current_dir()?, None).await?;

    let mut pyproject = PyProjectTomlMut::from_toml(project.current_project().pyproject_toml())?;
    for req in requirements {
        let req = Requirement::from(LenientRequirement::from_str(&req)?);
        if pyproject.remove_dependency(&req)?.is_none() {
            anyhow::bail!(
                "The dependency `{}` could not be found in `dependencies`",
                req.name
            );
        }
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(project.workspace(), preview, cache, printer)?;

    let index_locations = IndexLocations::default();
    let upgrade = Upgrade::default();
    let extras = ExtrasSpecification::default();
    let exclude_newer = None;
    let dev = false; // We only add regular dependencies currently.

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

    project::sync::do_sync(
        &project,
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
