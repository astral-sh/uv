use anyhow::Result;
use pep508_rs::PackageName;
use uv_distribution::pyproject_mut::PyProjectTomlMut;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode, Upgrade};
use uv_distribution::ProjectWorkspace;
use uv_warnings::warn_user;

use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::InstallerSettings;

/// Remove one or more packages from the project requirements.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn remove(
    requirements: Vec<PackageName>,
    python: Option<String>,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
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
        if pyproject.remove_dependency(&req)?.is_empty() {
            anyhow::bail!(
                "The dependency `{}` could not be found in `dependencies`",
                req
            );
        }
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(project.workspace(), python.as_deref(), cache, printer)?;

    // Use the default settings.
    let settings = InstallerSettings::default();
    let upgrade = Upgrade::default();

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
        upgrade,
        &settings,
        preview,
        connectivity,
        concurrency,
        native_tls,
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
        extras,
        dev,
        &settings,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}
