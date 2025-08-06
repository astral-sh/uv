//! SARIF (Static Analysis Results Interchange Format) report generation for uv audit
//!
//! This module implements comprehensive SARIF 2.1.0 compliant output for security
//! vulnerability reports, optimized for GitHub Security and GitLab Security integration.

use crate::matcher::{DatabaseStats, FixSuggestion};
use crate::scanner::DependencyStats;
use crate::vulnerability::{Severity, VulnerabilityMatch};
use crate::{AuditError, Result};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use uv_normalize::PackageName;

/// Generator for SARIF 2.1.0 compliant security reports
pub struct SarifGenerator {
    /// Project root directory for relative path resolution
    project_root: PathBuf,
    /// Cache for parsed file locations
    location_cache: HashMap<String, Vec<LocationInfo>>,
    /// Rules (vulnerability definitions) generated for this report
    rules: Vec<Value>,
}

/// Information about a location in a source file
#[derive(Debug, Clone)]
struct LocationInfo {
    /// File path relative to project root
    file_path: String,
    /// Line number (1-based)
    line: Option<u32>,
    /// Column number (1-based)
    column: Option<u32>,
    /// Context information (e.g., dependency declaration)
    context: Option<String>,
}

impl SarifGenerator {
    /// Create a new SARIF generator
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
            location_cache: HashMap::new(),
            rules: Vec::new(),
        }
    }

    /// Generate a complete SARIF report
    pub fn generate_report(
        &mut self,
        matches: &[VulnerabilityMatch],
        dependency_stats: &DependencyStats,
        database_stats: &DatabaseStats,
        fix_suggestions: &[FixSuggestion],
        warnings: &[String],
    ) -> Result<String> {
        info!(
            "Generating SARIF 2.1.0 report with {} vulnerabilities",
            matches.len()
        );

        // Pre-process locations for better mapping
        self.preprocess_locations(matches);

        // Generate rules for each unique vulnerability
        self.generate_rules(matches);

        // Create SARIF results
        let results = self.create_sarif_results(matches, fix_suggestions);

        // Build the complete SARIF document
        let sarif = self.build_sarif_document(&results, dependency_stats, database_stats, warnings);

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&sarif).map_err(AuditError::Json)?;

        info!("SARIF report generated successfully");
        Ok(json)
    }

    /// Pre-process file locations for better mapping
    fn preprocess_locations(&mut self, matches: &[VulnerabilityMatch]) {
        let mut packages_to_locate: HashSet<PackageName> = HashSet::new();

        for m in matches {
            packages_to_locate.insert(m.package_name.clone());
        }

        debug!(
            "Pre-processing locations for {} packages",
            packages_to_locate.len()
        );

        // Parse pyproject.toml for direct dependencies
        if let Ok(locations) = self.parse_pyproject_locations(&packages_to_locate) {
            for (package, locs) in locations {
                self.location_cache
                    .insert(format!("pyproject.toml:{package}"), locs);
            }
        }

        // Parse uv.lock for all dependencies
        if let Ok(locations) = self.parse_lock_locations(&packages_to_locate) {
            for (package, locs) in locations {
                self.location_cache
                    .insert(format!("uv.lock:{package}"), locs);
            }
        }
    }

    /// Parse pyproject.toml to find dependency locations
    fn parse_pyproject_locations(
        &self,
        packages: &HashSet<PackageName>,
    ) -> Result<HashMap<PackageName, Vec<LocationInfo>>> {
        let pyproject_path = self.project_root.join("pyproject.toml");
        if !pyproject_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs_err::read_to_string(&pyproject_path).map_err(AuditError::Cache)?;

        let mut locations = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();

        // Simple parser to find dependency declarations
        let mut in_dependencies = false;
        let mut in_dev_dependencies = false;
        let mut current_section = None;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = u32::try_from(line_idx + 1).unwrap_or(0);
            let trimmed = line.trim();

            // Track TOML sections
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                // Reset flags
                in_dependencies = false;
                in_dev_dependencies = false;

                // Check for dependencies sections
                if trimmed == "[project]" {
                    // We're in project section, but not yet in dependencies
                    current_section = Some(trimmed.to_string());
                    continue;
                }

                current_section = Some(trimmed.to_string());
                continue;
            }

            // Check for array-style dependencies in project section
            if current_section.as_deref() == Some("[project]") && trimmed == "dependencies = [" {
                in_dependencies = true;
                continue;
            }

            // Check for end of array
            if in_dependencies && trimmed == "]" {
                in_dependencies = false;
                continue;
            }

            // Look for package declarations
            if (in_dependencies || in_dev_dependencies)
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
            {
                for package in packages {
                    let package_str = package.to_string();

                    // Match various dependency declaration formats
                    if trimmed.contains(&package_str) {
                        // Try to find the exact column position
                        if let Some(col) = line.find(&package_str) {
                            let location = LocationInfo {
                                file_path: "pyproject.toml".to_string(),
                                line: Some(line_num),
                                column: Some(u32::try_from(col + 1).unwrap_or(0)),
                                context: Some(format!(
                                    "Dependency declaration in {}",
                                    current_section.as_deref().unwrap_or("unknown section")
                                )),
                            };

                            locations
                                .entry(package.clone())
                                .or_insert_with(Vec::new)
                                .push(location);
                        }
                    }
                }
            }
        }

        debug!(
            "Found {} package locations in pyproject.toml",
            locations.len()
        );
        Ok(locations)
    }

    /// Parse uv.lock to find dependency locations
    fn parse_lock_locations(
        &self,
        packages: &HashSet<PackageName>,
    ) -> Result<HashMap<PackageName, Vec<LocationInfo>>> {
        let lock_path = self.project_root.join("uv.lock");
        if !lock_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs_err::read_to_string(&lock_path).map_err(AuditError::Cache)?;

        let mut locations = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();

        // Parse the lock file format to find package declarations

        for (line_idx, line) in lines.iter().enumerate() {
            let line_num = u32::try_from(line_idx + 1).unwrap_or(0);
            let trimmed = line.trim();

            // Look for package declarations in lock file
            if let Some(name_start) = trimmed.find("name = \"") {
                if let Some(name_end) = trimmed[name_start + 8..].find('"') {
                    let package_name_str = &trimmed[name_start + 8..name_start + 8 + name_end];

                    // Check if this is one of our target packages
                    for package in packages {
                        if package.to_string() == package_name_str {
                            let location = LocationInfo {
                                file_path: "uv.lock".to_string(),
                                line: Some(line_num),
                                column: Some(u32::try_from(name_start + 8 + 1).unwrap_or(0)),
                                context: Some("Package declaration in lock file".to_string()),
                            };

                            locations
                                .entry(package.clone())
                                .or_insert_with(Vec::new)
                                .push(location);
                        }
                    }
                }
            }
        }

        debug!("Found {} package locations in uv.lock", locations.len());
        Ok(locations)
    }

    /// Generate rule definitions for vulnerabilities
    fn generate_rules(&mut self, matches: &[VulnerabilityMatch]) {
        let mut seen_rules = HashSet::new();

        for m in matches {
            let rule_id = &m.vulnerability.id;

            if seen_rules.contains(rule_id) {
                continue;
            }
            seen_rules.insert(rule_id.clone());

            // Create rule with comprehensive metadata
            let mut rule = json!({
                "id": rule_id,
                "name": format!("Security vulnerability {rule_id}"),
                "shortDescription": {
                    "text": m.vulnerability.summary
                },
                "defaultConfiguration": {
                    "level": Self::severity_to_sarif_level(m.vulnerability.severity)
                },
                "properties": {
                    "security-severity": Self::get_security_severity_score(m.vulnerability.severity),
                    "vulnerability_id": m.vulnerability.id,
                    "severity": format!("{:?}", m.vulnerability.severity),
                    "tags": ["security", "vulnerability", format!("{:?}", m.vulnerability.severity).to_lowercase()]
                }
            });

            // Add full description if available
            if let Some(description) = &m.vulnerability.description {
                rule["fullDescription"] = json!({
                    "text": description
                });
            }

            // Add help message
            rule["help"] = json!({
                "text": Self::create_help_text(&m.vulnerability),
                "markdown": Self::create_help_text(&m.vulnerability)
            });

            // Add help URI if available
            if let Some(primary_ref) = Self::extract_primary_reference(&m.vulnerability.references)
            {
                rule["helpUri"] = json!(primary_ref);
            }

            // Add CVSS score if available
            if let Some(cvss) = m.vulnerability.cvss_score {
                rule["properties"]["cvss_score"] = json!(cvss);
            }

            // Add timestamps if available
            if let Some(published) = &m.vulnerability.published {
                rule["properties"]["published_date"] = json!(published.to_string());
            }
            if let Some(modified) = &m.vulnerability.modified {
                rule["properties"]["modified_date"] = json!(modified.to_string());
            }

            self.rules.push(rule);
        }

        debug!("Generated {} SARIF rules", self.rules.len());
    }

    /// Create help text for a vulnerability
    fn create_help_text(vulnerability: &crate::vulnerability::Vulnerability) -> String {
        use std::fmt::Write;
        let mut help_text = format!("## {}\n\n", vulnerability.summary);

        if let Some(description) = &vulnerability.description {
            write!(help_text, "**Description:** {description}\n\n").unwrap();
        }

        if let Some(cvss) = vulnerability.cvss_score {
            write!(help_text, "**CVSS Score:** {cvss:.1}\n\n").unwrap();
        }

        if !vulnerability.fixed_versions.is_empty() {
            help_text.push_str("**Fixed Versions:**\n");
            for version in &vulnerability.fixed_versions {
                writeln!(help_text, "- {version}").unwrap();
            }
            help_text.push('\n');
        }

        if !vulnerability.references.is_empty() {
            help_text.push_str("**References:**\n");
            for reference in &vulnerability.references {
                writeln!(help_text, "- {reference}").unwrap();
            }
        }

        help_text
    }

    /// Extract primary reference URL
    fn extract_primary_reference(references: &[String]) -> Option<String> {
        // Prefer GHSA or CVE URLs, then any HTTPS URL
        references
            .iter()
            .find(|r| r.contains("github.com/advisories/") || r.contains("cve.mitre.org"))
            .or_else(|| references.iter().find(|r| r.starts_with("https://")))
            .cloned()
    }

    /// Convert severity to SARIF level
    fn severity_to_sarif_level(severity: Severity) -> &'static str {
        match severity {
            Severity::Critical => "error",
            Severity::High => "error",
            Severity::Medium => "warning",
            Severity::Low => "note",
        }
    }

    /// Get security severity score for GitHub integration
    fn get_security_severity_score(severity: Severity) -> &'static str {
        match severity {
            Severity::Critical => "10.0",
            Severity::High => "8.0",
            Severity::Medium => "5.0",
            Severity::Low => "2.0",
        }
    }

    /// Create SARIF results from vulnerability matches
    fn create_sarif_results(
        &self,
        matches: &[VulnerabilityMatch],
        fix_suggestions: &[FixSuggestion],
    ) -> Vec<Value> {
        let mut results = Vec::new();

        // Create a map of package names to fix suggestions for quick lookup
        let fix_map: HashMap<&PackageName, &FixSuggestion> = fix_suggestions
            .iter()
            .map(|fs| (&fs.package_name, fs))
            .collect();

        for m in matches {
            let mut result = json!({
                "ruleId": m.vulnerability.id,
                "ruleIndex": self.find_rule_index(&m.vulnerability.id),
                "message": {
                    "text": format!(
                        "Package '{}' version {} has vulnerability {}: {}",
                        m.package_name,
                        m.installed_version,
                        m.vulnerability.id,
                        m.vulnerability.summary
                    )
                },
                "level": Self::severity_to_sarif_level(m.vulnerability.severity),
                "locations": self.create_locations_for_match(m),
                "properties": {
                    "package_name": m.package_name.to_string(),
                    "installed_version": m.installed_version.to_string(),
                    "is_direct_dependency": m.is_direct,
                    "vulnerability_severity": format!("{:?}", m.vulnerability.severity)
                }
            });

            // Add CVSS score if available
            if let Some(cvss) = m.vulnerability.cvss_score {
                result["properties"]["cvss_score"] = json!(cvss);
            }

            // Add fixed versions if available
            if !m.vulnerability.fixed_versions.is_empty() {
                let fixed_versions: Vec<String> = m
                    .vulnerability
                    .fixed_versions
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                result["properties"]["fixed_versions"] = json!(fixed_versions);
            }

            // Add fix information if available
            if let Some(fix_suggestion) = fix_map.get(&m.package_name) {
                result["fixes"] = json!([{
                    "description": {
                        "text": format!(
                            "Update {} from {} to {} to fix vulnerability {}",
                            fix_suggestion.package_name,
                            fix_suggestion.current_version,
                            fix_suggestion.suggested_version,
                            fix_suggestion.vulnerability_id
                        )
                    }
                }]);
            }

            results.push(result);
        }

        debug!("Created {} SARIF results", results.len());
        results
    }

    /// Find rule index by ID
    fn find_rule_index(&self, rule_id: &str) -> Option<usize> {
        self.rules.iter().position(|r| r["id"] == rule_id)
    }

    /// Create locations for a vulnerability match
    fn create_locations_for_match(&self, m: &VulnerabilityMatch) -> Vec<Value> {
        let mut locations = Vec::new();

        // Try to find specific locations from cache
        let package_name = &m.package_name;

        // Check pyproject.toml first (for direct dependencies)
        if let Some(pyproject_locations) = self
            .location_cache
            .get(&format!("pyproject.toml:{package_name}"))
        {
            for loc_info in pyproject_locations {
                locations.push(Self::create_location_from_info(loc_info, m));
            }
        }

        // Add uv.lock location (for all dependencies)
        if let Some(lock_locations) = self.location_cache.get(&format!("uv.lock:{package_name}")) {
            for loc_info in lock_locations {
                locations.push(Self::create_location_from_info(loc_info, m));
            }
        }

        // If no specific locations found, create generic ones
        if locations.is_empty() {
            // Create a generic location pointing to the appropriate file
            let file_path = if m.is_direct {
                "pyproject.toml"
            } else {
                "uv.lock"
            };

            locations.push(json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": file_path
                    }
                },
                "logicalLocations": [{
                    "name": package_name.to_string(),
                    "kind": "package"
                }]
            }));
        }

        locations
    }

    /// Create location from location info
    fn create_location_from_info(loc_info: &LocationInfo, m: &VulnerabilityMatch) -> Value {
        let mut location = json!({
            "physicalLocation": {
                "artifactLocation": {
                    "uri": loc_info.file_path
                }
            },
            "logicalLocations": [{
                "name": m.package_name.to_string(),
                "kind": "package"
            }]
        });

        // Add region if we have line/column information
        if let (Some(line), Some(column)) = (loc_info.line, loc_info.column) {
            location["physicalLocation"]["region"] = json!({
                "startLine": line,
                "startColumn": column,
                "endLine": line,
                "endColumn": column + u32::try_from(m.package_name.to_string().len()).unwrap_or(0)
            });
        }

        // Add context message if available
        if let Some(context) = &loc_info.context {
            location["message"] = json!({
                "text": context
            });
        }

        location
    }

    /// Build complete SARIF document
    fn build_sarif_document(
        &self,
        results: &[Value],
        dependency_stats: &DependencyStats,
        database_stats: &DatabaseStats,
        warnings: &[String],
    ) -> Value {
        let sarif = json!({
            "version": "2.1.0",
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "uv audit",
                        "version": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://docs.astral.sh/uv/reference/cli/#uv-audit",
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "shortDescription": {
                            "text": "Security vulnerability scanner for Python dependencies"
                        },
                        "fullDescription": {
                            "text": "uv audit scans Python project dependencies for known security vulnerabilities using the PyPA Advisory Database"
                        },
                        "rules": self.rules,
                        "properties": {
                            "scan_stats": {
                                "total_packages": dependency_stats.total,
                                "direct_packages": dependency_stats.direct,
                                "transitive_packages": dependency_stats.transitive,
                                "database_vulnerabilities": database_stats.total_vulnerabilities,
                                "database_packages": database_stats.total_packages
                            }
                        }
                    }
                },
                "results": results,
                "invocations": [{
                    "commandLine": "uv audit",
                    "startTimeUtc": jiff::Timestamp::now().to_string(),
                    "executionSuccessful": true,
                    "exitCode": i32::from(!results.is_empty())
                }],
                "properties": {
                    "project_root": self.project_root.to_string_lossy(),
                    "dependency_sources": dependency_stats.source_counts,
                    "warnings": warnings
                }
            }]
        });

        sarif
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulnerability::Vulnerability;
    use std::str::FromStr;
    use tempfile::TempDir;
    use uv_pep440::Version;

    fn create_test_vulnerability() -> Vulnerability {
        Vulnerability {
            id: "GHSA-test-1234".to_string(),
            summary: "Test SQL injection vulnerability".to_string(),
            description: Some("A test SQL injection vulnerability in the test package".to_string()),
            severity: Severity::High,
            affected_versions: vec![],
            fixed_versions: vec![Version::from_str("2.0.0").unwrap()],
            references: vec![
                "https://github.com/advisories/GHSA-test-1234".to_string(),
                "https://nvd.nist.gov/vuln/detail/CVE-2023-12345".to_string(),
            ],
            cvss_score: Some(8.5),
            published: None,
            modified: None,
            source: Some("test".to_string()),
        }
    }

    fn create_test_match() -> VulnerabilityMatch {
        VulnerabilityMatch {
            package_name: PackageName::from_str("test-package").unwrap(),
            installed_version: Version::from_str("1.5.0").unwrap(),
            vulnerability: create_test_vulnerability(),
            is_direct: true,
        }
    }

    #[test]
    fn test_sarif_generator_creation() {
        let temp_dir = TempDir::new().unwrap();
        let generator = SarifGenerator::new(temp_dir.path());

        assert_eq!(generator.project_root, temp_dir.path());
        assert!(generator.location_cache.is_empty());
        assert!(generator.rules.is_empty());
    }

    #[test]
    fn test_severity_to_sarif_level() {
        let temp_dir = TempDir::new().unwrap();
        let _generator = SarifGenerator::new(temp_dir.path());

        assert_eq!(
            SarifGenerator::severity_to_sarif_level(Severity::Critical),
            "error"
        );
        assert_eq!(
            SarifGenerator::severity_to_sarif_level(Severity::High),
            "error"
        );
        assert_eq!(
            SarifGenerator::severity_to_sarif_level(Severity::Medium),
            "warning"
        );
        assert_eq!(
            SarifGenerator::severity_to_sarif_level(Severity::Low),
            "note"
        );
    }

    #[test]
    fn test_security_severity_score() {
        let temp_dir = TempDir::new().unwrap();
        let _generator = SarifGenerator::new(temp_dir.path());

        assert_eq!(
            SarifGenerator::get_security_severity_score(Severity::Critical),
            "10.0"
        );
        assert_eq!(
            SarifGenerator::get_security_severity_score(Severity::High),
            "8.0"
        );
        assert_eq!(
            SarifGenerator::get_security_severity_score(Severity::Medium),
            "5.0"
        );
        assert_eq!(
            SarifGenerator::get_security_severity_score(Severity::Low),
            "2.0"
        );
    }

    #[test]
    fn test_rule_generation() {
        let temp_dir = TempDir::new().unwrap();
        let mut generator = SarifGenerator::new(temp_dir.path());

        let matches = vec![create_test_match()];
        generator.generate_rules(&matches);

        assert_eq!(generator.rules.len(), 1);
        assert_eq!(generator.rules[0]["id"], "GHSA-test-1234");
        assert!(generator.rules[0]["shortDescription"].is_object());
        assert!(generator.rules[0]["help"].is_object());
    }

    #[test]
    fn test_extract_primary_reference() {
        let temp_dir = TempDir::new().unwrap();
        let _generator = SarifGenerator::new(temp_dir.path());

        let references = vec![
            "https://example.com/advisory".to_string(),
            "https://github.com/advisories/GHSA-1234".to_string(),
            "https://nvd.nist.gov/vuln/detail/CVE-2023-1234".to_string(),
        ];

        let primary = SarifGenerator::extract_primary_reference(&references);
        assert_eq!(
            primary,
            Some("https://github.com/advisories/GHSA-1234".to_string())
        );
    }

    #[test]
    fn test_full_sarif_generation() {
        let temp_dir = TempDir::new().unwrap();
        let mut generator = SarifGenerator::new(temp_dir.path());

        let matches = vec![create_test_match()];
        let dependency_stats = DependencyStats {
            total: 5,
            direct: 3,
            transitive: 2,
            source_counts: std::collections::HashMap::new(),
        };
        let database_stats = DatabaseStats {
            total_vulnerabilities: 100,
            total_packages: 50,
            severity_counts: std::collections::HashMap::new(),
            packages_with_most_vulns: vec![],
        };
        let fix_suggestions = vec![];
        let warnings = vec!["Test warning".to_string()];

        let sarif_json = generator
            .generate_report(
                &matches,
                &dependency_stats,
                &database_stats,
                &fix_suggestions,
                &warnings,
            )
            .unwrap();

        // Verify it's valid JSON
        let sarif: serde_json::Value = serde_json::from_str(&sarif_json).unwrap();

        // Check SARIF structure
        assert_eq!(sarif["version"], "2.1.0");
        assert!(sarif["runs"].is_array());
        assert_eq!(sarif["runs"][0]["tool"]["driver"]["name"], "uv audit");
        assert!(sarif["runs"][0]["results"].is_array());
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "GHSA-test-1234");
    }

    #[test]
    fn test_location_parsing_with_pyproject() {
        let temp_dir = TempDir::new().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");

        // Create a test pyproject.toml with proper dependencies section
        fs_err::write(
            &pyproject_path,
            r#"[project]
name = "test-project"
dependencies = [
    "test-package>=1.0.0",
    "other-package==2.0.0"
]

[project.optional-dependencies]
dev = [
    "pytest>=6.0.0"
]
"#,
        )
        .unwrap();

        let generator = SarifGenerator::new(temp_dir.path());
        let mut packages = HashSet::new();
        packages.insert(PackageName::from_str("test-package").unwrap());

        let locations = generator.parse_pyproject_locations(&packages).unwrap();

        // Verify we found the location
        assert!(!locations.is_empty());

        if let Some(test_package_locations) =
            locations.get(&PackageName::from_str("test-package").unwrap())
        {
            assert!(!test_package_locations.is_empty());
            assert_eq!(test_package_locations[0].file_path, "pyproject.toml");
            assert!(test_package_locations[0].line.is_some());
        }
    }
}
