//! Shared auditing and reporting for project and tool lockfiles.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools as _;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use tracing::trace;
use uv_audit::{
    AdverseStatus, Dependency, Finding, ProjectStatus, ProjectStatusAudit, Vulnerability,
    VulnerabilityID, VulnerabilityServiceFormat, osv,
};
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, CachedClient, RegistryClientBuilder};
use uv_command_support::{ExitStatus, Printer};
use uv_configuration::{
    AuditOutputFormat, Concurrency, DependencyGroupsWithDefaults, ExtrasSpecificationWithDefaults,
    KeyringProviderType,
};
use uv_distribution_types::{IndexCapabilities, IndexLocations, IndexUrl};
use uv_fs::{CWD, find_git_repository_root, relative_to};
use uv_lock::Lock;
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user;

mod reporter;
use reporter::AuditReporter;
pub mod json;
pub mod sarif;

/// Audit findings and ignore-rule matches for one lockfile.
pub struct AuditOutcome {
    pub n_packages: usize,
    pub findings: Vec<Finding>,
    pub matched_ignores: FxHashSet<VulnerabilityID>,
}

/// Audit the dependency graph reachable from a project, script, or tool lockfile.
pub async fn audit_lock(
    lock: &Lock,
    root: &Path,
    extras: &ExtrasSpecificationWithDefaults,
    groups: &DependencyGroupsWithDefaults,
    index_locations: &IndexLocations,
    keyring_provider: KeyringProviderType,
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
    let flat_index_urls: FxHashSet<&IndexUrl> = index_locations
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
        .index_locations(index_locations.clone())
        .keyring(keyring_provider)
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
pub fn warn_unmatched_ignores(
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
pub fn artifact_uri(path: &Path) -> String {
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

pub struct AuditResults {
    pub printer: Printer,
    pub n_packages: usize,
    pub output_format: AuditOutputFormat,
    pub findings: Vec<Finding>,
    pub artifact_uri: String,
}

impl AuditResults {
    pub fn render(&self) -> Result<ExitStatus> {
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

    pub fn exit_status(&self) -> ExitStatus {
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
