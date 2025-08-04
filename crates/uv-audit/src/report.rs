use crate::matcher::{DatabaseStats, FixAnalysis};
use crate::sarif::SarifGenerator;
use crate::scanner::DependencyStats;
use crate::vulnerability::{Severity, VulnerabilityMatch};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use uv_cli::AuditFormat;

/// A complete audit report containing all findings
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// Timestamp when the audit was performed
    pub scan_time: Timestamp,
    /// Statistics about dependencies scanned
    pub dependency_stats: DependencyStats,
    /// Statistics about the vulnerability database
    pub database_stats: DatabaseStats,
    /// All vulnerability matches found
    pub matches: Vec<VulnerabilityMatch>,
    /// Analysis of available fixes
    pub fix_analysis: FixAnalysis,
    /// Warnings generated during the audit
    pub warnings: Vec<String>,
}

impl AuditReport {
    /// Create a new audit report
    pub fn new(
        dependency_stats: DependencyStats,
        database_stats: DatabaseStats,
        matches: Vec<VulnerabilityMatch>,
        fix_analysis: FixAnalysis,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            scan_time: Timestamp::now(),
            dependency_stats,
            database_stats,
            matches,
            fix_analysis,
            warnings,
        }
    }

    /// Check if the audit found any vulnerabilities
    pub fn has_vulnerabilities(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Get summary statistics
    pub fn summary(&self) -> AuditSummary {
        let mut severity_counts = HashMap::new();
        let mut package_counts = HashMap::new();

        for m in &self.matches {
            *severity_counts.entry(m.vulnerability.severity).or_insert(0) += 1;
            *package_counts.entry(m.package_name.clone()).or_insert(0) += 1;
        }

        AuditSummary {
            total_packages_scanned: self.dependency_stats.total,
            vulnerable_packages: package_counts.len(),
            total_vulnerabilities: self.matches.len(),
            severity_counts,
            fixable_vulnerabilities: self.fix_analysis.fixable,
            unfixable_vulnerabilities: self.fix_analysis.unfixable,
        }
    }
}

/// Summary statistics for an audit
#[derive(Debug, Clone)]
pub struct AuditSummary {
    pub total_packages_scanned: usize,
    pub vulnerable_packages: usize,
    pub total_vulnerabilities: usize,
    pub severity_counts: HashMap<Severity, usize>,
    pub fixable_vulnerabilities: usize,
    pub unfixable_vulnerabilities: usize,
}

/// Report generator for different output formats
pub struct ReportGenerator;

impl ReportGenerator {
    /// Generate a report in the specified format
    pub fn generate(
        report: &AuditReport,
        format: AuditFormat,
        project_root: Option<&Path>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        match format {
            AuditFormat::Human => Self::generate_human_report(report),
            AuditFormat::Json => Self::generate_json_report(report),
            AuditFormat::Sarif => Self::generate_sarif_report(report, project_root),
        }
    }

    /// Generate a human-readable report
    fn generate_human_report(report: &AuditReport) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();
        let summary = report.summary();

        // Header
        writeln!(output, "🛡️  uv audit report")?;
        writeln!(output, "━━━━━━━━━━━━━━━━━━━━━━")?;
        writeln!(output)?;

        // Summary
        writeln!(output, "📊 Scan Summary")?;
        writeln!(
            output,
            "├─ Scanned: {} packages",
            summary.total_packages_scanned
        )?;
        writeln!(
            output,
            "├─ Vulnerable: {} packages",
            summary.vulnerable_packages
        )?;
        writeln!(
            output,
            "└─ Vulnerabilities: {}",
            summary.total_vulnerabilities
        )?;
        writeln!(output)?;

        // Severity breakdown
        if !summary.severity_counts.is_empty() {
            writeln!(output, "🚨 Severity Breakdown")?;
            for severity in [
                Severity::Critical,
                Severity::High,
                Severity::Medium,
                Severity::Low,
            ] {
                if let Some(count) = summary.severity_counts.get(&severity) {
                    let icon = match severity {
                        Severity::Critical => "🔴",
                        Severity::High => "🟠",
                        Severity::Medium => "🟡",
                        Severity::Low => "🟢",
                    };
                    writeln!(output, "├─ {} {:?}: {}", icon, severity, count)?;
                }
            }
            writeln!(output)?;
        }

        // Fix analysis
        if report.fix_analysis.total_matches > 0 {
            writeln!(output, "🔧 Fix Analysis")?;
            writeln!(output, "├─ Fixable: {}", report.fix_analysis.fixable)?;
            writeln!(output, "└─ Unfixable: {}", report.fix_analysis.unfixable)?;
            writeln!(output)?;
        }

        // Warnings
        if !report.warnings.is_empty() {
            writeln!(output, "⚠️  Warnings")?;
            for warning in &report.warnings {
                writeln!(output, "├─ {}", warning)?;
            }
            writeln!(output)?;
        }

        // Vulnerability details
        if !report.matches.is_empty() {
            writeln!(output, "🐛 Vulnerabilities Found")?;
            writeln!(output, "━━━━━━━━━━━━━━━━━━━━━━━━━")?;
            writeln!(output)?;

            for (i, m) in report.matches.iter().enumerate() {
                let severity_icon = match m.vulnerability.severity {
                    Severity::Critical => "🔴",
                    Severity::High => "🟠",
                    Severity::Medium => "🟡",
                    Severity::Low => "🟢",
                };

                writeln!(
                    output,
                    "{}. {} {}",
                    i + 1,
                    severity_icon,
                    m.vulnerability.id
                )?;
                writeln!(
                    output,
                    "   Package: {} v{}",
                    m.package_name, m.installed_version
                )?;
                writeln!(output, "   Severity: {:?}", m.vulnerability.severity)?;

                if let Some(cvss) = m.vulnerability.cvss_score {
                    writeln!(output, "   CVSS Score: {:.1}", cvss)?;
                }

                writeln!(output, "   Summary: {}", m.vulnerability.summary)?;

                if let Some(description) = &m.vulnerability.description {
                    writeln!(output, "   Description: {}", description)?;
                }

                if !m.vulnerability.fixed_versions.is_empty() {
                    writeln!(
                        output,
                        "   Fixed in: {}",
                        m.vulnerability
                            .fixed_versions
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }

                if !m.vulnerability.references.is_empty() {
                    writeln!(output, "   References:")?;
                    for ref_url in &m.vulnerability.references {
                        writeln!(output, "     - {}", ref_url)?;
                    }
                }

                writeln!(output)?;
            }
        } else {
            writeln!(output, "✅ No vulnerabilities found!")?;
        }

        // Fix suggestions
        if !report.fix_analysis.fix_suggestions.is_empty() {
            writeln!(output, "💡 Fix Suggestions")?;
            writeln!(output, "━━━━━━━━━━━━━━━━━━━")?;
            writeln!(output)?;

            for suggestion in &report.fix_analysis.fix_suggestions {
                writeln!(output, "• {}", suggestion)?;
            }
            writeln!(output)?;
        }

        // Footer
        writeln!(
            output,
            "Scan completed at {}",
            report.scan_time.strftime("%Y-%m-%d %H:%M:%S UTC")
        )?;

        Ok(output)
    }

    /// Generate a JSON report
    fn generate_json_report(report: &AuditReport) -> Result<String, Box<dyn std::error::Error>> {
        let summary = report.summary();

        let json_report = JsonReport {
            scan_time: report
                .scan_time
                .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
            total_packages: summary.total_packages_scanned,
            vulnerable_packages: summary.vulnerable_packages,
            total_vulnerabilities: summary.total_vulnerabilities,
            vulnerabilities: report
                .matches
                .iter()
                .map(|m| JsonVulnerability {
                    id: m.vulnerability.id.clone(),
                    package_name: m.package_name.to_string(),
                    installed_version: m.installed_version.to_string(),
                    severity: format!("{:?}", m.vulnerability.severity),
                    summary: m.vulnerability.summary.clone(),
                    description: m.vulnerability.description.clone(),
                    cvss_score: m.vulnerability.cvss_score,
                    fixed_versions: m
                        .vulnerability
                        .fixed_versions
                        .iter()
                        .map(|v| v.to_string())
                        .collect(),
                    references: m.vulnerability.references.clone(),
                    is_direct: m.is_direct,
                })
                .collect(),
            fix_suggestions: report
                .fix_analysis
                .fix_suggestions
                .iter()
                .map(|s| JsonFixSuggestion {
                    package_name: s.package_name.to_string(),
                    current_version: s.current_version.to_string(),
                    suggested_version: s.suggested_version.to_string(),
                    vulnerability_id: s.vulnerability_id.clone(),
                })
                .collect(),
            warnings: report.warnings.clone(),
        };

        Ok(serde_json::to_string_pretty(&json_report)?)
    }

    /// Generate a SARIF report using the comprehensive SarifGenerator
    fn generate_sarif_report(
        report: &AuditReport,
        project_root: Option<&Path>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let project_root = project_root.unwrap_or_else(|| Path::new("."));
        let mut generator = SarifGenerator::new(project_root);

        let sarif_json = generator.generate_report(
            &report.matches,
            &report.dependency_stats,
            &report.database_stats,
            &report.fix_analysis.fix_suggestions,
            &report.warnings,
        )?;

        Ok(sarif_json)
    }
}

// JSON report structures
#[derive(Serialize, Deserialize)]
struct JsonReport {
    scan_time: String,
    total_packages: usize,
    vulnerable_packages: usize,
    total_vulnerabilities: usize,
    vulnerabilities: Vec<JsonVulnerability>,
    fix_suggestions: Vec<JsonFixSuggestion>,
    warnings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct JsonVulnerability {
    id: String,
    package_name: String,
    installed_version: String,
    severity: String,
    summary: String,
    description: Option<String>,
    cvss_score: Option<f32>,
    fixed_versions: Vec<String>,
    references: Vec<String>,
    is_direct: bool,
}

#[derive(Serialize, Deserialize)]
struct JsonFixSuggestion {
    package_name: String,
    current_version: String,
    suggested_version: String,
    vulnerability_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulnerability::Vulnerability;
    use std::collections::HashMap;
    use std::str::FromStr;
    use uv_normalize::PackageName;
    use uv_pep440::Version;

    fn create_test_report() -> AuditReport {
        let dependency_stats = DependencyStats {
            total: 10,
            direct: 5,
            transitive: 5,
            source_counts: {
                let mut map = HashMap::new();
                map.insert("Registry".to_string(), 10);
                map
            },
        };

        let database_stats = DatabaseStats {
            total_vulnerabilities: 100,
            total_packages: 50,
            severity_counts: HashMap::new(),
            packages_with_most_vulns: vec![],
        };

        let vulnerability = Vulnerability {
            id: "GHSA-test-1234".to_string(),
            summary: "Test vulnerability".to_string(),
            description: Some("A test vulnerability for unit testing".to_string()),
            severity: Severity::High,
            affected_versions: vec![],
            fixed_versions: vec![Version::from_str("1.5.0").unwrap()],
            references: vec!["https://example.com/advisory".to_string()],
            cvss_score: Some(7.5),
            published: None,
            modified: None,
        };

        let matches = vec![VulnerabilityMatch {
            package_name: PackageName::from_str("test-package").unwrap(),
            installed_version: Version::from_str("1.0.0").unwrap(),
            vulnerability,
            is_direct: true,
        }];

        let fix_analysis = FixAnalysis {
            total_matches: 1,
            fixable: 1,
            unfixable: 0,
            fix_suggestions: vec![],
        };

        AuditReport::new(
            dependency_stats,
            database_stats,
            matches,
            fix_analysis,
            vec!["Test warning".to_string()],
        )
    }

    #[test]
    fn test_audit_summary() {
        let report = create_test_report();
        let summary = report.summary();

        assert_eq!(summary.total_packages_scanned, 10);
        assert_eq!(summary.vulnerable_packages, 1);
        assert_eq!(summary.total_vulnerabilities, 1);
        assert_eq!(summary.fixable_vulnerabilities, 1);
        assert_eq!(summary.unfixable_vulnerabilities, 0);
    }

    #[test]
    fn test_human_report_generation() {
        let report = create_test_report();
        let output = ReportGenerator::generate_human_report(&report).unwrap();

        assert!(output.contains("uv audit report"));
        assert!(output.contains("Scan Summary"));
        assert!(output.contains("Scanned: 10 packages"));
        assert!(output.contains("GHSA-test-1234"));
        assert!(output.contains("test-package"));
    }

    #[test]
    fn test_json_report_generation() {
        let report = create_test_report();
        let output = ReportGenerator::generate_json_report(&report).unwrap();

        let json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["total_packages"], 10);
        assert_eq!(json["vulnerable_packages"], 1);
        assert_eq!(json["total_vulnerabilities"], 1);
        assert_eq!(json["vulnerabilities"][0]["id"], "GHSA-test-1234");
    }

    #[test]
    fn test_sarif_report_generation() {
        let report = create_test_report();
        let output =
            ReportGenerator::generate_sarif_report(&report, Some(std::path::Path::new(".")))
                .unwrap();

        let sarif: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["tool"]["driver"]["name"], "uv audit");
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "GHSA-test-1234");
    }

    #[test]
    fn test_empty_report() {
        let dependency_stats = DependencyStats {
            total: 5,
            direct: 5,
            transitive: 0,
            source_counts: HashMap::new(),
        };

        let database_stats = DatabaseStats {
            total_vulnerabilities: 0,
            total_packages: 0,
            severity_counts: HashMap::new(),
            packages_with_most_vulns: vec![],
        };

        let fix_analysis = FixAnalysis {
            total_matches: 0,
            fixable: 0,
            unfixable: 0,
            fix_suggestions: vec![],
        };

        let report = AuditReport::new(
            dependency_stats,
            database_stats,
            vec![],
            fix_analysis,
            vec![],
        );

        assert!(!report.has_vulnerabilities());

        let output = ReportGenerator::generate_human_report(&report).unwrap();
        assert!(output.contains("No vulnerabilities found"));
    }
}
