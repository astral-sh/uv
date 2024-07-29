use anyhow::{Context, Result};

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, ExtrasSpecification, PreviewMode};
use uv_fs::CWD;
use uv_python::{PythonFetch, PythonPreference, PythonRequest};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::PyProjectTomlMut;
use uv_workspace::{DiscoveryOptions, ProjectWorkspace, VirtualProject, Workspace};

use crate::commands::pip::operations::Modifications;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Remove one or more packages from the project requirements.
pub(crate) async fn remove(
    locked: bool,
    frozen: bool,
    requirements: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
    settings: ResolverInstallerSettings,
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
        warn_user_once!("`uv remove` is experimental and may change without warning");
    }

    // Find the project in the workspace.
    let project = if let Some(package) = package {
        Workspace::discover(&CWD, &DiscoveryOptions::default())
            .await?
            .with_current_project(package.clone())
            .with_context(|| format!("Package `{package}` not found in workspace"))?
    } else {
        ProjectWorkspace::discover(&CWD, &DiscoveryOptions::default()).await?
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

    // If `--frozen`, exit early. There's no reason to lock and sync, and we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_fetch,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Initialize any shared state.
    let state = SharedState::default();

    // Lock and sync the environment, if necessary.
    let lock = project::lock::do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.as_ref().into(),
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
    let extras = ExtrasSpecification::All;
    let dev = true;

    project::sync::do_sync(
        &VirtualProject::Project(project),
        &venv,
        &lock.lock,
        &extras,
        dev,
        Modifications::Exact,
        settings.as_ref().into(),
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
