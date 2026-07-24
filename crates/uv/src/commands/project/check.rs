use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, DryRun, ExtrasSpecification,
    InstallOptions,
};
use uv_fs::normalize_path;
use uv_normalize::{DEV_DEPENDENCIES, DefaultExtras, PackageName};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_scripts::Pep723Script;
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceErrorKind};

use crate::commands::pip::loggers::{SummaryInstallLogger, SummaryResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::environment::CachedEnvironment;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    LinkErrorReporting, ProjectEnvironment, ProjectError, ProjectInterpreter, ScriptEnvironment,
    ScriptInterpreter, UniversalState, WorkspacePython, default_dependency_groups,
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
    all_packages: bool,
    package: Vec<PackageName>,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    ty_version: Option<String>,
    show_version: bool,
    script: Option<Pep723Script>,
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
    config_discovery: ConfigDiscovery,
    malware_settings: MalwareCheckSettings,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeature::Check) {
        warn_user!(
            "`uv check` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Check
        );
    }

    // Discover the project.
    let project = if no_project || script.is_some() {
        None
    } else {
        let discovery = if let [name] = package.as_slice() {
            VirtualProject::discover_with_package(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                workspace_cache,
                name.clone(),
            )
            .await
        } else {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                workspace_cache,
            )
            .await
        };

        match discovery {
            Ok(project) => {
                for name in &package {
                    if !project.workspace().packages().contains_key(name) {
                        anyhow::bail!("Package `{name}` not found in workspace");
                    }
                }
                Some(project)
            }
            Err(err) => {
                if let WorkspaceErrorKind::NoSuchMember(name, _) = err.as_ref() {
                    anyhow::bail!("Package `{name}` not found in workspace");
                }
                if !all_packages
                    && package.is_empty()
                    && matches!(
                        err.as_ref(),
                        WorkspaceErrorKind::MissingPyprojectToml
                            | WorkspaceErrorKind::MissingProject(_)
                            | WorkspaceErrorKind::NonWorkspace(_),
                    )
                {
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
    } else if project.is_none() && script.is_none() {
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

    let is_virtual_workspace = project
        .as_ref()
        .is_some_and(|project| project.project_name().is_none());
    let defacto_all_packages = all_packages || (is_virtual_workspace && package.is_empty());

    let target_dir = script
        .as_ref()
        .and_then(|script| script.path.parent())
        .map(Path::to_path_buf)
        .or_else(|| {
            project.as_ref().map(|project| {
                // If multiple packages are selected, or the package is outside the workspace dir,
                // require analysis to run in the workspace dir.
                if defacto_all_packages
                    || package.len() > 1
                    || !normalize_path(project.root())
                        .starts_with(project.workspace().install_path())
                {
                    project.workspace().install_path().to_owned()
                } else {
                    project.root().to_owned()
                }
            })
        })
        .unwrap_or_else(|| project_dir.to_owned());

    let check_targets = if let Some(script) = script.as_ref() {
        vec![script.path.clone()]
    } else if let Some(project) = project.as_ref() {
        if defacto_all_packages {
            // In --all-packages mode, and anything equivalent like virtual workspaces,
            // we can't just pass ty the root of the project because:
            //
            // * It excludes members of the workspace that aren't nested under the root,
            //   as constructs like `members = ["../foo"]` are legal.
            // * For virtual workspaces, this can include files that are not strictly
            //   part of any member, such as `scripts/myscript.py`
            //
            // The first issue is definitely important to handle, but the second issue
            // is debatable. It is in fact Useful for ty to find and check all your
            // random scripts, and indeed this is the default ty behaviour. Attempting
            // to manually suppress this behaviour is an attempt to maintain "uv-like"
            // behaviour, but if anyone disagrees we can change this by just always
            // including the workspace root, even for virtual workspaces.
            project
                .workspace()
                .packages()
                .values()
                .map(|member| member.root().clone())
                .collect()
        } else if !package.is_empty() {
            // If the user has specified a list of packages, tell ty to only check those packages.
            package
                .iter()
                .map(|name| {
                    project
                        .workspace()
                        .packages()
                        .get(name)
                        .map(|member| member.root().clone())
                        .ok_or_else(|| anyhow::anyhow!("Package `{name}` not found in workspace"))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            // Otherwise we're checking just this one package (nearest ancestor).
            vec![project.root().to_owned()]
        }
    } else {
        Vec::new()
    };

    // Any selected package can contain other workspace members, even in a virtual workspace.
    // Explicitly exclude any workspace members that aren't selected *and are nested under
    // a selected one*, so ty doesn't emit diagnostics for them (if they're dependencies
    // of selected packages that's fine, ty will still find them for those purposes).
    //
    // The most common case this is handling is a non-virtual workspace, where the root
    // package will almost always have the other packages nested under it, and we need a
    // way to select just the workspace root.
    let excluded_targets = if let Some(project) = project.as_ref()
        && !defacto_all_packages
    {
        project
            .workspace()
            .packages()
            .iter()
            .filter(|(name, member)| {
                let selected = if package.is_empty() {
                    project.project_name() == Some(*name)
                } else {
                    package.contains(name)
                };
                !selected
                    && check_targets
                        .iter()
                        .any(|target| member.root().starts_with(target))
            })
            .map(|(_, member)| member.root().clone())
            .collect()
    } else {
        Vec::new()
    };

    let groups = if let Some(project) = &project {
        groups.with_defaults(default_dependency_groups(project.pyproject_toml())?)
    } else {
        DependencyGroupsWithDefaults::none()
    };

    // Create an isolated environment, if requested.
    let temp_dir;
    let isolated_venv = if isolated {
        debug!("Creating isolated virtual environment");

        let interpreter = if let Some(script) = script.as_ref() {
            ScriptInterpreter::discover(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                Some(false),
                cache,
                printer,
            )
            .await?
            .into_interpreter()
        } else {
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
                config_discovery,
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
            interpreter
        };

        temp_dir = cache.venv_dir()?;
        Some(uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::TemporaryEnvironment),
            false,
            uv_virtualenv::Seed::Disabled,
            false,
        )?)
    } else {
        None
    };

    // Select an environment and, if we found a project, sync it before running checks.
    let mut locked_ty_path = None;
    let venv_path = if let Some(script) = &script {
        let extras = extras.with_defaults(DefaultExtras::default());
        let venv = if let Some(venv) = isolated_venv {
            venv
        } else {
            ScriptEnvironment::get_or_init(
                script.into(),
                python.as_deref().map(PythonRequest::parse),
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                no_sync,
                config_discovery,
                Some(false),
                cache,
                DryRun::Disabled,
                printer,
            )
            .await?
            .into_environment()?
        };

        let state = UniversalState::default();
        let lock_target = LockTarget::Script(script);
        // Scripts always run in an isolated environment, so `--no-sync` has no effect.
        let _environment_lock = venv
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
        } else if isolated || !lock_target.lock_path().is_file() {
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
            .execute(lock_target),
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

        let marker_environment = venv.interpreter().to_resolver_marker_environment();
        if ty_path.is_none()
            && ty_version.is_none()
            && result
                .lock()
                .dependency_selection(
                    None,
                    &PackageName::from_str("ty")?,
                    marker_environment.markers(),
                )
                .map_err(anyhow::Error::msg)?
                .root()
                .is_some()
        {
            locked_ty_path = Some(
                venv.scripts()
                    .join(format!("ty{}", std::env::consts::EXE_SUFFIX)),
            );
        }

        let target = InstallTarget::Script {
            script,
            lock: result.lock(),
        };
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
                return diagnostics::OperationDiagnostic::default()
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }
            Err(err) => return Err(err.into()),
        }

        if no_sync {
            warn_user!(
                "`--no-sync` is a no-op for Python scripts with inline metadata, which always run in isolation"
            );
        }

        Some(venv.root().to_owned())
    } else if let Some(project) = &project {
        let extras = extras.with_defaults(DefaultExtras::default());
        let mut malware_context = project::sync::MalwareCheckContext::from(&malware_settings);

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
                config_discovery,
                None,
                cache,
                DryRun::Disabled,
                LinkErrorReporting::User,
                printer,
            )
            .await?
            .into_environment()?
        };

        // `--no-sync` intentionally permits an incompatible project environment, but locking must
        // still use an interpreter that satisfies the project and any explicit Python request.
        let lock_interpreter = if no_sync && !isolated && frozen.is_none() {
            let workspace_python = WorkspacePython::from_request(
                python.as_deref().map(PythonRequest::parse),
                Some(project.workspace()),
                &groups,
                project_dir,
                config_discovery,
            )
            .await?;
            Some(
                ProjectInterpreter::discover(
                    project.workspace(),
                    &groups,
                    workspace_python,
                    &client_builder,
                    python_preference,
                    python_downloads,
                    &install_mirrors,
                    false,
                    None,
                    cache,
                    printer,
                )
                .await?
                .into_interpreter(),
            )
        } else {
            None
        };
        let lock_interpreter = lock_interpreter
            .as_ref()
            .unwrap_or_else(|| venv.interpreter());

        let state = UniversalState::default();
        // Keep the environment locked through synchronization and metadata collection.
        let _environment_lock;
        if !no_sync {
            _environment_lock = venv
                .lock()
                .await
                .inspect_err(|err| {
                    tracing::warn!("Failed to acquire environment lock: {err}");
                })
                .ok();
        }

        let mode = if let Some(frozen_source) = frozen {
            LockMode::Frozen(frozen_source.into())
        } else if let LockCheck::Enabled(lock_check) = lock_check {
            LockMode::Locked(lock_interpreter, lock_check)
        } else if isolated {
            LockMode::DryRun(lock_interpreter)
        } else {
            LockMode::Write(lock_interpreter)
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
                return diagnostics::OperationDiagnostic::default()
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }
            Err(err) => return Err(err.into()),
        };

        let target = project::sync::identify_project_installation_target(
            project,
            result.lock(),
            all_packages,
            &package,
        );

        target.validate_extras(&extras)?;
        target.validate_groups(&groups)?;

        if ty_path.is_none()
            && ty_version.is_none()
            && let Some(tool) = project::toolchain::find_locked_tool(
                project,
                result.lock(),
                lock_interpreter,
                &PackageName::from_str("ty")?,
                &DEV_DEPENDENCIES,
                &groups,
            )?
        {
            locked_ty_path = Some(if !tool.requires_separate_environment() && !no_sync {
                // Synchronization will install the locked tool into the selected project or
                // isolated environment.
                venv.scripts()
                    .join(format!("ty{}", std::env::consts::EXE_SUFFIX))
            } else {
                // Do not modify the selected environment when synchronization is disabled or the
                // locked tool is excluded from it. Install only the locked `ty` subgraph.
                let base_interpreter =
                    CachedEnvironment::base_interpreter(lock_interpreter, cache)?;
                let resolution = project::toolchain::resolution_from_lock(
                    project,
                    result.lock(),
                    &tool,
                    &base_interpreter,
                    &settings.resolver.build_options,
                )?;
                project::sync::store_credentials_from_target(target, &client_builder)?;
                let ty_state = state.fork();
                let environment = match CachedEnvironment::from_locked_resolution(
                    &resolution,
                    result
                        .lock()
                        .build_constraints(project.workspace().install_path()),
                    &base_interpreter,
                    &settings,
                    &malware_settings,
                    &client_builder,
                    &ty_state,
                    Box::new(SummaryInstallLogger),
                    installer_metadata,
                    &concurrency,
                    cache,
                    printer,
                    preview,
                )
                .await
                {
                    Ok(environment) => environment,
                    Err(ProjectError::Operation(err)) => {
                        return diagnostics::OperationDiagnostic::default()
                            .report(err)
                            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                    }
                    Err(err) => return Err(err.into()),
                };
                malware_context.record_resolution(&resolution);
                PythonEnvironment::from(environment)
                    .scripts()
                    .join(format!("ty{}", std::env::consts::EXE_SUFFIX))
            });
        }

        if no_sync {
            debug!("Skipping environment synchronization due to `--no-sync`");
        } else {
            let sync_state = state.fork();
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
                malware_context,
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
        ty_version,
        ty_path.or(locked_ty_path),
        &target_dir,
        &check_targets,
        &excluded_targets,
        venv_path.as_deref(),
        exclude_newer,
        show_version,
        &client_builder,
        cache,
        printer,
    )
    .await
}
