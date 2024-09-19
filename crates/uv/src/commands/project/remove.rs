use std::fmt::Write;

use anyhow::{Context, Result};

use owo_colors::OwoColorize;
use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, DevMode, EditableMode, ExtrasSpecification, InstallOptions};
use uv_fs::{Simplified, CWD};
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_scripts::Pep723Script;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, InstallTarget, VirtualProject, Workspace};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::{project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Remove one or more packages from the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn remove(
    locked: bool,
    frozen: bool,
    no_sync: bool,
    packages: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
    settings: ResolverInstallerSettings,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let target = if let Some(script) = script {
        // If we found a PEP 723 script and the user provided a project-only setting, warn.
        if package.is_some() {
            warn_user_once!(
                "`--package` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if locked {
            warn_user_once!(
                "`--locked` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if frozen {
            warn_user_once!(
                "`--frozen` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if no_sync {
            warn_user_once!(
                "`--no_sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        Target::Script(script)
    } else {
        // Find the project in the workspace.
        let project = if let Some(package) = package {
            VirtualProject::Project(
                Workspace::discover(&CWD, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await?
        };

        Target::Project(project)
    };

    let mut toml = match &target {
        Target::Script(script) => {
            PyProjectTomlMut::from_toml(&script.metadata.raw, DependencyTarget::Script)
        }
        Target::Project(project) => PyProjectTomlMut::from_toml(
            project.pyproject_toml().raw.as_ref(),
            DependencyTarget::PyProjectToml,
        ),
    }?;

    for package in packages {
        match dependency_type {
            DependencyType::Production => {
                let deps = toml.remove_dependency(&package)?;
                if deps.is_empty() {
                    warn_if_present(&package, &toml);
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `dependencies`"
                    );
                }
            }
            DependencyType::Dev => {
                let deps = toml.remove_dev_dependency(&package)?;
                if deps.is_empty() {
                    warn_if_present(&package, &toml);
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `dev-dependencies`"
                    );
                }
            }
            DependencyType::Optional(ref group) => {
                let deps = toml.remove_optional_dependency(&package, group)?;
                if deps.is_empty() {
                    warn_if_present(&package, &toml);
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `optional-dependencies`"
                    );
                }
            }
        }
    }

    // Save the modified dependencies.
    match &target {
        Target::Script(script) => {
            script.write(&toml.to_string()).await?;
        }
        Target::Project(project) => {
            let pyproject_path = project.root().join("pyproject.toml");
            fs_err::write(pyproject_path, toml.to_string())?;
        }
    };

    // If `--frozen`, exit early. There's no reason to lock and sync, and we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    let project = match target {
        Target::Project(project) => project,
        // If `--script`, exit early. There's no reason to lock and sync.
        Target::Script(script) => {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            return Ok(ExitStatus::Success);
        }
    };

    // Discover or create the virtual environment.
    let venv = project::get_or_init_environment(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // Lock and sync the environment, if necessary.
    let lock = project::lock::do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        venv.interpreter(),
        settings.as_ref().into(),
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?
    .into_lock();

    if no_sync {
        return Ok(ExitStatus::Success);
    }

    // Perform a full sync, because we don't know what exactly is affected by the removal.
    // TODO(ibraheem): Should we accept CLI overrides for this? Should we even sync here?
    let dev = DevMode::Include;
    let extras = ExtrasSpecification::All;
    let install_options = InstallOptions::default();

    // Initialize any shared state.
    let state = SharedState::default();

    project::sync::do_sync(
        InstallTarget::from(&project),
        &venv,
        &lock,
        &extras,
        dev,
        EditableMode::Editable,
        install_options,
        Modifications::Exact,
        settings.as_ref().into(),
        &state,
        Box::new(DefaultInstallLogger),
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    Ok(ExitStatus::Success)
}

/// Represents the destination where dependencies are added, either to a project or a script.
#[derive(Debug)]
enum Target {
    /// A PEP 723 script, with inline metadata.
    Project(VirtualProject),
    /// A project with a `pyproject.toml`.
    Script(Pep723Script),
}

/// Emit a warning if a dependency with the given name is present as any dependency type.
///
/// This is useful when a dependency of the user-specified type was not found, but it may be present
/// elsewhere.
fn warn_if_present(name: &PackageName, pyproject: &PyProjectTomlMut) {
    for dep_ty in pyproject.find_dependency(name, None) {
        match dep_ty {
            DependencyType::Production => {
                warn_user!("`{name}` is a production dependency");
            }
            DependencyType::Dev => {
                warn_user!("`{name}` is a development dependency; try calling `uv remove --dev`");
            }
            DependencyType::Optional(group) => {
                warn_user!(
                    "`{name}` is an optional dependency; try calling `uv remove --optional {group}`",
                );
            }
        }
    }
}
