use std::collections::BTreeSet;
use std::fmt::Write;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::{debug, warn};

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_normalize::{DEV_DEPENDENCIES, DefaultExtras, DefaultGroups};
use uv_preview::Preview;
use uv_python::{ConfigDiscovery, PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::{DirectDependencyKind, ReachabilityRoots, reachable_package_names};
use uv_scripts::{Pep723Metadata, Pep723Script};
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{DependencyType, PyProjectToml};
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::{Modifications, RemovalRoot};
use crate::commands::project::add::{AddTarget, PythonTarget};
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockResult};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    LinkErrorReporting, ProjectEnvironment, ProjectError, ProjectInterpreter, ScriptInterpreter,
    UniversalState, WorkspacePython, default_dependency_groups,
};
use crate::commands::{ExitStatus, diagnostics, project};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverInstallerSettings};

/// Remove one or more packages from the project requirements.
pub(crate) async fn remove(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    active: Option<bool>,
    no_sync: bool,
    packages: Vec<PackageName>,
    dependency_type: DependencyType,
    package: Option<PackageName>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
    malware_settings: MalwareCheckSettings,
) -> Result<ExitStatus> {
    let target = if let Some(script) = script {
        // If we found a PEP 723 script and the user provided a project-only setting, warn.
        if package.is_some() {
            warn_user_once!(
                "`--package` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }
        if let LockCheck::Enabled(lock_check) = lock_check {
            warn_user_once!(
                "`{lock_check}` is a no-op for Python scripts with inline metadata, which always run in isolation",
            );
        }
        if frozen.is_some() {
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
            VirtualProject::discover_with_package(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                &WorkspaceCache::default(),
                package.clone(),
            )
            .await?
        } else {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
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

    let mut removed_roots = Vec::new();
    for package in packages {
        match dependency_type {
            DependencyType::Production => {
                let deps = toml.remove_dependency(&package)?;
                if deps.is_empty() {
                    return Err(DependencyNotFoundError {
                        package: package.clone(),
                        dependency_type: dependency_type.clone(),
                        found_in: toml.find_dependency(&package, None),
                    }
                    .into());
                }
                removed_roots.extend(deps.into_iter().map(|requirement| RemovalRoot {
                    name: requirement.name,
                    extras: requirement.extras,
                    marker: requirement.marker,
                    marker_extras: Box::default(),
                }));
            }
            DependencyType::Dev => {
                let dev_deps = toml.remove_dev_dependency(&package)?;
                let group_deps =
                    toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                if dev_deps.is_empty() && group_deps.is_empty() {
                    return Err(DependencyNotFoundError {
                        package: package.clone(),
                        dependency_type: dependency_type.clone(),
                        found_in: toml.find_dependency(&package, None),
                    }
                    .into());
                }
                removed_roots.extend(dev_deps.into_iter().chain(group_deps).map(|requirement| {
                    RemovalRoot {
                        name: requirement.name,
                        extras: requirement.extras,
                        marker: requirement.marker,
                        marker_extras: Box::default(),
                    }
                }));
            }
            DependencyType::Optional(ref extra) => {
                let deps = toml.remove_optional_dependency(&package, extra)?;
                if deps.is_empty() {
                    return Err(DependencyNotFoundError {
                        package: package.clone(),
                        dependency_type: dependency_type.clone(),
                        found_in: toml.find_dependency(&package, None),
                    }
                    .into());
                }
                removed_roots.extend(deps.into_iter().map(|requirement| RemovalRoot {
                    name: requirement.name,
                    extras: requirement.extras,
                    marker: requirement.marker,
                    marker_extras: vec![extra.clone()].into_boxed_slice(),
                }));
            }
            DependencyType::Group(ref group) => {
                if group == &*DEV_DEPENDENCIES {
                    let dev_deps = toml.remove_dev_dependency(&package)?;
                    let group_deps =
                        toml.remove_dependency_group_requirement(&package, &DEV_DEPENDENCIES)?;
                    if dev_deps.is_empty() && group_deps.is_empty() {
                        return Err(DependencyNotFoundError {
                            package: package.clone(),
                            dependency_type: dependency_type.clone(),
                            found_in: toml.find_dependency(&package, None),
                        }
                        .into());
                    }
                    removed_roots.extend(dev_deps.into_iter().chain(group_deps).map(
                        |requirement| RemovalRoot {
                            name: requirement.name,
                            extras: requirement.extras,
                            marker: requirement.marker,
                            marker_extras: Box::default(),
                        },
                    ));
                } else {
                    let deps = toml.remove_dependency_group_requirement(&package, group)?;
                    if deps.is_empty() {
                        return Err(DependencyNotFoundError {
                            package: package.clone(),
                            dependency_type: dependency_type.clone(),
                            found_in: toml.find_dependency(&package, None),
                        }
                        .into());
                    }
                    removed_roots.extend(deps.into_iter().map(|requirement| RemovalRoot {
                        name: requirement.name,
                        extras: requirement.extras,
                        marker: requirement.marker,
                        marker_extras: Box::default(),
                    }));
                }
            }
        }
    }

    let content = toml.to_string();

    // `--frozen` edits only the declaration and must not read or validate the lockfile.
    if frozen.is_some() {
        target.write(&content)?;
        return Ok(ExitStatus::Success);
    }

    // Preserve both edited files so failures and interrupts can restore a retryable state.
    let snapshot = target.snapshot().await?;
    let mut rollback = RemoveRollback::new(snapshot.clone());

    // Save the modified `pyproject.toml` or script.
    target.write(&content)?;

    // If we're modifying a script, and lockfile doesn't exist, don't create it.
    if let RemoveTarget::Script(ref script) = target {
        if !LockTarget::from(script).lock_path().is_file() {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            rollback.commit();
            return Ok(ExitStatus::Success);
        }
    }

    // Revert both the declaration and lockfile when interrupted after the edit is written.
    let rollback_armed = Arc::clone(&rollback.armed);
    let _ = ctrlc::set_handler(move || {
        if rollback_armed.swap(false, Ordering::SeqCst) {
            let _ = snapshot.revert();
        }

        #[expect(clippy::exit, clippy::cast_possible_wrap)]
        std::process::exit(if cfg!(windows) {
            0xC000_013A_u32 as i32
        } else {
            130
        });
    });

    // Update the `pypackage.toml` in-memory.
    let target = target.update(&content, &WorkspaceCache::default())?;

    // Determine enabled groups and extras
    let default_groups = match &target {
        RemoveTarget::Project(project) => default_dependency_groups(project.pyproject_toml())?,
        RemoveTarget::Script(_) => DefaultGroups::default(),
    };
    let groups = DependencyGroups::default().with_defaults(default_groups);
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());

    // Convert to an `AddTarget` by attaching the appropriate interpreter or environment.
    let target = match target {
        RemoveTarget::Project(project) => {
            if no_sync {
                // Discover the interpreter.
                let workspace_python = WorkspacePython::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(project.workspace()),
                    &groups,
                    project_dir,
                    config_discovery,
                )
                .await?;
                let interpreter = ProjectInterpreter::discover(
                    project.workspace(),
                    &groups,
                    workspace_python,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    &install_mirrors,
                    false,
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
                    &groups,
                    python.as_deref().map(PythonRequest::parse),
                    &install_mirrors,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    no_sync,
                    config_discovery,
                    active,
                    cache,
                    DryRun::Disabled,
                    LinkErrorReporting::User,
                    printer,
                )
                .await?
                .into_environment()?;

                AddTarget::Project(project, Box::new(PythonTarget::Environment(environment)))
            }
        }
        RemoveTarget::Script(script) => {
            let interpreter = ScriptInterpreter::discover(
                (&script).into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_sync,
                config_discovery,
                active,
                cache,
                printer,
            )
            .await?
            .into_interpreter();

            AddTarget::Script(script, Box::new(interpreter))
        }
    };

    let _lock = target
        .acquire_lock()
        .await
        .inspect_err(|err| {
            warn!("Failed to acquire environment lock: {err}");
        })
        .ok();

    // Determine the lock mode.
    let mode = if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(target.interpreter(), lock_check)
    } else {
        LockMode::Write(target.interpreter())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock and sync the environment, if necessary.
    let lock_result = match Box::pin(
        project::lock::LockOperation::new(
            mode,
            &settings.resolver,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            cache,
            &WorkspaceCache::default(),
            printer,
            preview,
        )
        .execute((&target).into()),
    )
    .await
    {
        Ok(result) => result,
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    let AddTarget::Project(project, environment) = target else {
        // If we're not adding to a project, exit early.
        rollback.commit();
        return Ok(ExitStatus::Success);
    };

    let PythonTarget::Environment(venv) = &*environment else {
        // If we're not syncing, exit early.
        rollback.commit();
        return Ok(ExitStatus::Success);
    };

    let direct_kind = match &dependency_type {
        DependencyType::Production => DirectDependencyKind::Production,
        DependencyType::Dev => DirectDependencyKind::Group(&DEV_DEPENDENCIES),
        DependencyType::Optional(extra) => DirectDependencyKind::Optional(extra),
        DependencyType::Group(group) => DirectDependencyKind::Group(group),
    };
    let marker_environment = venv.interpreter().to_resolver_marker_environment();
    let removed_names = removed_roots
        .iter()
        .filter(|root| {
            root.marker
                .evaluate(&marker_environment, &root.marker_extras)
        })
        .map(|root| root.name.clone())
        .collect::<BTreeSet<_>>();

    // Retain every package reachable from the edited project's declarations for this interpreter.
    // Conflict selections are conservatively unioned so every compatible extra/group combination
    // remains owned.
    let current_target = match &project {
        VirtualProject::Project(project) => InstallTarget::Project {
            workspace: project.workspace(),
            name: project.project_name(),
            lock: lock_result.lock(),
        },
        VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
            workspace,
            lock: lock_result.lock(),
        },
    };
    let retained = reachable_package_names(
        &current_target,
        &marker_environment,
        ReachabilityRoots::AllDeclared {
            include_manifest: true,
        },
    )?;

    // Scope lock-derived candidates to the exact removed declaration edges. Installed metadata is
    // also walked during planning, both to reflect the actual environment and to support missing
    // or unreadable previous lockfiles.
    let candidates = if let LockResult::Changed(Some(previous), _) = &lock_result {
        let previous_target = match &project {
            VirtualProject::Project(project) => InstallTarget::Project {
                workspace: project.workspace(),
                name: project.project_name(),
                lock: previous,
            },
            VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
                workspace,
                lock: previous,
            },
        };
        match reachable_package_names(
            &previous_target,
            &marker_environment,
            ReachabilityRoots::Direct {
                project: match &project {
                    VirtualProject::Project(project) => Some(project.project_name()),
                    VirtualProject::NonProject(_) => None,
                },
                kind: direct_kind,
                names: &removed_names,
            },
        ) {
            Ok(candidates) => candidates,
            Err(error) => {
                debug!(
                    %error,
                    "Ignoring an inapplicable previous lockfile while computing removal candidates"
                );
                BTreeSet::new()
            }
        }
    } else {
        BTreeSet::new()
    };
    let modifications = Modifications::Prune {
        roots: removed_roots,
        candidates,
        retained,
    };

    let lock = lock_result.into_lock();
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
        &groups,
        None,
        InstallOptions::default(),
        modifications,
        None,
        (&settings).into(),
        &client_builder,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        &concurrency,
        cache,
        &WorkspaceCache::default(),
        DryRun::Disabled,
        printer,
        preview,
        &malware_settings,
    )
    .await
    {
        Ok(_) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    }

    rollback.commit();
    Ok(ExitStatus::Success)
}

/// Represents the destination where dependencies are added, either to a project or a script.
#[derive(Debug)]
#[expect(clippy::large_enum_variant)]
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
    fn update(self, content: &str, workspace_cache: &WorkspaceCache) -> Result<Self, ProjectError> {
        match self {
            Self::Script(mut script) => {
                script.metadata = Pep723Metadata::from_str(content)
                    .map_err(ProjectError::Pep723ScriptTomlParse)?;
                Ok(Self::Script(script))
            }
            Self::Project(project) => {
                let pyproject_path = project.root().join("pyproject.toml");
                let project = project
                    .update_member(
                        PyProjectToml::from_string(content.to_string(), &pyproject_path)
                            .map_err(ProjectError::PyprojectTomlParse)?,
                        workspace_cache,
                    )?
                    .ok_or(ProjectError::PyprojectTomlUpdate)?;
                Ok(Self::Project(project))
            }
        }
    }

    /// Take a detached snapshot of the declaration and lockfile for rollback.
    async fn snapshot(&self) -> Result<RemoveTargetSnapshot, io::Error> {
        let target = match self {
            Self::Script(script) => LockTarget::from(script),
            Self::Project(project) => LockTarget::Workspace(project.workspace()),
        };
        let lock = target.read_bytes().await?;

        match self {
            Self::Script(script) => Ok(RemoveTargetSnapshot::Script(script.clone(), lock)),
            Self::Project(project) => {
                Ok(RemoveTargetSnapshot::Project(project.clone_detach(), lock))
            }
        }
    }
}

#[derive(Debug, Clone)]
#[expect(clippy::large_enum_variant)]
enum RemoveTargetSnapshot {
    Script(Pep723Script, Option<Vec<u8>>),
    Project(VirtualProject, Option<Vec<u8>>),
}

impl RemoveTargetSnapshot {
    /// Restore the declaration and lockfile captured before removal.
    fn revert(&self) -> Result<(), io::Error> {
        match self {
            Self::Script(script, lock) => {
                debug!("Reverting changes to PEP 723 script block");
                script.write(&script.metadata.raw)?;
                Self::revert_lock(LockTarget::from(script), lock.as_deref())
            }
            Self::Project(project, lock) => {
                let workspace = project.workspace();
                debug!("Reverting changes to `pyproject.toml`");
                fs_err::write(
                    project.root().join("pyproject.toml"),
                    project.pyproject_toml().as_ref(),
                )?;
                Self::revert_lock(LockTarget::from(workspace), lock.as_deref())
            }
        }
    }

    fn revert_lock(target: LockTarget<'_>, lock: Option<&[u8]>) -> Result<(), io::Error> {
        if let Some(lock) = lock {
            debug!("Reverting changes to `uv.lock`");
            fs_err::write(target.lock_path(), lock)
        } else {
            debug!("Removing `uv.lock`");
            match fs_err::remove_file(target.lock_path()) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }
}

struct RemoveRollback {
    snapshot: RemoveTargetSnapshot,
    armed: Arc<AtomicBool>,
}

impl RemoveRollback {
    fn new(snapshot: RemoveTargetSnapshot) -> Self {
        Self {
            snapshot,
            armed: Arc::new(AtomicBool::new(true)),
        }
    }

    fn commit(&mut self) {
        self.armed.store(false, Ordering::SeqCst);
    }
}

impl Drop for RemoveRollback {
    fn drop(&mut self) {
        if self.armed.swap(false, Ordering::SeqCst)
            && let Err(error) = self.snapshot.revert()
        {
            warn!(%error, "Failed to revert changes after removal failed");
        }
    }
}

/// A dependency was not found in the expected dependency type, but may exist elsewhere.
#[derive(Debug, thiserror::Error)]
#[error("The dependency `{package}` could not be found in {}", dependency_type.toml_table_name())]
pub(crate) struct DependencyNotFoundError {
    package: PackageName,
    dependency_type: DependencyType,
    /// Other dependency types where this package was found.
    found_in: Vec<DependencyType>,
}

impl uv_errors::Hint for DependencyNotFoundError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        self.found_in
            .iter()
            .map(|dep_ty| match dep_ty {
                DependencyType::Production => {
                    format!("`{}` is a production dependency", self.package)
                }
                DependencyType::Dev => {
                    format!(
                        "`{}` is a development dependency (try: `{}`)",
                        self.package,
                        format!("uv remove {} --dev", self.package).bold(),
                    )
                }
                DependencyType::Optional(group) => {
                    format!(
                        "`{}` is an optional dependency (try: `{}`)",
                        self.package,
                        format!("uv remove {} --optional {group}", self.package).bold(),
                    )
                }
                DependencyType::Group(group) => {
                    format!(
                        "`{}` is in the `{group}` group (try: `{}`)",
                        self.package,
                        format!("uv remove {} --group {group}", self.package).bold(),
                    )
                }
            })
            .collect()
    }
}
