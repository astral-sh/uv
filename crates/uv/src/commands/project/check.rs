use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, DryRun, ExtrasSpecification,
    InstallOptions,
};
use uv_normalize::DefaultExtras;
use uv_preview::{Preview, PreviewFeature};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceErrorKind};

use crate::commands::pip::loggers::{SummaryInstallLogger, SummaryResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectEnvironment, ProjectError, UniversalState, WorkspacePython, default_dependency_groups,
    validate_project_requires_python,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, diagnostics, project};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverInstallerSettings};

mod ty;

/// Run project checks.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn check(
    project_dir: &Path,
    ty_path: Option<PathBuf>,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    no_sync: bool,
    isolated: bool,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    ty_version: Option<String>,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
    no_project: bool,
    no_config: bool,
    malware_settings: MalwareCheckSettings,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::Check) {
        warn_user!(
            "`uv check` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Check
        );
    }

    // Discover the project.
    let project = if no_project {
        None
    } else {
        match VirtualProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            cache,
            workspace_cache,
        )
        .await
        {
            Ok(project) => Some(project),
            Err(err) => {
                if matches!(
                    err.as_ref(),
                    WorkspaceErrorKind::MissingPyprojectToml
                        | WorkspaceErrorKind::MissingProject(_)
                        | WorkspaceErrorKind::NonWorkspace(_),
                ) {
                    None
                } else {
                    return Err(err.into());
                }
            }
        }
    };

    if no_project {
        for flag in extras.history().as_flags_pretty() {
            warn_user!("`{flag}` has no effect when used alongside `--no-project`");
        }
        for flag in groups.history().as_flags_pretty() {
            warn_user!("`{flag}` has no effect when used alongside `--no-project`");
        }
        if let LockCheck::Enabled(lock_check) = lock_check {
            warn_user!("`{lock_check}` has no effect when used alongside `--no-project`");
        }
        if frozen.is_some() {
            warn_user!("`--frozen` has no effect when used alongside `--no-project`");
        }
        if no_sync {
            warn_user!("`--no-sync` has no effect when used alongside `--no-project`");
        }
    } else if project.is_none() {
        for flag in extras.history().as_flags_pretty() {
            warn_user!("`{flag}` has no effect when used outside of a project");
        }
        for flag in groups.history().as_flags_pretty() {
            warn_user!("`{flag}` has no effect when used outside of a project");
        }
        if let LockCheck::Enabled(lock_check) = lock_check {
            warn_user!("`{lock_check}` has no effect when used outside of a project");
        }
        if frozen.is_some() {
            warn_user!("`--frozen` has no effect when used outside of a project");
        }
        if no_sync {
            warn_user!("`--no-sync` has no effect when used outside of a project");
        }
    }

    let target_dir = project
        .as_ref()
        .map(|p| p.root().to_owned())
        .unwrap_or_else(|| project_dir.to_owned());

    let groups = if let Some(project) = &project {
        groups.with_defaults(default_dependency_groups(project.pyproject_toml())?)
    } else {
        DependencyGroupsWithDefaults::none()
    };

    // Create an isolated environment, if requested.
    let temp_dir;
    let isolated_venv = if isolated {
        debug!("Creating isolated virtual environment");

        let workspace = project.as_ref().map(VirtualProject::workspace);
        let WorkspacePython {
            source,
            python_request,
            requires_python,
        } = WorkspacePython::from_request(
            python.as_deref().map(PythonRequest::parse),
            workspace,
            &groups,
            project_dir,
            no_config,
        )
        .await?;

        let reporter = PythonDownloadReporter::single(printer);
        let interpreter = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::Any,
            python_preference,
            python_downloads,
            &client_builder,
            cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await?
        .into_interpreter();

        if let Some(requires_python) = requires_python.as_ref() {
            validate_project_requires_python(
                &interpreter,
                workspace,
                &groups,
                requires_python,
                &source,
            )?;
        }

        temp_dir = cache.venv_dir()?;
        Some(uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::TemporaryEnvironment),
            false,
            false,
            false,
        )?)
    } else {
        None
    };

    // Select an environment and, if we found a project, sync it before running checks.
    let mut workspace_metadata = None;
    let mut locked_ty_version = None;
    let venv_path = if let Some(project) = &project {
        let extras = extras.with_defaults(DefaultExtras::default());

        let venv = if let Some(venv) = isolated_venv {
            venv
        } else {
            ProjectEnvironment::get_or_init(
                project.workspace(),
                &groups,
                python.as_deref().map(PythonRequest::parse),
                &install_mirrors,
                &client_builder,
                python_preference,
                python_downloads,
                no_sync,
                no_config,
                None,
                cache,
                DryRun::Disabled,
                printer,
            )
            .await?
            .into_environment()?
        };

        let ty_declaration = if ty_path.is_none() && ty_version.is_none() {
            ty::active_declaration(project, venv.interpreter(), &settings.resolver.sources)?
        } else {
            None
        };

        let state = UniversalState::default();
        let _environment_lock;
        let lock = if no_sync {
            debug!("Skipping environment synchronization due to `--no-sync`");

            let lock = match LockTarget::Workspace(project.workspace()).read().await {
                Ok(lock) => lock,
                Err(err) if ty_declaration.is_some() => return Err(err.into()),
                Err(err) => {
                    debug!("Failed to read lockfile; skipping workspace metadata: {err}");
                    None
                }
            };
            if let Some(declaration) = ty_declaration.as_ref() {
                let Some(lock) = lock.as_ref() else {
                    anyhow::bail!(
                        "The active `ty` development dependency requires an existing lockfile when `--no-sync` is used; update `uv.lock`, remove `--no-sync`, or use `--ty-version` or the `TY` environment variable"
                    );
                };
                locked_ty_version = Some(ty::version_from_lock(declaration, project, lock, &venv)?);
            }
            lock
        } else {
            // Keep the environment locked through synchronization and metadata collection.
            _environment_lock = venv
                .lock()
                .await
                .inspect_err(|err| {
                    tracing::warn!("Failed to acquire environment lock: {err}");
                })
                .ok();

            let sync_state = state.fork();

            let mode = if let Some(frozen_source) = frozen {
                LockMode::Frozen(frozen_source.into())
            } else if let LockCheck::Enabled(lock_check) = lock_check {
                LockMode::Locked(venv.interpreter(), lock_check)
            } else if isolated {
                LockMode::DryRun(venv.interpreter())
            } else {
                LockMode::Write(venv.interpreter())
            };

            let result = match Box::pin(
                project::lock::LockOperation::new(
                    mode,
                    &settings.resolver,
                    &client_builder,
                    &state,
                    Box::new(SummaryResolveLogger),
                    &concurrency,
                    cache,
                    workspace_cache,
                    printer,
                    preview,
                )
                .execute(project.workspace().into()),
            )
            .await
            {
                Ok(result) => result,
                Err(ProjectError::Operation(err)) => {
                    return diagnostics::OperationDiagnostic::with_system_certs(
                        client_builder.system_certs(),
                    )
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                }
                Err(err) => return Err(err.into()),
            };

            if let Some(declaration) = ty_declaration.as_ref() {
                locked_ty_version = Some(ty::version_from_lock(
                    declaration,
                    project,
                    result.lock(),
                    &venv,
                )?);
            }

            let target = match project {
                VirtualProject::Project(project) => InstallTarget::Project {
                    workspace: project.workspace(),
                    name: project.project_name(),
                    lock: result.lock(),
                },
                VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
                    workspace,
                    lock: result.lock(),
                },
            };

            target.validate_extras(&extras)?;
            target.validate_groups(&groups)?;

            match project::sync::do_sync(
                target,
                &venv,
                &extras,
                &groups,
                None,
                InstallOptions::default(),
                Modifications::Sufficient,
                None,
                (&settings).into(),
                &client_builder,
                &sync_state,
                Box::new(SummaryInstallLogger),
                installer_metadata,
                &concurrency,
                cache,
                workspace_cache,
                DryRun::Disabled,
                printer,
                preview,
                &malware_settings,
            )
            .await
            {
                Ok(_) => {}
                Err(ProjectError::Operation(err)) => {
                    return diagnostics::OperationDiagnostic::with_system_certs(
                        client_builder.system_certs(),
                    )
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                }
                Err(err) => return Err(err.into()),
            }

            Some(result.into_lock())
        };

        if let Some(lock) = lock {
            let target = match project {
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
            let metadata = crate::commands::workspace::metadata::metadata_from_target(
                project.workspace(),
                (!no_sync).then_some(&venv),
                target,
                &extras,
                &groups,
                &settings.resolver,
            )?;
            let mut metadata = metadata.to_json()?;
            metadata.push('\n');
            workspace_metadata = Some(metadata);
        }

        Some(venv.root().to_owned())
    } else {
        isolated_venv.map(|venv| venv.root().to_owned())
    };

    let exclude_newer = settings
        .resolver
        .exclude_newer
        .global
        .map(|value| value.timestamp());

    ty::run(
        ty_version.or(locked_ty_version),
        ty_path,
        &target_dir,
        venv_path.as_deref(),
        workspace_metadata,
        exclude_newer,
        &client_builder,
        cache,
        printer,
    )
    .await
}
