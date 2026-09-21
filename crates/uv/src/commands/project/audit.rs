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
    ProjectEnvironmentPolicy, ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState,
    WorkspacePython,
};
use crate::commands::reporters::AuditReporter;
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

use anyhow::Result;
use rustc_hash::FxHashSet;
use tracing::trace;
use uv_audit::{
    AdverseStatus, Dependency, Finding, ProjectStatus, ProjectStatusAudit, Vulnerability,
    VulnerabilityID, VulnerabilityServiceFormat, osv,
};
use uv_cache::Cache;
use uv_cli::AuditOutputFormat;
use uv_client::{BaseClientBuilder, CachedClient, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, ExtrasSpecification,
    ExtrasSpecificationWithDefaults, TargetTriple,
};
use uv_distribution_types::{IndexCapabilities, IndexUrl};
use uv_fs::{CWD, find_git_repository_root, relative_to};
use uv_normalize::{DefaultExtras, DefaultGroups};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{ConfigDiscovery, PythonDownloads, PythonPreference, PythonVersion};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::Lock;
use uv_scripts::Pep723Script;
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

pub(crate) mod json;
pub(crate) mod sarif;

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
        LockTarget::Workspace(workspace) => default_dependency_groups(workspace.pyproject_toml())?,
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
                python_downloads,
                &install_mirrors,
                false,
                config_discovery,
                Some(false),
                &cache,
                printer,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => {
                let workspace_python = WorkspacePython::from_request(
                    None,
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
                    ProjectEnvironmentPolicy::Optional,
                    Some(false),
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
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::default()
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

    let outcome = audit_lock(
        &lock,
        target.install_path(),
        &extras,
        &groups,
        &settings,
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

/// Audit findings and ignore-rule matches for one lockfile.
pub(crate) struct AuditOutcome {
    pub(crate) n_packages: usize,
    pub(crate) findings: Vec<Finding>,
    pub(crate) matched_ignores: FxHashSet<VulnerabilityID>,
}

/// Audit the dependency graph reachable from a project, script, or tool lockfile.
pub(crate) async fn audit_lock(
    lock: &Lock,
    root: &Path,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    settings: &ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    service: VulnerabilityServiceFormat,
    service_url: Option<DisplaySafeUrl>,
    ignore: &[VulnerabilityID],
    ignore_until_fixed: &[VulnerabilityID],
) -> Result<AuditOutcome> {
    let auditable = lock.auditable(extras, groups, |_| true);
    let mut projects = auditable.projects(root)?;

    // Flat indexes cannot provide PEP 792 project-status metadata.
    let flat_index_urls: FxHashSet<&IndexUrl> = settings
        .index_locations
        .flat_indexes()
        .map(|index| &index.url)
        .collect();
    projects.retain(|(_, url)| !flat_index_urls.contains(url));

    let reporter = AuditReporter::from(printer);
    let dependencies: Vec<Dependency> = auditable
        .packages()
        .map(|(name, version)| Dependency::new(name.clone(), version.clone()))
        .collect();
    let base_client = client_builder.clone().build()?;
    let registry_client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(settings.index_locations.clone())
        .keyring(settings.keyring_provider)
        .build()?;
    let capabilities = IndexCapabilities::default();
    let status_audit =
        ProjectStatusAudit::new(&registry_client, &capabilities, concurrency.clone());

    let osv_future = async {
        match service {
            VulnerabilityServiceFormat::Osv => {
                let client = CachedClient::new(base_client);
                let service = osv::Osv::new(client, service_url, concurrency, cache.clone());
                trace!("Auditing {n} dependencies against OSV", n = auditable.len());
                service.query_batch(&dependencies, osv::Filter::All).await
            }
        }
    };
    let status_future = async {
        trace!(
            "Auditing {n} projects for adverse status",
            n = projects.len()
        );
        status_audit.query_batch(&projects).await
    };
    let (osv_findings, status_findings) = tokio::join!(osv_future, status_future);
    let mut findings = osv_findings?;
    findings.extend(status_findings);
    reporter.on_audit_complete();

    let mut matched_ignores = FxHashSet::default();
    let findings = findings
        .into_iter()
        .filter(|finding| match finding {
            Finding::Vulnerability(vulnerability) => {
                if let Some(id) = ignore.iter().find(|id| vulnerability.matches(id)) {
                    matched_ignores.insert(id.clone());
                    return false;
                }
                if let Some(id) = ignore_until_fixed
                    .iter()
                    .find(|id| vulnerability.matches(id))
                {
                    matched_ignores.insert(id.clone());
                    if vulnerability.fix_versions.is_empty() {
                        return false;
                    }
                }
                true
            }
            Finding::ProjectStatus(_) => true,
        })
        .collect();

    Ok(AuditOutcome {
        n_packages: auditable.len(),
        findings,
        matched_ignores,
    })
}

/// Warn once for each ignore rule that did not match an audited vulnerability.
pub(crate) fn warn_unmatched_ignores(
    ignore: &[VulnerabilityID],
    ignore_until_fixed: &[VulnerabilityID],
    matched_ignores: &FxHashSet<VulnerabilityID>,
    scope: &str,
) {
    for id in ignore.iter().chain(ignore_until_fixed.iter()) {
        if !matched_ignores.contains(id) {
            warn_user!(
                "Ignored vulnerability `{}` does not match any vulnerability in {scope}",
                id.as_str()
            );
        }
    }
}

/// Resolve a lockfile path into the URI used by SARIF consumers.
pub(crate) fn artifact_uri(path: &Path) -> String {
    let path = if let Some(repository_root) = find_git_repository_root(path)
        && let Ok(relative) = relative_to(path, repository_root)
    {
        relative
    } else if let Ok(relative) = path.strip_prefix(&*CWD) {
        relative.to_path_buf()
    } else {
        path.to_path_buf()
    };
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) struct AuditResults {
    pub(crate) printer: Printer,
    pub(crate) n_packages: usize,
    pub(crate) output_format: AuditOutputFormat,
    pub(crate) findings: Vec<Finding>,
    pub(crate) artifact_uri: String,
}

impl AuditResults {
    pub(crate) fn render(&self) -> Result<ExitStatus> {
        match self.output_format {
            AuditOutputFormat::Text => self.render_text(),
            AuditOutputFormat::Json => self.render_json(),
            AuditOutputFormat::Sarif => self.render_sarif(),
        }
    }

    fn split_findings(&self) -> (Vec<&Vulnerability>, Vec<&ProjectStatus>) {
        self.findings.iter().partition_map(|finding| match finding {
            Finding::Vulnerability(vulnerability) => {
                itertools::Either::Left(vulnerability.as_ref())
            }
            Finding::ProjectStatus(status) => itertools::Either::Right(status),
        })
    }

    pub(crate) fn exit_status(&self) -> ExitStatus {
        // NOTE: intentional: we don't currently fail if there are any adverse statuses,
        // only when there are vulnerabilities. We will likely change this once we allow users
        // to ignore adverse statuses and configure policies.
        if self
            .findings
            .iter()
            .any(|finding| matches!(finding, Finding::Vulnerability(_)))
        {
            ExitStatus::Failure
        } else {
            ExitStatus::Success
        }
    }

    fn render_text(&self) -> Result<ExitStatus> {
        let (vulnerabilities, statuses) = self.split_findings();

        let vulnerability_banner = if !vulnerabilities.is_empty() {
            let suffix = if vulnerabilities.len() == 1 {
                "y"
            } else {
                "ies"
            };
            format!("{} known vulnerabilit{suffix}", vulnerabilities.len())
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
            "Found {vulnerability_banner} and {status_banner} in {packages}",
            packages = format!(
                "{npackages} {label}",
                npackages = self.n_packages,
                label = if self.n_packages == 1 {
                    "package"
                } else {
                    "packages"
                }
            )
            .bold()
        )?;

        if !vulnerabilities.is_empty() {
            writeln!(self.printer.stdout_important(), "\nVulnerabilities:\n")?;

            // Group vulnerabilities by (dependency name, version).
            let groups = vulnerabilities.into_iter().chunk_by(|vulnerability| {
                (
                    vulnerability.dependency.name(),
                    vulnerability.dependency.version(),
                )
            });

            for (dependency, vulnerabilities) in &groups {
                let vulnerabilities: Vec<_> = vulnerabilities.collect();
                let (name, version) = dependency;

                writeln!(
                    self.printer.stdout_important(),
                    "{name_version} has {n} known vulnerabilit{ies}:\n",
                    name_version = format!("{name} {version}").bold(),
                    n = vulnerabilities.len(),
                    ies = if vulnerabilities.len() == 1 {
                        "y"
                    } else {
                        "ies"
                    },
                )?;

                for vulnerability in vulnerabilities {
                    writeln!(
                        self.printer.stdout_important(),
                        "- {id}: {description}",
                        id = vulnerability.best_id().as_str().bold(),
                        description = vulnerability
                            .summary
                            .as_deref()
                            .unwrap_or("No summary provided"),
                    )?;

                    if vulnerability.fix_versions.is_empty() {
                        writeln!(
                            self.printer.stdout_important(),
                            "\n  No fix versions available\n"
                        )?;
                    } else {
                        writeln!(
                            self.printer.stdout_important(),
                            "\n  Fixed in: {}\n",
                            vulnerability
                                .fix_versions
                                .iter()
                                .map(std::string::ToString::to_string)
                                .join(", ")
                                .blue()
                        )?;
                    }

                    if let Some(link) = &vulnerability.link {
                        writeln!(
                            self.printer.stdout_important(),
                            "  Advisory information: {link}\n",
                            link = link.as_str().blue()
                        )?;
                    }
                }
            }
        }

        if !statuses.is_empty() {
            writeln!(self.printer.stdout_important(), "\nAdverse statuses:\n")?;

            for status in statuses {
                let label = match &status.status {
                    AdverseStatus::Archived | AdverseStatus::Deprecated => {
                        status.status.to_string().yellow().to_string()
                    }
                    AdverseStatus::Quarantined => status.status.to_string().red().to_string(),
                };
                let name = status.name.bold();
                if let Some(reason) = &status.reason {
                    writeln!(
                        self.printer.stdout_important(),
                        "- {name} is {label}: {reason}"
                    )?;
                } else {
                    writeln!(self.printer.stdout_important(), "- {name} is {label}")?;
                }
            }
        }

        Ok(self.exit_status())
    }

    fn render_json(&self) -> Result<ExitStatus> {
        let (vulnerabilities, statuses) = self.split_findings();
        let report = json::Report::from_findings(self.n_packages, &vulnerabilities, &statuses);

        writeln!(
            self.printer.stdout_important(),
            "{}",
            serde_json::to_string_pretty(&report)?
        )?;

        Ok(self.exit_status())
    }

    fn render_sarif(&self) -> Result<ExitStatus> {
        let (vulnerabilities, statuses) = self.split_findings();
        let report = sarif::Report::from_findings(&vulnerabilities, &statuses, &self.artifact_uri);

        writeln!(
            self.printer.stdout_important(),
            "{}",
            serde_json::to_string_pretty(&report)?
        )?;

        Ok(self.exit_status())
    }
}
