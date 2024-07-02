use anyhow::{Context, Result};

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_distribution::pyproject::DependencyType;
use uv_distribution::pyproject_mut::PyProjectTomlMut;
use uv_distribution::{ProjectWorkspace, VirtualProject, Workspace};
use uv_toolchain::{ToolchainFetch, ToolchainPreference, ToolchainRequest};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::pip::operations::Modifications;
use crate::commands::project::SharedState;
use crate::commands::{project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{InstallerSettings, ResolverSettings};

/// Remove one or more packages from the project requirements.
pub(crate) async fn remove(
    requirements: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
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
        match dependency_type {
            DependencyType::Production => {
                let deps = pyproject.remove_dependency(&req)?;
                if deps.is_empty() {
                    warn_if_present(&req, &pyproject);
                    anyhow::bail!("The dependency `{req}` could not be found in `dependencies`");
                }
            }
            DependencyType::Dev => {
                let deps = pyproject.remove_dev_dependency(&req)?;
                if deps.is_empty() {
                    warn_if_present(&req, &pyproject);
                    anyhow::bail!(
                        "The dependency `{req}` could not be found in `dev-dependencies`"
                    );
                }
            }
            DependencyType::Optional(ref group) => {
                let deps = pyproject.remove_optional_dependency(&req, group)?;
                if deps.is_empty() {
                    warn_if_present(&req, &pyproject);
                    anyhow::bail!(
                        "The dependency `{req}` could not be found in `optional-dependencies`"
                    );
                }
            }
        }
    }

    // Save the modified `pyproject.toml`.
    fs_err::write(
        project.current_project().root().join("pyproject.toml"),
        pyproject.to_string(),
    )?;

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(ToolchainRequest::parse),
        toolchain_preference,
        toolchain_fetch,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Use the default settings.
    let settings = ResolverSettings::default();

    // Initialize any shared state.
    let state = SharedState::default();

    // Lock and sync the environment.
    let lock = project::lock::do_lock(
        project.workspace(),
        venv.interpreter(),
        settings.as_ref(),
        &state,
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
        &VirtualProject::Project(project),
        &venv,
        &lock,
        extras,
        dev,
        Modifications::Exact,
        settings.as_ref(),
        &state,
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

/// Emit a warning if a dependency with the given name is present as any dependency type.
///
/// This is useful when a dependency of the user-specified type was not found, but it may be present
/// elsewhere.
fn warn_if_present(name: &PackageName, pyproject: &PyProjectTomlMut) {
    for dep_ty in pyproject.find_dependency(name) {
        match dep_ty {
            DependencyType::Production => {
                warn_user!("`{name}` is a production dependency");
            }
            DependencyType::Dev => {
                warn_user!("`{name}` is a development dependency; try calling `uv remove --dev`");
            }
            DependencyType::Optional(group) => {
                warn_user!(
                    "`{name}` is an optional dependency; try calling `uv remove --optional {group}`"
                );
            }
        }
    }
}
