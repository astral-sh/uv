use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroupsWithDefaults, DryRun, ExtrasSpecificationWithDefaults,
};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{
    ConfigDiscovery, PythonDownloads, PythonEnvironment, PythonPreference, PythonRequest,
};
use uv_resolver::Metadata;
use uv_scripts::Pep723Script;
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    LinkErrorReporting, ProjectEnvironment, ProjectError, ProjectInterpreter, ScriptEnvironment,
    ScriptInterpreter, UniversalState, WorkspacePython,
};
use crate::commands::{ExitStatus, UvError, diagnostics};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

use super::module_owners::{collect_module_owners, find_module_owners};

/// Display metadata about the workspace.
pub(crate) async fn metadata(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    dry_run: DryRun,
    refresh: Refresh,
    sync: bool,
    active: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    malware_settings: MalwareCheckSettings,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    script: Option<Pep723Script>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::WorkspaceMetadata) {
        warn_user!(
            "The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::WorkspaceMetadata
        );
    }

    let virtual_project;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        virtual_project = VirtualProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            cache,
            workspace_cache,
        )
        .await?;
        LockTarget::Workspace(virtual_project.workspace())
    };

    // Don't enable any groups' requires-python for interpreter discovery.
    let groups = DependencyGroupsWithDefaults::none();

    // Determine the lock mode.
    let interpreter;
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else {
        interpreter = match target {
            LockTarget::Script(script) => ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                Some(active),
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => {
                let workspace_python = WorkspacePython::from_request(
                    python.as_deref().map(PythonRequest::parse),
                    Some(workspace),
                    &groups,
                    project_dir,
                    config_discovery,
                )
                .await?;
                ProjectInterpreter::discover(
                    workspace,
                    &groups,
                    workspace_python,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    &install_mirrors,
                    false,
                    Some(active),
                    cache,
                    printer,
                )
                .await?
                .into_interpreter()
            }
        };

        if let LockCheck::Enabled(lock_check) = lock_check {
            LockMode::Locked(&interpreter, lock_check)
        } else if dry_run.enabled()
            || (matches!(target, LockTarget::Script(_)) && !target.lock_path().is_file())
        {
            LockMode::DryRun(&interpreter)
        } else {
            LockMode::Write(&interpreter)
        }
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Perform the lock operation.
    match Box::pin(
        LockOperation::new(
            mode,
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            cache,
            workspace_cache,
            printer,
            preview,
        )
        .with_refresh(&refresh)
        .execute(target),
    )
    .await
    {
        Ok(lock) => {
            let lock = lock.into_lock();
            let install_target = match target {
                LockTarget::Workspace(workspace) => InstallTarget::Workspace {
                    workspace,
                    lock: &lock,
                },
                LockTarget::Script(script) => InstallTarget::Script {
                    script,
                    lock: &lock,
                },
            };
            let mut export = metadata_for_target(install_target)?;
            if sync {
                let environment = match target {
                    LockTarget::Workspace(workspace) => ProjectEnvironment::get_or_init(
                        workspace,
                        &groups,
                        python.as_deref().map(PythonRequest::parse),
                        &install_mirrors,
                        &client_builder,
                        python_preference,
                        python_downloads,
                        false,
                        config_discovery,
                        Some(active),
                        cache,
                        DryRun::Disabled,
                        LinkErrorReporting::User,
                        printer,
                    )
                    .await?
                    .into_environment()?,
                    LockTarget::Script(script) => ScriptEnvironment::get_or_init(
                        script.into(),
                        python.as_deref().map(PythonRequest::parse),
                        &client_builder,
                        python_preference,
                        python_downloads,
                        &install_mirrors,
                        false,
                        config_discovery,
                        Some(active),
                        cache,
                        DryRun::Disabled,
                        printer,
                    )
                    .await?
                    .into_environment()?,
                };
                let _lock = environment
                    .lock()
                    .await
                    .inspect_err(|err| {
                        tracing::warn!("Failed to acquire environment lock: {err}");
                    })
                    .ok();
                let module_owners = collect_module_owners(
                    install_target,
                    &environment,
                    &settings,
                    &client_builder,
                    &state,
                    &concurrency,
                    cache,
                    workspace_cache,
                    preview,
                    &malware_settings,
                )
                .await
                .context("Failed to collect module owners")?;
                export = export
                    .with_environment_root(environment.root())
                    .with_module_owners(module_owners);
            }

            print_metadata(&export, printer)
        }
        Err(err @ ProjectError::LockMismatch(..)) => Err(UvError::user(err).into()),
        Err(ProjectError::Operation(err)) => diagnostics::OperationDiagnostic::default()
            .report(err)
            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into())),
        Err(err) => Err(err.into()),
    }
}

/// Build metadata from an existing lock and environment without synchronizing it.
pub(crate) fn metadata_from_target(
    environment: Option<&PythonEnvironment>,
    target: InstallTarget<'_>,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    settings: &ResolverSettings,
) -> Result<Metadata> {
    let mut export = metadata_for_target(target)?;
    if let Some(environment) = environment {
        let module_owners = find_module_owners(target, environment, extras, groups, settings)
            .context("Failed to collect module owners")?;
        export = export
            .with_environment_root(environment.root())
            .with_module_owners(module_owners);
    }

    Ok(export)
}

fn metadata_for_target(target: InstallTarget<'_>) -> Result<Metadata> {
    match target {
        InstallTarget::Project {
            workspace, lock, ..
        }
        | InstallTarget::Projects {
            workspace, lock, ..
        }
        | InstallTarget::Workspace { workspace, lock }
        | InstallTarget::NonProjectWorkspace { workspace, lock } => {
            Ok(Metadata::from_lock(workspace, lock)?)
        }
        InstallTarget::Script { script, lock } => Ok(Metadata::from_script(&script.path, lock)?),
    }
}

fn print_metadata(export: &Metadata, printer: Printer) -> Result<ExitStatus> {
    writeln!(printer.stdout(), "{}", export.to_json()?)?;

    Ok(ExitStatus::Success)
}
