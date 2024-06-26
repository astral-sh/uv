use anyhow::{Context, Result};

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_distribution::pyproject_mut::PyProjectTomlMut;
use uv_distribution::{ProjectWorkspace, Workspace};
use uv_toolchain::{ToolchainPreference, ToolchainRequest};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::pip::operations::Modifications;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{InstallerSettings, ResolverSettings};

/// Remove one or more packages from the project requirements.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn remove(
    requirements: Vec<PackageName>,
    dev: bool,
    package: Option<PackageName>,
    python: Option<String>,
    toolchain_preference: ToolchainPreference,
    preview: PreviewMode,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv remove` is experimental and may change without warning.");
    }

    // Find the project in the workspace.
    let project = if let Some(package) = package {
        Workspace::discover(&std::env::current_dir()?, None)
            .await?
            .with_current_project(package.clone())
            .with_context(|| format!("Package `{package}` not found in workspace"))?
    } else {
        ProjectWorkspace::discover(&std::env::current_dir()?, None).await?
    };

    let mut pyproject = PyProjectTomlMut::from_toml(project.current_project().pyproject_toml())?;
    for req in requirements {
        if dev {
            let deps = pyproject.remove_dev_dependency(&req)?;
            if deps.is_empty() {
                // Check if there is a matching regular dependency.
                if pyproject
                    .remove_dependency(&req)
                    .ok()
                    .filter(|deps| !deps.is_empty())
                    .is_some()
                {
                    warn_user!("`{req}` is not a development dependency; try calling `uv remove` without the `--dev` flag");
                }

                anyhow::bail!("The dependency `{req}` could not be found in `dev-dependencies`");
            }

            continue;
        }

        let deps = pyproject.remove_dependency(&req)?;
        if deps.is_empty() {
            // Check if there is a matching development dependency.
            if pyproject
                .remove_dev_dependency(&req)
                .ok()
                .filter(|deps| !deps.is_empty())
                .is_some()
            {
                warn_user!("`{req}` is a development dependency; try calling `uv remove --dev`");
            }

            anyhow::bail!("The dependency `{req}` could not be found in `dependencies`");
        }

        continue;
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Discover or create the virtual environment.
    let venv = project::init_environment(
        project.workspace(),
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Use the default settings.
    let settings = ResolverSettings::default();

    // Lock and sync the environment.
    let lock = project::lock::do_lock(
        project.workspace(),
        venv.interpreter(),
        settings.as_ref(),
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
    let settings = InstallerSettings::default();
    let extras = ExtrasSpecification::All;
    let dev = true;

    project::sync::do_sync(
        project.project_name(),
        project.workspace().root(),
        &venv,
        &lock,
        extras,
        dev,
        Modifications::Exact,
        settings.as_ref(),
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
