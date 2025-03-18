use std::fmt::Write;
use std::io;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, EditableMode, ExtrasSpecification, InstallOptions,
    PreviewMode,
};
use uv_fs::Simplified;
use uv_normalize::DEV_DEPENDENCIES;
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_scripts::{Pep723ItemRef, Pep723Metadata, Pep723Script};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::DependencyType;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, Workspace, WorkspaceCache};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::add::{AddTarget, PythonTarget};
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    default_dependency_groups, ProjectEnvironment, ProjectError, ProjectInterpreter,
    ScriptInterpreter, UniversalState,
};
use crate::commands::{diagnostics, project, ExitStatus};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// Remove one or more packages from the project requirements.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn remove(
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    active: Option<bool>,
    no_sync: bool,
    packages: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
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
        RemoveTarget::Script(script)
    } else {
        // Find the project in the workspace.
        // No workspace caching since `uv remove` changes the workspace definition.
        let project = if let Some(package) = package {
            VirtualProject::Project(
                Workspace::discover(
                    project_dir,
                    &DiscoveryOptions::default(),
                    &WorkspaceCache::default(),
                )
                .await?
                .with_current_project(package.clone())
                .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                &WorkspaceCache::default(),
            )
            .await?
        };

        RemoveTarget::Project(project)
    };

    let mut toml = match &target {
        RemoveTarget::Script(script) => {
            PyProjectTomlMut::from_toml(&script.metadata.raw, DependencyTarget::Script)
        }
        RemoveTarget::Project(project) => PyProjectTomlMut::from_toml(
            project.pyproject_toml().raw.as_ref(),
            DependencyTarget::PyProjectToml,
        ),
    }?;

    for package in packages {
        match dependency_type {
            DependencyType::Production => {
                let deps = toml.remove_dependency(&package)?;
                if deps.is_empty() {
                    show_other_dependency_type_hint(printer, &package, &toml)?;
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `project.dependencies`"
                    )
                }
            }
            DependencyType::Dev => {
                let dev_deps = toml.remove_dev_dependency(&package)?;
                let group_deps =
                    toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                if dev_deps.is_empty() && group_deps.is_empty() {
                    show_other_dependency_type_hint(printer, &package, &toml)?;
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `tool.uv.dev-dependencies` or `tool.uv.dependency-groups.dev`"
                    );
                }
            }
            DependencyType::Optional(ref extra) => {
                let deps = toml.remove_optional_dependency(&package, extra)?;
                if deps.is_empty() {
                    show_other_dependency_type_hint(printer, &package, &toml)?;
                    anyhow::bail!(
                        "The dependency `{package}` could not be found in `project.optional-dependencies.{extra}`"
                    );
                }
            }
            DependencyType::Group(ref group) => {
                if group == &*DEV_DEPENDENCIES {
                    let dev_deps = toml.remove_dev_dependency(&package)?;
                    let group_deps =
                        toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                    if dev_deps.is_empty() && group_deps.is_empty() {
                        show_other_dependency_type_hint(printer, &package, &toml)?;
                        anyhow::bail!(
                            "The dependency `{package}` could not be found in `tool.uv.dev-dependencies` or `tool.uv.dependency-groups.dev`"
                        );
                    }
                } else {
                    let deps = toml.remove_dependency_group_requirement(&package, group)?;
                    if deps.is_empty() {
                        show_other_dependency_type_hint(printer, &package, &toml)?;
                        anyhow::bail!(
                            "The dependency `{package}` could not be found in `dependency-groups.{group}`"
                        );
                    }
                }
            }
        }
    }

    let content = toml.to_string();

    // Save the modified `pyproject.toml` or script.
    target.write(&content)?;

    // If `--frozen`, exit early. There's no reason to lock and sync, since we don't need a `uv.lock`
    // to exist at all.
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // If we're modifying a script, and lockfile doesn't exist, don't create it.
    if let RemoveTarget::Script(ref script) = target {
        if !LockTarget::from(script).lock_path().is_file() {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            return Ok(ExitStatus::Success);
        }
    }

    // Update the `pypackage.toml` in-memory.
    let target = target.update(&content)?;

    // Convert to an `AddTarget` by attaching the appropriate interpreter or environment.
    let target = match target {
        RemoveTarget::Project(project) => {
            if no_sync {
                // Discover the interpreter.
                let interpreter = ProjectInterpreter::discover(
                    project.workspace(),
                    project_dir,
                    python.as_deref().map(PythonRequest::parse),
                    &network_settings,
                    python_preference,
                    python_downloads,
                    &install_mirrors,
                    no_config,
                    active,
                    cache,
                    printer,
                )
                .await?
                .into_interpreter();

                AddTarget::Project(project, Box::new(PythonTarget::Interpreter(interpreter)))
            } else {
                // Discover or create the virtual environment.
                let environment = ProjectEnvironment::get_or_init(
                    project.workspace(),
                    python.as_deref().map(PythonRequest::parse),
                    &install_mirrors,
                    &network_settings,
                    python_preference,
                    python_downloads,
                    no_config,
                    active,
                    cache,
                    DryRun::Disabled,
                    printer,
                )
                .await?
                .into_environment()?;

                AddTarget::Project(project, Box::new(PythonTarget::Environment(environment)))
            }
        }
        RemoveTarget::Script(script) => {
            let interpreter = ScriptInterpreter::discover(
                Pep723ItemRef::Script(&script),
                python.as_deref().map(PythonRequest::parse),
                &network_settings,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_config,
                active,
                cache,
                printer,
            )
            .await?
            .into_interpreter();

            AddTarget::Script(script, Box::new(interpreter))
        }
    };

    // Determine the lock mode.
    let mode = if locked {
        LockMode::Locked(target.interpreter())
    } else {
        LockMode::Write(target.interpreter())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::LockOperation::new(
        mode,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute((&target).into())
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    };

    let AddTarget::Project(project, environment) = target else {
        // If we're not adding to a project, exit early.
        return Ok(ExitStatus::Success);
    };

    let PythonTarget::Environment(venv) = &*environment else {
        // If we're not syncing, exit early.
        return Ok(ExitStatus::Success);
    };

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

    let state = state.fork();

    match project::sync::do_sync(
        target,
        venv,
        &extras,
        &DependencyGroups::default().with_defaults(defaults),
        EditableMode::Editable,
        install_options,
        Modifications::Exact,
        (&settings).into(),
        &network_settings,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        DryRun::Disabled,
        printer,
        preview,
    )
    .await
    {
        Ok(()) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    }

    Ok(ExitStatus::Success)
}

/// Represents the destination where dependencies are added, either to a project or a script.
#[derive(Debug)]
enum RemoveTarget {
    /// A PEP 723 script, with inline metadata.
    Project(VirtualProject),
    /// A project with a `pyproject.toml`.
    Script(Pep723Script),
}

impl RemoveTarget {
    /// Write the updated content to the target.
    ///
    /// Returns `true` if the content was modified.
    fn write(&self, content: &str) -> Result<bool, io::Error> {
        match self {
            Self::Script(script) => {
                if content == script.metadata.raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    script.write(content)?;
                    Ok(true)
                }
            }
            Self::Project(project) => {
                if content == project.pyproject_toml().raw {
                    debug!("No changes to dependencies; skipping update");
                    Ok(false)
                } else {
                    let pyproject_path = project.root().join("pyproject.toml");
                    fs_err::write(pyproject_path, content)?;
                    Ok(true)
                }
            }
        }
    }

    /// Update the target in-memory to incorporate the new content.
    #[allow(clippy::result_large_err)]
    fn update(self, content: &str) -> Result<Self, ProjectError> {
        match self {
            Self::Script(mut script) => {
                script.metadata = Pep723Metadata::from_str(content)
                    .map_err(ProjectError::Pep723ScriptTomlParse)?;
                Ok(Self::Script(script))
            }
            Self::Project(project) => {
                let project = project
                    .with_pyproject_toml(
                        toml::from_str(content).map_err(ProjectError::PyprojectTomlParse)?,
                    )
                    .ok_or(ProjectError::PyprojectTomlUpdate)?;
                Ok(Self::Project(project))
            }
        }
    }
}

/// Show a hint if a dependency with the given name is present as any dependency type.
///
/// This is useful when a dependency of the user-specified type was not found, but it may be present
/// elsewhere.
fn show_other_dependency_type_hint(
    printer: Printer,
    name: &PackageName,
    pyproject: &PyProjectTomlMut,
) -> Result<()> {
    // TODO(zanieb): Attach these hints to the error so they render _after_ in accordance our
    // typical styling
    for dep_ty in pyproject.find_dependency(name, None) {
        match dep_ty {
            DependencyType::Production => writeln!(
                printer.stderr(),
                "{}{} `{name}` is a production dependency",
                "hint".bold().cyan(),
                ":".bold(),
            )?,
            DependencyType::Dev => writeln!(
                printer.stderr(),
                "{}{} `{name}` is a development dependency (try: `{}`)",
                "hint".bold().cyan(),
                ":".bold(),
                format!("uv remove {name} --dev`").bold()
            )?,
            DependencyType::Optional(group) => writeln!(
                printer.stderr(),
                "{}{} `{name}` is an optional dependency (try: `{}`)",
                "hint".bold().cyan(),
                ":".bold(),
                format!("uv remove {name} --optional {group}").bold()
            )?,
            DependencyType::Group(group) => writeln!(
                printer.stderr(),
                "{}{} `{name}` is in the `{group}` group (try: `{}`)",
                "hint".bold().cyan(),
                ":".bold(),
                format!("uv remove {name} --group {group}").bold()
            )?,
        }
    }

    Ok(())
}
