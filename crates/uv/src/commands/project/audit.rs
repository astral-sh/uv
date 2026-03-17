use itertools::Itertools as _;
use owo_colors::OwoColorize;
use std::fmt::Write as _;
use std::path::Path;

use crate::commands::ExitStatus;
use crate::commands::diagnostics;
use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::resolution_markers;
use crate::commands::project::default_dependency_groups;
use crate::commands::project::lock::{LockMode, LockOperation};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState,
};
use crate::commands::reporters::AuditReporter;
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

use anyhow::Result;
use tracing::trace;
use uv_audit::service::{VulnerabilityServiceFormat, osv};
use uv_audit::types::{Dependency, Finding};
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
    service: VulnerabilityServiceFormat,
    service_url: Option<String>,
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
            &concurrency,
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
            return diagnostics::OperationDiagnostic::with_system_certs(
                client_builder.system_certs(),
            )
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

    // Build the list of auditable packages by traversing the lockfile from workspace roots,
    // respecting the user's extras and dependency-group filters. Workspace members are excluded
    // (they are local and have no external package identity), as are packages without a version.
    let auditable = lock.packages_for_audit(&extras, &groups);

    // Perform the audit.
    let reporter = AuditReporter::from(printer);
    let dependencies: Vec<Dependency> = auditable
        .iter()
        .map(|(name, version)| Dependency::new((*name).clone(), (*version).clone()))
        .collect();
    let base_client = client_builder.build();
    let all_findings = {
        match service {
            VulnerabilityServiceFormat::Osv => {
                let osv_url = service_url
                    .as_deref()
                    .unwrap_or(osv::API_BASE)
                    .parse()
                    .expect("invalid OSV service URL");
                let client = base_client.for_host(&osv_url).raw_client().clone();
                let service = osv::Osv::new(client, None, concurrency);
                trace!("Auditing {n} dependencies against OSV", n = auditable.len());
                service.query_batch(&dependencies).await?
            }
        }
    };

    reporter.on_audit_complete();

    let display = AuditResults {
        printer,
        n_packages: auditable.len(),
        findings: all_findings,
    };
    display.render()
}

struct AuditResults {
    printer: Printer,
    n_packages: usize,
    findings: Vec<Finding>,
}

impl AuditResults {
    fn render(&self) -> Result<ExitStatus> {
        let (vulns, statuses): (Vec<_>, Vec<_>) =
            self.findings.iter().partition_map(|finding| match finding {
                Finding::Vulnerability(vuln) => itertools::Either::Left(vuln),
                Finding::ProjectStatus(status) => itertools::Either::Right(status),
            });

        let vuln_banner = if !vulns.is_empty() {
            let s = if vulns.len() == 1 { "y" } else { "ies" };
            format!("{} known vulnerabilit{}", vulns.len(), s)
                .yellow()
                .to_string()
        } else {
            "no known vulnerabilities".bold().to_string()
        };

        let status_banner = if !statuses.is_empty() {
            let s = if statuses.len() == 1 { "" } else { "es" };
            format!(
                "{} adverse project status{}",
                statuses.len().to_string().yellow(),
                s
            )
        } else {
            "no adverse project statuses".bold().to_string()
        };

        writeln!(
            self.printer.stderr(),
            "Found {vuln_banner} and {status_banner} in {packages}",
            packages = format!("{npackages} packages", npackages = self.n_packages).bold()
        )?;

        let has_findings = !vulns.is_empty() || !statuses.is_empty();

        if !vulns.is_empty() {
            writeln!(self.printer.stdout_important(), "\nVulnerabilities:\n")?;

            // Group vulnerabilities by (dependency name, version).
            let groups = vulns
                .into_iter()
                .chunk_by(|vuln| (vuln.dependency.name(), vuln.dependency.version()));

            for (dependency, vulns) in &groups {
                let vulns: Vec<_> = vulns.collect();
                let (name, version) = dependency;

                writeln!(
                    self.printer.stdout_important(),
                    "{name_version} has {n} known vulnerabilit{ies}:\n",
                    name_version = format!("{name} {version}").bold(),
                    n = vulns.len(),
                    ies = if vulns.len() == 1 { "y" } else { "ies" },
                )?;

                for vuln in vulns {
                    writeln!(
                        self.printer.stdout_important(),
                        "- {id}: {description}",
                        id = vuln.best_id().as_str().bold(),
                        description = vuln.summary.as_deref().unwrap_or("No summary provided"),
                    )?;

                    if vuln.fix_versions.is_empty() {
                        writeln!(
                            self.printer.stdout_important(),
                            "\n  No fix versions available\n"
                        )?;
                    } else {
                        writeln!(
                            self.printer.stdout_important(),
                            "\n  Fixed in: {}\n",
                            vuln.fix_versions
                                .iter()
                                .map(std::string::ToString::to_string)
                                .join(", ")
                                .blue()
                        )?;
                    }

                    if let Some(link) = &vuln.link {
                        writeln!(
                            self.printer.stdout_important(),
                            "  Advisory information: {link}\n",
                            link = link.as_str().blue()
                        )?;
                    }
                }

                writeln!(self.printer.stdout_important())?;
            }
        }

        if !statuses.is_empty() {
            writeln!(self.printer.stdout_important(), "\nAdverse statuses:\n")?;

            // NOTE: Nothing here yet, since we don't actually produce
            // any adverse project statuses at the moment.
        }

        if has_findings {
            Ok(ExitStatus::Failure)
        } else {
            Ok(ExitStatus::Success)
        }
    }
}
