use std::fmt::Write;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use owo_colors::OwoColorize;
use tokio::process::Command;

use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::Metadata;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::child::run_to_completion;
use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectEnvironment, ProjectError, ProjectInterpreter, UniversalState, WorkspacePython,
};
use crate::commands::workspace::module_owners::collect_module_owners;
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

/// Run dependency linting via `ty check`.
pub(crate) async fn mow(
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    refresh: Refresh,
    settings: ResolverSettings,
    package: Option<PackageName>,
    ty: Option<PathBuf>,
    extra_args: Vec<String>,
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
    if !preview.is_enabled(PreviewFeature::Mow) {
        warn_user!(
            "`uv mow` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Mow
        );
    }

    let virtual_project = if let Some(package) = package {
        VirtualProject::discover_with_package(
            project_dir,
            &DiscoveryOptions::default(),
            workspace_cache,
            package,
        )
        .await?
    } else {
        VirtualProject::discover(project_dir, &DiscoveryOptions::default(), workspace_cache).await?
    };
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

    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(&interpreter, lock_check)
    } else {
        LockMode::Write(&interpreter)
    };

    let state = UniversalState::default();

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
        Ok(result) => {
            let lock = result.into_lock();
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
            let export = Metadata::from_lock(virtual_project.workspace(), &lock)?
                .with_module_owners(module_owners);

            let temp_dir = tempfile::tempdir()
                .context("Failed to create temporary directory for dependency metadata")?;
            let dependency_metadata_path = temp_dir.path().join("dependency-metadata.json");
            fs_err::write(&dependency_metadata_path, export.to_json()?)
                .context("Failed to write dependency metadata")?;

            let target_project = virtual_project
                .project_name()
                .map(|_| virtual_project.root().to_path_buf());

            let ty_path = ty.as_deref().unwrap_or_else(|| Path::new("ty"));

            let mut command = Command::new(ty_path);
            command.arg("check");
            command.arg("--python").arg(environment.root());
            command
                .arg("--dependency-metadata")
                .arg(&dependency_metadata_path);
            command.args(extra_args.iter());
            if let Some(target_project) = target_project {
                command.arg(target_project);
            }

            let handle = match command.spawn() {
                Ok(handle) => handle,
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    if let Some(ty) = ty.as_deref() {
                        return Err(anyhow!(
                            "Failed to find `ty` executable at `{}`. `uv mow` requires a compatible `ty` executable with `--dependency-metadata` support.",
                            ty.user_display(),
                        ));
                    }
                    return Err(anyhow!(
                        "Failed to find `ty` on PATH. `uv mow` requires a compatible `ty` executable with `--dependency-metadata` support."
                    ));
                }
                Err(err) => return Err(err).context("Failed to spawn `ty check`"),
            };

            run_to_completion(handle).await
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
