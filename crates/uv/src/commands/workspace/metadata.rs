use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::Metadata;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectEnvironment, ProjectError, ProjectInterpreter, UniversalState, WorkspacePython,
};
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

use super::module_owners::collect_module_owners;

/// Display metadata about the workspace.
pub(crate) async fn metadata(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    dry_run: DryRun,
    refresh: Refresh,
    sync: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
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

    let virtual_project =
        VirtualProject::discover(project_dir, &DiscoveryOptions::default(), workspace_cache)
            .await?;
    let target = LockTarget::Workspace(virtual_project.workspace());

    // Don't enable any groups' requires-python for interpreter discovery.
    let groups = DependencyGroupsWithDefaults::none();
    let workspace_python = WorkspacePython::from_request(
        python.as_deref().map(PythonRequest::parse),
        Some(virtual_project.workspace()),
        &groups,
        project_dir,
        no_config,
    )
    .await?;
    let interpreter = ProjectInterpreter::discover(
        virtual_project.workspace(),
        &groups,
        workspace_python,
        &client_builder,
        python_preference,
        python_downloads,
        &install_mirrors,
        false,
        Some(false),
        cache,
        printer,
        preview,
    )
    .await?
    .into_interpreter();

    // Determine the lock mode.
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else {
        if let LockCheck::Enabled(lock_check) = lock_check {
            LockMode::Locked(&interpreter, lock_check)
        } else if dry_run.enabled() {
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
            let mut export = Metadata::from_lock(virtual_project.workspace(), &lock)?;
            if sync {
                let environment = ProjectEnvironment::get_or_init(
                    virtual_project.workspace(),
                    &groups,
                    python.as_deref().map(PythonRequest::parse),
                    &install_mirrors,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    false,
                    no_config,
                    Some(false),
                    cache,
                    DryRun::Disabled,
                    printer,
                    preview,
                )
                .await?;
                let module_owners = collect_module_owners(
                    virtual_project.workspace(),
                    &lock,
                    &environment,
                    &settings,
                    &client_builder,
                    &state,
                    &concurrency,
                    cache,
                    workspace_cache,
                    preview,
                )
                .await
                .context("Failed to collect module owners")?;
                export = export.with_module_owners(module_owners);
            }

            print_metadata(&export, printer)
        }
        Err(err @ ProjectError::LockMismatch(..)) => {
            writeln!(printer.stderr(), "{}", err.to_string().bold())?;
            Ok(ExitStatus::Failure)
        }
        Err(ProjectError::Operation(err)) => {
            diagnostics::OperationDiagnostic::with_system_certs(client_builder.system_certs())
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => Err(err.into()),
    }
}

fn print_metadata(export: &Metadata, printer: Printer) -> Result<ExitStatus> {
    writeln!(printer.stdout(), "{}", export.to_json()?)?;

    Ok(ExitStatus::Success)
}
