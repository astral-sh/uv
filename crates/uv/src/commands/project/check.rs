use std::path::Path;

use anyhow::Result;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions,
};
use uv_normalize::DefaultExtras;
use uv_preview::{Preview, PreviewFeature};
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::commands::pip::loggers::{SummaryInstallLogger, SummaryResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::{
    ProjectEnvironment, ProjectError, UniversalState, default_dependency_groups,
};
use crate::commands::{ExitStatus, diagnostics, project};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverInstallerSettings};

mod ty;

/// Run project checks.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn check(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    no_sync: bool,
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
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), workspace_cache)
            .await
        {
            Ok(project) => Some(project),
            Err(
                WorkspaceError::MissingPyprojectToml
                | WorkspaceError::MissingProject(_)
                | WorkspaceError::NonWorkspace(_),
            ) => None,
            Err(err) => return Err(err.into()),
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

    // If we found a project, sync the environment before running checks.
    let venv_path = if let Some(project) = &project {
        let default_groups = default_dependency_groups(project.pyproject_toml())?;
        let default_extras = DefaultExtras::default();
        let groups = groups.with_defaults(default_groups);
        let extras = extras.with_defaults(default_extras);

        let venv = ProjectEnvironment::get_or_init(
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
            preview,
        )
        .await?
        .into_environment()?;

        if no_sync {
            debug!("Skipping environment synchronization due to `--no-sync`");
        } else {
            let _lock = venv
                .lock()
                .await
                .inspect_err(|err| {
                    tracing::warn!("Failed to acquire environment lock: {err}");
                })
                .ok();

            let lock_state = UniversalState::default();
            let sync_state = lock_state.fork();

            let mode = if let Some(frozen_source) = frozen {
                LockMode::Frozen(frozen_source.into())
            } else if let LockCheck::Enabled(lock_check) = lock_check {
                LockMode::Locked(venv.interpreter(), lock_check)
            } else {
                LockMode::Write(venv.interpreter())
            };

            let result = match Box::pin(
                project::lock::LockOperation::new(
                    mode,
                    &settings.resolver,
                    &client_builder,
                    &lock_state,
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
        }

        Some(venv.root().to_owned())
    } else {
        None
    };

    let exclude_newer = settings
        .resolver
        .exclude_newer
        .global
        .map(|value| value.timestamp());

    ty::run(
        ty_version,
        &target_dir,
        venv_path.as_deref(),
        exclude_newer,
        &client_builder,
        cache,
        printer,
    )
    .await
}
