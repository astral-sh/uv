use itertools::Itertools as _;
use owo_colors::OwoColorize;
use serde::Serialize;
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
    ProjectError, ProjectInterpreter, ScriptInterpreter, UniversalState, WorkspacePython,
};
use crate::commands::reporters::AuditReporter;
use crate::printer::Printer;
use crate::settings::{FrozenSource, LockCheck, ResolverSettings};

use anyhow::Result;
use rustc_hash::FxHashSet;
use tracing::trace;
use uv_audit::{
    AdverseStatus, Dependency, Finding, ProjectStatus, ProjectStatusAudit, Vulnerability,
    VulnerabilityID, VulnerabilityServiceFormat, osv, pypi,
};
use uv_cache_key::CanonicalUrl;
use uv_cache::Cache;
use uv_cli::AuditOutputFormat;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{Concurrency, DependencyGroups, ExtrasSpecification, TargetTriple};
use uv_distribution_types::{IndexCapabilities, IndexUrl};
use uv_normalize::{DefaultExtras, DefaultGroups};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{PythonDownloads, PythonPreference, PythonVersion};
use uv_redacted::DisplaySafeUrl;
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
    output_format: AuditOutputFormat,
    service: VulnerabilityServiceFormat,
    service_url: Option<String>,
    ignore: Vec<VulnerabilityID>,
    ignore_until_fixed: Vec<VulnerabilityID>,
) -> Result<ExitStatus> {
    // Check if the audit feature is in preview
    if !preview.is_enabled(PreviewFeature::Audit) {
        warn_user!(
            "`uv audit` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Audit
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
                no_config,
                Some(false),
                &cache,
                printer,
                preview,
            )
            .await?
            .into_interpreter(),
            LockTarget::Workspace(workspace) => {
                let workspace_python = WorkspacePython::from_request(
                    None,
                    Some(workspace),
                    &groups,
                    project_dir,
                    no_config,
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
                    Some(false),
                    &cache,
                    printer,
                    preview,
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

    // Build the set of auditable packages by traversing the lockfile from workspace roots,
    // respecting the user's extras and dependency-group filters. Workspace members are excluded
    // (they are local and have no external package identity), as are packages without a version.
    // The `Auditable` view offers per-version and per-project projections from a single walk.
    let auditable = lock.auditable(&extras, &groups);
    let mut projects = auditable.projects(target.install_path())?;

    // Drop projects whose index is configured as flat, since we know we won't
    // find PEP 792 statuses.
    let flat_index_urls: FxHashSet<&IndexUrl> = settings
        .index_locations
        .flat_indexes()
        .map(|index| &index.url)
        .collect();
    projects.retain(|(_, url)| !flat_index_urls.contains(url));

    // Perform the audit.
    let reporter = AuditReporter::from(printer);
    let base_client = client_builder.clone().build()?;

    let registry_client = RegistryClientBuilder::new(client_builder, cache.clone())
        .index_locations(settings.index_locations.clone())
        .keyring(settings.keyring_provider)
        .build()?;
    let capabilities = IndexCapabilities::default();
    let status_audit =
        ProjectStatusAudit::new(&registry_client, &capabilities, concurrency.clone());

    let vulnerability_future = async {
        match service {
            VulnerabilityServiceFormat::Osv => {
                let dependencies: Vec<Dependency> = auditable
                    .packages()
                    .map(|(name, version)| Dependency::new(name.clone(), version.clone()))
                    .collect();
                let osv_url = service_url
                    .as_deref()
                    .unwrap_or(osv::API_BASE)
                    .parse()
                    .expect("invalid OSV service URL");
                let client = base_client.for_host(&osv_url).raw_client().clone();
                let service = osv::Osv::new(client, Some(osv_url), concurrency);
                trace!("Auditing {n} dependencies against OSV", n = auditable.len());
                service
                    .query_batch(&dependencies, osv::Filter::All)
                    .await
                    .map_err(anyhow::Error::from)
                    .map(|findings| (findings, 0usize))
            }
            VulnerabilityServiceFormat::Pypi => {
                let pypi_url = service_url
                    .as_deref()
                    .unwrap_or(pypi::API_BASE)
                    .parse()
                    .expect("invalid PyPI service URL");
                let pypi_root = pypi_service_root(&pypi_url);
                let mut dependencies = Vec::with_capacity(auditable.len());
                let mut skipped = 0usize;
                for package in auditable.entries() {
                    let Some(index) = package.index(target.install_path())? else {
                        skipped += 1;
                        continue;
                    };
                    if !pypi_service_compatible(&index, &pypi_root) {
                        skipped += 1;
                        continue;
                    }
                    dependencies.push(Dependency::new(
                        package.name().clone(),
                        package.version().clone(),
                    ));
                }
                let client = base_client.for_host(&pypi_url).raw_client().clone();
                let service = pypi::Pypi::new(client, Some(pypi_url), concurrency);
                trace!("Auditing {n} dependencies against PyPI", n = auditable.len());
                service
                    .query_batch(&dependencies)
                    .await
                    .map_err(anyhow::Error::from)
                    .map(|findings| (findings, skipped))
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
    let (vulnerability_findings, status_findings) =
        tokio::join!(vulnerability_future, status_future);
    let (mut all_findings, skipped) = vulnerability_findings?;
    all_findings.extend(status_findings);

    reporter.on_audit_complete();

    if skipped > 0 {
        let package_noun = if skipped == 1 { "package" } else { "packages" };
        let pronoun = if skipped == 1 { "its" } else { "their" };
        warn_user!(
            "Skipped {skipped} {package_noun} during PyPI vulnerability lookup because {pronoun} source index is not compatible with the configured service URL."
        );
    }

    // Filter out ignored vulnerabilities, tracking how many were ignored
    // and which ignore rules actually matched.
    let mut matched_ignores: FxHashSet<&VulnerabilityID> = FxHashSet::default();
    let all_findings: Vec<_> = all_findings
        .into_iter()
        .filter(|finding| match finding {
            Finding::Vulnerability(vulnerability) => {
                if let Some(id) = ignore.iter().find(|id| vulnerability.matches(id)) {
                    matched_ignores.insert(id);
                    return false;
                }
                if let Some(id) = ignore_until_fixed
                    .iter()
                    .find(|id| vulnerability.matches(id))
                {
                    matched_ignores.insert(id);
                    if vulnerability.fix_versions.is_empty() {
                        return false;
                    }
                }
                true
            }
            Finding::ProjectStatus(_) => true,
        })
        .collect();

    // Warn about ignore rules that didn't match any vulnerability.
    for id in ignore.iter().chain(ignore_until_fixed.iter()) {
        if !matched_ignores.contains(id) {
            warn_user!(
                "Ignored vulnerability `{}` does not match any vulnerability in the project",
                id.as_str()
            );
        }
    }

    let display = AuditResults {
        printer,
        n_packages: auditable.len(),
        output_format,
        findings: all_findings,
    };
    display.render()
}

fn pypi_service_root(service_url: &DisplaySafeUrl) -> DisplaySafeUrl {
    let mut root = service_url.clone();
    let strip_simple = {
        let Some(mut segments) = root.path_segments() else {
            return root;
        };
        let last = match segments.next_back() {
            Some("") => segments.next_back(),
            segment => segment,
        };
        matches!(last, Some(segment) if segment.eq_ignore_ascii_case("simple") || segment.eq_ignore_ascii_case("+simple"))
    };
    if strip_simple && let Ok(mut segments) = root.path_segments_mut() {
        segments.pop_if_empty().pop();
    }
    root
}

fn pypi_service_compatible(index: &IndexUrl, service_root: &DisplaySafeUrl) -> bool {
    let Some(index_root) = index.root() else {
        return false;
    };
    CanonicalUrl::new(&index_root) == CanonicalUrl::new(service_root)
}

struct AuditResults {
    printer: Printer,
    n_packages: usize,
    output_format: AuditOutputFormat,
    findings: Vec<Finding>,
}

impl AuditResults {
    fn render(&self) -> Result<ExitStatus> {
        match self.output_format {
            AuditOutputFormat::Text => self.render_text(),
            AuditOutputFormat::Json => self.render_json(),
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

    fn exit_status(&self) -> ExitStatus {
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
        let report = JsonReport::from_findings(self.n_packages, &vulnerabilities, &statuses);

        writeln!(
            self.printer.stdout_important(),
            "{}",
            serde_json::to_string_pretty(&report)?
        )?;

        Ok(self.exit_status())
    }
}

#[derive(Debug, Serialize)]
struct JsonReport {
    schema: JsonSchema,
    summary: JsonSummary,
    vulnerabilities: Vec<JsonVulnerability>,
    adverse_statuses: Vec<JsonAdverseStatus>,
}

impl JsonReport {
    fn from_findings(
        n_packages: usize,
        vulnerabilities: &[&Vulnerability],
        statuses: &[&ProjectStatus],
    ) -> Self {
        let mut vulnerabilities = vulnerabilities
            .iter()
            .copied()
            .map(JsonVulnerability::from)
            .collect::<Vec<_>>();
        vulnerabilities.sort_by(|first, second| {
            first
                .dependency
                .name
                .cmp(&second.dependency.name)
                .then_with(|| first.dependency.version.cmp(&second.dependency.version))
                .then_with(|| first.display_id.cmp(&second.display_id))
        });

        let mut adverse_statuses = statuses
            .iter()
            .copied()
            .map(JsonAdverseStatus::from)
            .collect::<Vec<_>>();
        adverse_statuses.sort_by(|first, second| {
            first
                .name
                .cmp(&second.name)
                .then_with(|| first.status.cmp(&second.status))
        });

        Self {
            schema: JsonSchema::default(),
            summary: JsonSummary {
                audited_packages: n_packages,
                vulnerabilities: vulnerabilities.len(),
                adverse_statuses: adverse_statuses.len(),
            },
            vulnerabilities,
            adverse_statuses,
        }
    }
}

#[derive(Debug, Serialize, Default)]
struct JsonSchema {
    version: JsonSchemaVersion,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "snake_case")]
enum JsonSchemaVersion {
    #[default]
    Preview,
}

#[derive(Debug, Serialize)]
struct JsonSummary {
    audited_packages: usize,
    vulnerabilities: usize,
    adverse_statuses: usize,
}

#[derive(Debug, Serialize)]
struct JsonDependency {
    name: String,
    version: String,
}

impl From<&Dependency> for JsonDependency {
    fn from(dependency: &Dependency) -> Self {
        Self {
            name: dependency.name().to_string(),
            version: dependency.version().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonVulnerability {
    dependency: JsonDependency,
    id: String,
    display_id: String,
    aliases: Vec<String>,
    summary: Option<String>,
    description: Option<String>,
    link: Option<String>,
    fix_versions: Vec<String>,
    published: Option<String>,
    modified: Option<String>,
}

impl From<&Vulnerability> for JsonVulnerability {
    fn from(vulnerability: &Vulnerability) -> Self {
        Self {
            dependency: JsonDependency::from(&vulnerability.dependency),
            id: vulnerability.id.as_str().to_string(),
            display_id: vulnerability.best_id().as_str().to_string(),
            aliases: vulnerability
                .aliases
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            summary: vulnerability.summary.clone(),
            description: vulnerability.description.clone(),
            link: vulnerability
                .link
                .as_ref()
                .map(|link| link.as_str().to_string()),
            fix_versions: vulnerability
                .fix_versions
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            published: vulnerability
                .published
                .as_ref()
                .map(std::string::ToString::to_string),
            modified: vulnerability
                .modified
                .as_ref()
                .map(std::string::ToString::to_string),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonAdverseStatus {
    name: String,
    status: String,
    reason: Option<String>,
}

impl From<&ProjectStatus> for JsonAdverseStatus {
    fn from(status: &ProjectStatus) -> Self {
        Self {
            name: status.name.to_string(),
            status: status.status.to_string(),
            reason: status.reason.clone(),
        }
    }
}
