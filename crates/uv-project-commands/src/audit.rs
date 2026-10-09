use std::path::Path;

use anyhow::{Result, bail};

use uv_audit::{VulnerabilityID, VulnerabilityServiceFormat};
use uv_audit_operations::{AuditResults, artifact_uri, audit_lock, warn_unmatched_ignores};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, AuditOutputFormat, Concurrency, DependencyGroups, ExtrasSpecification,
    TargetTriple,
};
use uv_dispatch::UniversalState;
use uv_environment_operations::{
    ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter,
};
use uv_lock_operations::{LockMode, LockOperation, LockTarget};
use uv_normalize::{DefaultExtras, DefaultGroups};
use uv_preview::{Preview, PreviewFeature};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_discovery::ScriptInterpreter;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonVersion};
use uv_redacted::DisplaySafeUrl;
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_resolve_operations::resolution_markers;
use uv_scripts::Pep723Script;
use uv_settings::{FrozenSource, LockCheck, PythonInstallMirrors, ResolverSettings};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

pub async fn audit(
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
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
    output_format: AuditOutputFormat,
    service: VulnerabilityServiceFormat,
    service_url: Option<DisplaySafeUrl>,
    ignore: Vec<VulnerabilityID>,
    ignore_until_fixed: Vec<VulnerabilityID>,
) -> Result<ExitStatus> {
    if client_builder.is_offline() {
        bail!("Auditing requires network access and cannot be performed in offline mode");
    }

    // Check if the audit feature is in preview
    if !preview.is_enabled(PreviewFeature::AuditCommand) {
        warn_user!(
            "`uv audit` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::AuditCommand
        );
    }
    if matches!(output_format, AuditOutputFormat::Json)
        && !preview.is_enabled(PreviewFeature::JsonOutput)
    {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    let workspace;
    let target = if let Some(script) = script.as_ref() {
        LockTarget::Script(script)
    } else {
        workspace = Workspace::discover(
            project_dir,
            &DiscoveryOptions::default(),
            &cache,
            workspace_cache,
        )
        .await?;
        LockTarget::Workspace(&workspace)
    };

    // Determine the groups to include.
    let default_groups = match target {
        LockTarget::Workspace(workspace) => workspace.default_groups()?,
        LockTarget::Script(_) => DefaultGroups::default(),
    };
    let groups = groups.with_defaults(default_groups);

    // Determine the extras to include.
    let default_extras = match &target {
        LockTarget::Workspace(_) => DefaultExtras::All,
        LockTarget::Script(_) => DefaultExtras::All,
    };
    let extras = extras.with_defaults(default_extras);

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
                python_arch,
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                ActiveEnvironment::Ignore,
                &cache,
                printer,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => {
                let project_python = ProjectPythonRequest::from_request(
                    None,
                    Some(workspace),
                    &groups,
                    project_dir,
                    config_discovery,
                )
                .await?;
                ProjectInterpreter::discover(
                    ProjectEnvironmentTarget::from(workspace),
                    project_python,
                    &client_builder,
                    python_preference,
                    python_arch,
                    python_downloads,
                    &install_mirrors,
                    ProjectEnvironmentPolicy::Optional,
                    ActiveEnvironment::Ignore,
                    &cache,
                    printer,
                )
                .await?
                .into_interpreter()
            }
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
            &concurrency,
            &cache,
            workspace_cache,
            printer,
            preview,
        )
        .execute(target),
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(err) => return Err(UvError::from(err).into()),
    };

    // Determine the markers to use for resolution.
    let _markers = (!universal).then(|| {
        resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            interpreter.as_ref().unwrap(),
        )
    });

    let outcome = audit_lock(
        &lock,
        target.install_path(),
        &extras,
        &groups,
        &settings.index_locations,
        settings.keyring_provider,
        client_builder,
        concurrency,
        &cache,
        printer,
        service,
        service_url,
        &ignore,
        &ignore_until_fixed,
    )
    .await?;

    warn_unmatched_ignores(
        &ignore,
        &ignore_until_fixed,
        &outcome.matched_ignores,
        "the project",
    );

    let display = AuditResults {
        printer,
        n_packages: outcome.n_packages,
        output_format,
        findings: outcome.findings,
        artifact_uri: {
            let lock_path = target.lock_path();
            // If we've run `uv audit --script`, we might only have an in-memory lockfile.
            // In that case, use the script's own path as the artifact path.
            let artifact_path = if let LockTarget::Script(script) = target
                && !lock_path.is_file()
            {
                script.path.as_path()
            } else {
                lock_path.as_path()
            };
            artifact_uri(artifact_path)
        },
    };
    display.render()
}
