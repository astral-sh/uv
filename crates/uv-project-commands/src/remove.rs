use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::warn;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions,
    Modifications,
};
use uv_dispatch::UniversalState;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::malware::MalwareCheckContext;
use uv_environment_operations::{
    LinkErrorReporting, ProjectEnvironment, ProjectEnvironmentPolicy, ProjectEnvironmentTarget,
    ProjectInterpreter, sync_from_lock,
};
use uv_fs::Simplified;
use uv_install_operations::loggers::DefaultInstallLogger;
use uv_lock_operations::{LockMode, LockOperation, LockTarget};
use uv_normalize::{DEV_DEPENDENCIES, DefaultExtras, DefaultGroups, PackageName};
use uv_preview::Preview;
use uv_project_edit::{DependencyTarget, PyProjectTomlMut};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::ScriptInterpreter;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest};
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_scripts::Pep723Script;
use uv_settings::{
    FrozenSource, LockCheck, MalwareCheckSettings, PythonInstallMirrors, ResolverInstallerSettings,
};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::DependencyType;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::edit::{EditTarget, ProjectEdit, PythonTarget};

/// Remove one or more packages from the project requirements.
pub async fn remove(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    active: ActiveEnvironment,
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
    python_arch: Option<PythonArchitecture>,
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
        EditTarget::Script(script)
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

        EditTarget::Project(project)
    };

    let mut toml = match &target {
        EditTarget::Script(script) => {
            PyProjectTomlMut::from_toml(&script.metadata.raw, DependencyTarget::Script)
        }
        EditTarget::Project(project) => PyProjectTomlMut::from_toml(
            project.pyproject_toml().raw.as_ref(),
            DependencyTarget::PyProjectToml,
        ),
    }?;

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
                }
            }
        }
    }

    let content = toml.to_string();

    let (path, lock_target) = match &target {
        EditTarget::Script(script) => (script.path.clone(), LockTarget::from(script)),
        EditTarget::Project(project) => (
            project.root().join("pyproject.toml"),
            LockTarget::from(project.workspace()),
        ),
    };
    let edit = ProjectEdit::new(
        [path]
            .into_iter()
            .chain(frozen.is_none().then(|| lock_target.lock_path())),
    )?;

    // Save the modified `pyproject.toml` or script.
    target.write(&content)?;

    // If `--frozen`, exit early. There's no reason to lock and sync, since we don't need a `uv.lock`
    // to exist at all.
    if frozen.is_some() {
        edit.commit();
        return Ok(ExitStatus::Success);
    }

    // If we're modifying a script, and lockfile doesn't exist, don't create it.
    if let EditTarget::Script(ref script) = target {
        if !LockTarget::from(script).lock_path().is_file() {
            writeln!(
                printer.stderr(),
                "Updated `{}`",
                script.path.user_display().cyan()
            )?;
            edit.commit();
            return Ok(ExitStatus::Success);
        }
    }

    // Update the `pypackage.toml` in-memory.
    let target = target.update(&content, &WorkspaceCache::default())?;

    // Determine enabled groups and extras
    let default_groups = match &target {
        EditTarget::Project(project) => project.default_groups()?,
        EditTarget::Script(_) => DefaultGroups::default(),
    };
    let groups = DependencyGroups::default().with_defaults(default_groups);
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());

    // Discover the interpreter or environment used to lock and sync the target.
    let python_target = match &target {
        EditTarget::Project(project) => {
            if no_sync {
                // Discover the interpreter.
                let project_python = ProjectPythonRequest::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(project.workspace()),
                    &groups,
                    project_dir,
                    config_discovery,
                )
                .await?;
                let interpreter = ProjectInterpreter::discover(
                    ProjectEnvironmentTarget::from(project.workspace()),
                    project_python,
                    &client_builder,
                    python_preference,
                    python_arch,
                    python_downloads,
                    &install_mirrors,
                    ProjectEnvironmentPolicy::Optional,
                    // Suppress warnings about the active environment when we won't modify it.
                    active.without_warning(),
                    cache,
                    printer,
                )
                .await?
                .into_interpreter();

                PythonTarget::Interpreter(interpreter)
            } else {
                // Discover or create the virtual environment.
                let environment = ProjectEnvironment::get_or_init(
                    ProjectEnvironmentTarget::from(project.workspace()),
                    None,
                    &groups,
                    python.as_deref().map(PythonRequest::parse),
                    &install_mirrors,
                    &client_builder,
                    python_preference,
                    python_arch,
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

                PythonTarget::Environment(environment)
            }
        }
        EditTarget::Script(script) => {
            let interpreter = ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_arch,
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

            PythonTarget::Interpreter(interpreter)
        }
    };

    let _lock = python_target
        .interpreter()
        .lock()
        .await
        .inspect_err(|err| {
            warn!("Failed to acquire environment lock: {err}");
        })
        .ok();

    // Determine the lock mode.
    let mode = if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(python_target.interpreter(), lock_check)
    } else {
        LockMode::Write(python_target.interpreter())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock and sync the environment, if necessary.
    let lock = match Box::pin(
        LockOperation::new(
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
        Ok(result) => result.into_lock(),
        Err(err) => return Err(UvError::from(err).into()),
    };

    let EditTarget::Project(project) = target else {
        // If we're not adding to a project, exit early.
        edit.commit();
        return Ok(ExitStatus::Success);
    };

    let PythonTarget::Environment(venv) = &python_target else {
        // If we're not syncing, exit early.
        edit.commit();
        return Ok(ExitStatus::Success);
    };

    // Identify the installation target.
    let target = InstallTarget::from_project(
        &project,
        &lock,
        PackageSelection::from_args(false, &[], project.project_name()),
    );

    let state = state.fork();

    match sync_from_lock(
        target,
        venv,
        &extras,
        &groups,
        None,
        InstallOptions::default(),
        Modifications::Exact,
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
        MalwareCheckContext::from(&malware_settings),
    )
    .await
    {
        Ok(_) => {}
        Err(err) => return Err(UvError::from(err).into()),
    }

    edit.commit();
    Ok(ExitStatus::Success)
}

/// A dependency was not found in the expected dependency type, but may exist elsewhere.
#[derive(Debug, thiserror::Error)]
#[error("The dependency `{package}` could not be found in {}", dependency_type.toml_table_name())]
pub struct DependencyNotFoundError {
    package: PackageName,
    dependency_type: DependencyType,
    /// Other dependency types where this package was found.
    found_in: Vec<DependencyType>,
}

impl uv_errors::Hinted for DependencyNotFoundError {
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
