use crate::Result;
use crate::scanner::{DependencySource, ScannedDependency};
use crate::vulnerability::{
    Severity, VersionRange, Vulnerability, VulnerabilityDatabase, VulnerabilityMatch,
};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};
use uv_cli::SeverityLevel;
use uv_normalize::PackageName;
use uv_pep440::Version;

/// Configuration for vulnerability matching
#[derive(Debug, Clone)]
pub struct MatcherConfig {
    /// Minimum severity level to include
    pub min_severity: SeverityLevel,
    /// Vulnerability IDs to ignore
    pub ignore_ids: HashSet<String>,
    /// Only check direct dependencies
    pub direct_only: bool,
}

impl MatcherConfig {
    /// Create a new matcher configuration
    pub fn new(min_severity: SeverityLevel, ignore_ids: Vec<String>, direct_only: bool) -> Self {
        Self {
            min_severity,
            ignore_ids: ignore_ids.into_iter().collect(),
            direct_only,
        }
    }

    /// Check if a severity level meets the minimum threshold
    pub fn severity_matches(&self, severity: Severity) -> bool {
        match self.min_severity {
            SeverityLevel::Low => true,
            SeverityLevel::Medium => severity >= Severity::Medium,
            SeverityLevel::High => severity >= Severity::High,
            SeverityLevel::Critical => severity >= Severity::Critical,
        }
    }

    /// Check if a vulnerability ID should be ignored
    pub fn should_ignore(&self, vulnerability_id: &str) -> bool {
        self.ignore_ids.contains(vulnerability_id)
    }
}

/// Engine for matching dependencies against vulnerability database
pub struct VulnerabilityMatcher {
    database: VulnerabilityDatabase,
    config: MatcherConfig,
}

impl VulnerabilityMatcher {
    /// Create a new vulnerability matcher
    pub fn new(database: VulnerabilityDatabase, config: MatcherConfig) -> Self {
        Self { database, config }
    }

    /// Find all vulnerabilities affecting the given dependencies
    pub fn find_vulnerabilities(
        &self,
        dependencies: &[ScannedDependency],
    ) -> Result<Vec<VulnerabilityMatch>> {
        info!(
            "Matching {} dependencies against vulnerability database",
            dependencies.len()
        );

        let mut matches = Vec::new();
        let mut stats = MatchingStats::new();

        for dependency in dependencies {
            // Apply direct-only filter
            if self.config.direct_only && !dependency.is_direct {
                continue;
            }

            stats.total_checked += 1;

            // Only check registry dependencies for now
            // TODO: Add support for git/path dependencies
            if !matches!(dependency.source, DependencySource::Registry) {
                stats.non_registry_skipped += 1;
                continue;
            }

            let dependency_matches = self.find_vulnerabilities_for_dependency(dependency);
            stats.vulnerable_packages += usize::from(!dependency_matches.is_empty());
            stats.total_vulnerabilities += dependency_matches.len();

            matches.extend(dependency_matches);
        }

        info!(
            "Vulnerability matching complete: {} vulnerabilities found in {} packages",
            stats.total_vulnerabilities, stats.vulnerable_packages
        );

        debug!("Matching statistics: {:?}", stats);

        Ok(matches)
    }

    /// Find vulnerabilities for a specific dependency
    fn find_vulnerabilities_for_dependency(
        &self,
        dependency: &ScannedDependency,
    ) -> Vec<VulnerabilityMatch> {
        let mut matches = Vec::new();

        // Get potential vulnerabilities for this package
        if let Some(vulnerability_indices) = self.database.package_index.get(&dependency.name) {
            for &vuln_index in vulnerability_indices {
                if let Some(vulnerability) = self.database.advisories.get(vuln_index) {
                    // Check if this vulnerability should be ignored
                    if self.config.should_ignore(&vulnerability.id) {
                        continue;
                    }

                    // Check severity threshold
                    if !self.config.severity_matches(vulnerability.severity) {
                        continue;
                    }

                    // Check if the installed version is affected
                    if Self::is_version_affected(&dependency.version, vulnerability) {
                        matches.push(VulnerabilityMatch {
                            package_name: dependency.name.clone(),
                            installed_version: dependency.version.clone(),
                            vulnerability: vulnerability.clone(),
                            is_direct: dependency.is_direct,
                        });
                    }
                }
            }
        }

        matches
    }

    /// Check if a specific version is affected by a vulnerability
    fn is_version_affected(version: &Version, vulnerability: &Vulnerability) -> bool {
        // Check each affected version range
        for range in &vulnerability.affected_versions {
            if Self::version_in_range(version, range) {
                return true;
            }
        }

        false
    }

    /// Check if a version falls within a vulnerability range
    fn version_in_range(version: &Version, range: &VersionRange) -> bool {
        // Check minimum version constraint
        if let Some(min_version) = &range.min {
            if version < min_version {
                return false;
            }
        }

        // Check maximum version constraint (exclusive)
        if let Some(max_version) = &range.max {
            if version >= max_version {
                return false;
            }
        }

        true
    }

    /// Get vulnerability statistics
    pub fn get_database_stats(&self) -> DatabaseStats {
        let mut severity_counts = HashMap::new();
        let mut package_counts = HashMap::new();

        for vulnerability in &self.database.advisories {
            *severity_counts.entry(vulnerability.severity).or_insert(0) += 1;
        }

        for (package_name, vuln_indices) in &self.database.package_index {
            package_counts.insert(package_name.clone(), vuln_indices.len());
        }

        DatabaseStats {
            total_vulnerabilities: self.database.advisories.len(),
            total_packages: self.database.package_index.len(),
            severity_counts,
            packages_with_most_vulns: Self::get_top_vulnerable_packages(&package_counts, 10),
        }
    }

    /// Get packages with the most vulnerabilities
    fn get_top_vulnerable_packages(
        package_counts: &HashMap<PackageName, usize>,
        limit: usize,
    ) -> Vec<(PackageName, usize)> {
        let mut packages: Vec<_> = package_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        packages.sort_by(|a, b| b.1.cmp(&a.1));
        packages.truncate(limit);
        packages
    }

    /// Filter matches based on additional criteria
    pub fn filter_matches(&self, matches: Vec<VulnerabilityMatch>) -> Vec<VulnerabilityMatch> {
        let mut filtered = Vec::new();
        let mut seen_vulnerability_ids = HashSet::new();

        for m in matches {
            // Deduplicate by vulnerability ID + package name
            let key = format!("{}:{}", m.vulnerability.id, m.package_name);
            if seen_vulnerability_ids.contains(&key) {
                continue;
            }
            seen_vulnerability_ids.insert(key);

            filtered.push(m);
        }

        // Sort by severity (highest first), then by package name
        filtered.sort_by(|a, b| {
            b.vulnerability
                .severity
                .cmp(&a.vulnerability.severity)
                .then_with(|| a.package_name.cmp(&b.package_name))
        });

        filtered
    }

    /// Check if any vulnerabilities have fixes available
    pub fn analyze_fixes(&self, matches: &[VulnerabilityMatch]) -> FixAnalysis {
        let mut analysis = FixAnalysis {
            total_matches: matches.len(),
            fixable: 0,
            unfixable: 0,
            fix_suggestions: Vec::new(),
        };

        for m in matches {
            if !m.vulnerability.fixed_versions.is_empty() {
                analysis.fixable += 1;

                // Find the best fix version (usually the minimum fixed version)
                if let Some(fix_version) = m.vulnerability.fixed_versions.first() {
                    analysis.fix_suggestions.push(FixSuggestion {
                        package_name: m.package_name.clone(),
                        current_version: m.installed_version.clone(),
                        suggested_version: fix_version.clone(),
                        vulnerability_id: m.vulnerability.id.clone(),
                    });
                }
            } else {
                analysis.unfixable += 1;
            }
        }

        analysis
    }
}

/// Statistics about vulnerability matching
#[derive(Debug)]
struct MatchingStats {
    total_checked: usize,
    vulnerable_packages: usize,
    total_vulnerabilities: usize,
    non_registry_skipped: usize,
}

impl MatchingStats {
    fn new() -> Self {
        Self {
            total_checked: 0,
            vulnerable_packages: 0,
            total_vulnerabilities: 0,
            non_registry_skipped: 0,
        }
    }
}

/// Statistics about the vulnerability database
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_vulnerabilities: usize,
    pub total_packages: usize,
    pub severity_counts: HashMap<Severity, usize>,
    pub packages_with_most_vulns: Vec<(PackageName, usize)>,
}

impl std::fmt::Display for DatabaseStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Database: {} vulnerabilities across {} packages",
            self.total_vulnerabilities, self.total_packages
        )?;

        if !self.severity_counts.is_empty() {
            write!(f, " (")?;
            let mut first = true;
            for (severity, count) in &self.severity_counts {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "{severity:?}: {count}")?;
                first = false;
            }
            write!(f, ")")?;
        }

        Ok(())
    }
}

/// Analysis of available fixes for vulnerabilities
#[derive(Debug, Clone)]
pub struct FixAnalysis {
    pub total_matches: usize,
    pub fixable: usize,
    pub unfixable: usize,
    pub fix_suggestions: Vec<FixSuggestion>,
}

/// A suggestion for fixing a vulnerability
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    pub package_name: PackageName,
    pub current_version: Version,
    pub suggested_version: Version,
    pub vulnerability_id: String,
}

impl std::fmt::Display for FixSuggestion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} → {} (fixes {})",
            self.package_name, self.current_version, self.suggested_version, self.vulnerability_id
        )
    }
}

impl std::fmt::Display for FixAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Fix analysis: {} fixable, {} unfixable out of {} total vulnerabilities",
            self.fixable, self.unfixable, self.total_matches
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn create_test_vulnerability() -> Vulnerability {
        Vulnerability {
            id: "TEST-2023-001".to_string(),
            summary: "Test vulnerability".to_string(),
            description: Some("A test vulnerability for unit tests".to_string()),
            severity: Severity::High,
            affected_versions: vec![VersionRange {
                min: Some(Version::from_str("1.0.0").unwrap()),
                max: Some(Version::from_str("1.5.0").unwrap()),
                constraint: ">=1.0.0,<1.5.0".to_string(),
            }],
            fixed_versions: vec![Version::from_str("1.5.0").unwrap()],
            references: vec!["https://example.com/advisory".to_string()],
            cvss_score: Some(7.5),
            published: None,
            modified: None,
            source: Some("test".to_string()),
        }
    }

    fn create_test_database() -> VulnerabilityDatabase {
        let vulnerability = create_test_vulnerability();
        let mut package_index = HashMap::new();
        package_index.insert(
            PackageName::from_str("vulnerable-package").unwrap(),
            vec![0],
        );

        VulnerabilityDatabase {
            advisories: vec![vulnerability],
            package_index,
        }
    }

    #[test]
    fn test_matcher_config() {
        let config = MatcherConfig::new(SeverityLevel::High, vec!["IGNORE-001".to_string()], true);

        assert!(config.severity_matches(Severity::High));
        assert!(config.severity_matches(Severity::Critical));
        assert!(!config.severity_matches(Severity::Medium));
        assert!(!config.severity_matches(Severity::Low));

        assert!(config.should_ignore("IGNORE-001"));
        assert!(!config.should_ignore("INCLUDE-001"));
    }

    #[test]
    fn test_vulnerability_matching() {
        let database = create_test_database();
        let config = MatcherConfig::new(SeverityLevel::Low, vec![], false);
        let matcher = VulnerabilityMatcher::new(database, config);

        let dependencies = vec![
            ScannedDependency {
                name: PackageName::from_str("vulnerable-package").unwrap(),
                version: Version::from_str("1.2.0").unwrap(), // In vulnerable range
                is_direct: true,
                source: DependencySource::Registry,
                path: None,
            },
            ScannedDependency {
                name: PackageName::from_str("safe-package").unwrap(),
                version: Version::from_str("2.0.0").unwrap(),
                is_direct: true,
                source: DependencySource::Registry,
                path: None,
            },
        ];

        let matches = matcher.find_vulnerabilities(&dependencies).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].package_name.to_string(), "vulnerable-package");
        assert_eq!(matches[0].vulnerability.id, "TEST-2023-001");
    }

    #[test]
    fn test_version_range_matching() {
        let database = create_test_database();
        let config = MatcherConfig::new(SeverityLevel::Low, vec![], false);
        let matcher = VulnerabilityMatcher::new(database, config);

        let vulnerability = &matcher.database.advisories[0];

        // Test vulnerable version
        assert!(VulnerabilityMatcher::is_version_affected(
            &Version::from_str("1.2.0").unwrap(),
            vulnerability
        ));

        // Test safe version (before range)
        assert!(!VulnerabilityMatcher::is_version_affected(
            &Version::from_str("0.9.0").unwrap(),
            vulnerability
        ));

        // Test safe version (after fix)
        assert!(!VulnerabilityMatcher::is_version_affected(
            &Version::from_str("1.5.0").unwrap(),
            vulnerability
        ));
    }

    #[test]
    fn test_fix_analysis() {
        let database = create_test_database();
        let config = MatcherConfig::new(SeverityLevel::Low, vec![], false);
        let matcher = VulnerabilityMatcher::new(database, config);

        let matches = vec![VulnerabilityMatch {
            package_name: PackageName::from_str("vulnerable-package").unwrap(),
            installed_version: Version::from_str("1.2.0").unwrap(),
            vulnerability: create_test_vulnerability(),
            is_direct: true,
        }];

        let analysis = matcher.analyze_fixes(&matches);
        assert_eq!(analysis.total_matches, 1);
        assert_eq!(analysis.fixable, 1);
        assert_eq!(analysis.unfixable, 0);
        assert_eq!(analysis.fix_suggestions.len(), 1);
    }
}
