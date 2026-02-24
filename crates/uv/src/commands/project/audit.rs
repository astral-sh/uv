use std::path::Path;

use crate::{
    commands::{
        ExitStatus, diagnostics,
        pip::{loggers::DefaultResolveLogger, resolution_markers},
        project::{
            ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState,
            default_dependency_groups,
            lock::{LockMode, LockOperation},
            lock_target::LockTarget,
        },
        reporters::AuditReporter,
    },
    printer::Printer,
    settings::{FrozenSource, LockCheck, ResolverSettings},
};
use anyhow::Result;
use tracing::trace;
use uv_audit::{
    service::osv,
    types::{Dependency, Finding},
};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroups, ExtrasSpecification, TargetTriple};
use uv_normalize::{DefaultExtras, DefaultGroups};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_scripts::Pep723Script;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

pub(crate) async fn audit(
    project_dir: &Path,
    extras: ExtrasSpecification,
    groups: DependencyGroups,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    script: Option<Pep723Script>,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // Check if the audit feature is in preview
    if !preview.is_enabled(PreviewFeature::Audit) {
        warn_user!(
            "`uv audit` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Audit
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let workspace;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        workspace =
            Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
                .await?;
        LockTarget::Workspace(&workspace)
    };

    // Determine the groups to include.
    let default_groups = match target {
        LockTarget::Workspace(workspace) => default_dependency_groups(workspace.pyproject_toml())?,
        LockTarget::Script(_) => DefaultGroups::default(),
    };
    let groups = groups.with_defaults(default_groups);

    // Determine the extras to include.
    let default_extras = match &target {
        LockTarget::Workspace(_) => DefaultExtras::default(),
        LockTarget::Script(_) => DefaultExtras::default(),
    };
    let _extras = extras.with_defaults(default_extras);

    // Determine whether we're performing a universal audit.
    let universal = python_version.is_none() && python_platform.is_none();

    // Find an interpreter for the project, unless we're performing a frozen audit with a universal target.
    let interpreter = if frozen.is_some() && universal {
        None
    } else {
        Some(match target {
            LockTarget::Script(script) => ScriptInterpreter::discover(
                script.into(),
                None,
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                no_config,
                Some(false),
                &cache,
                printer,
                preview,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => ProjectInterpreter::discover(
                workspace,
                project_dir,
                &groups,
                None,
                &client_builder,
                python_preference,
                python_downloads,
                &install_mirrors,
                false,
                no_config,
                Some(false),
                &cache,
                printer,
                preview,
            )
            .await?
            .into_interpreter(),
        })
    };

    // Determine the lock mode.
    let mode = if let Some(frozen_source) = frozen {
        LockMode::Frozen(frozen_source.into())
    } else if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(interpreter.as_ref().unwrap(), lock_check)
    } else if matches!(target, LockTarget::Script(_)) && !target.lock_path().is_file() {
        // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
        LockMode::DryRun(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Update the lockfile, if necessary.
    let lock = match Box::pin(
        LockOperation::new(
            mode,
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            concurrency,
            &cache,
            &WorkspaceCache::default(),
            printer,
            preview,
        )
        .execute(target),
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(client_builder.is_native_tls())
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    // Determine the markers to use for resolution.
    let _markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    // TODO: validate the sets of requested extras/groups against the lockfile?

    // Perform the audit.
    // TODO: Use `client_builder` to produce an HTTP client through our normal process here.
    let service = osv::Osv::default();
    trace!(
        "Auditing {n} dependencies against OSV",
        n = lock.packages().len()
    );

    let reporter = AuditReporter::from(printer).with_length(lock.packages().len() as u64);

    let mut all_findings = vec![];
    for package in lock.packages() {
        let Some(version) = package.version() else {
            trace!(
                "Skipping audit for {} because it has no version information",
                package.name()
            );
            reporter.on_audit_progress();
            continue;
        };

        reporter.on_audit_package(package.name(), version);

        let dependency = Dependency::new(package.name().clone(), version.clone());
        all_findings.extend(service.query(&dependency).await?);
    }

    reporter.on_audit_complete();

    for finding in all_findings {
        println!("{finding:?}");
    }

    Ok(ExitStatus::Success)
}

struct AuditDisplay {
    findings: Vec<Finding>,
}
