use anyhow::{Context, Result};
use std::fmt::Write;
use std::path::Path;
use uv_settings::PythonInstallMirrors;

use owo_colors::OwoColorize;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{
    Concurrency, DevGroupsManifest, EditableMode, ExtrasSpecification, InstallOptions, LowerBound,
    TrustedHost,
};
use uv_fs::Simplified;
use uv_normalize::DEV_DEPENDENCIES;
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::InstallTarget;
use uv_scripts::Pep723Script;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::lock::LockMode;
use crate::commands::project::{default_dependency_groups, ProjectError};
use crate::commands::{diagnostics, project, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Remove one or more packages from the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn remove(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    no_sync: bool,
    packages: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
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
                "`--no-sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        Target::Script(script)
    } else {
        // Find the project in the workspace.
        let project = if let Some(package) = package {
            VirtualProject::Project(
                Workspace::discover(project_dir, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await?
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
                let dev_deps = toml.remove_dev_dependency(&package)?;
                let group_deps =
                    toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                if dev_deps.is_empty() && group_deps.is_empty() {
                    warn_if_present(&package, &toml);
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `dev-dependencies` or `dependency-groups.dev`"
                    );
                }
            }
            DependencyType::Optional(ref extra) => {
                let deps = toml.remove_optional_dependency(&package, extra)?;
                if deps.is_empty() {
                    warn_if_present(&package, &toml);
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `optional-dependencies`"
                    );
                }
            }
            DependencyType::Group(ref group) => {
                if group == &*DEV_DEPENDENCIES {
                    let dev_deps = toml.remove_dev_dependency(&package)?;
                    let group_deps =
                        toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                    if dev_deps.is_empty() && group_deps.is_empty() {
                        warn_if_present(&package, &toml);
                        anyhow::bail!(
                            "The dependency `{package}` could not be found in `dev-dependencies` or `dependency-groups.dev`"
                        );
                    }
                } else {
                    let deps = toml.remove_dependency_group_requirement(&package, group)?;
                    if deps.is_empty() {
                        warn_if_present(&package, &toml);
                        anyhow::bail!(
                            "The dependency `{package}` could not be found in `dependency-groups`"
                        );
                    }
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
        install_mirrors,
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        no_config,
        cache,
        printer,
    )
    .await?;

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(venv.interpreter())
    } else {
        LockMode::Write(venv.interpreter())
    };

    // Initialize any shared state.
    let state = SharedState::default();

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::do_safe_lock(
        mode,
        project.workspace(),
        settings.as_ref().into(),
        LowerBound::Allow,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    };

    if no_sync {
        return Ok(ExitStatus::Success);
    }

    // Perform a full sync, because we don't know what exactly is affected by the removal.
    // TODO(ibraheem): Should we accept CLI overrides for this? Should we even sync here?
    let extras = ExtrasSpecification::All;
    let install_options = InstallOptions::default();

    // Determine the default groups to include.
    let defaults = default_dependency_groups(project.pyproject_toml())?;

    // Identify the installation target.
    let target = match &project {
        VirtualProject::Project(project) => InstallTarget::Project {
            workspace: project.workspace(),
            name: project.project_name(),
            lock: &lock,
        },
        VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
            workspace,
            lock: &lock,
        },
    };

    match project::sync::do_sync(
        target,
        &venv,
        &extras,
        &DevGroupsManifest::from_defaults(defaults),
        EditableMode::Editable,
        install_options,
        Modifications::Exact,
        settings.as_ref().into(),
        Box::new(DefaultInstallLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await
    {
        Ok(()) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    }

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
            DependencyType::Group(group) => {
                warn_user!(
                    "`{name}` is in the `{group}` group; try calling `uv remove --group {group}`",
                );
            }
        }
    }
}
