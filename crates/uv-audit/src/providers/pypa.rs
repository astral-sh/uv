use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::debug;
use uv_cache::CacheEntry;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{
    AuditCache, AuditError, Result, Severity, VersionRange, Vulnerability, VulnerabilityDatabase,
    VulnerabilityProvider,
};

/// URL for downloading the `PyPA` advisory database
const PYPA_ADVISORY_DB_URL: &str = "https://github.com/pypa/advisory-database/archive/main.zip";

/// `PyPA` Advisory Database YAML record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaAdvisory {
    /// Unique vulnerability identifier
    pub id: String,

    /// Detailed description (`PyPA` uses 'details' instead of 'summary')
    pub details: String,

    /// Affected packages and versions
    pub affected: Vec<PypaAffected>,

    /// Reference URLs
    pub references: Vec<PypaReference>,

    /// CVE aliases and other identifiers
    #[serde(default)]
    pub aliases: Vec<String>,

    /// Publication timestamp
    pub published: Option<String>,

    /// Last modification timestamp
    pub modified: Option<String>,

    /// Withdrawn status
    #[serde(default)]
    pub withdrawn: Option<String>,

    /// Summary (optional, some `PyPA` records may have it)
    pub summary: Option<String>,

    /// Additional severity information
    #[serde(default)]
    pub severity: Vec<PypaSeverity>,

    /// Database-specific fields
    #[serde(default)]
    pub database_specific: Option<serde_json::Value>,
}

/// Affected package information in `PyPA` format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaAffected {
    /// Package information
    pub package: PypaPackage,

    /// Version ranges affected
    #[serde(default)]
    pub ranges: Vec<PypaRange>,

    /// Specific versions affected
    pub versions: Option<Vec<String>>,

    /// Ecosystem-specific database information
    #[serde(default)]
    pub database_specific: Option<serde_json::Value>,

    /// Ecosystem-specific fields
    #[serde(default)]
    pub ecosystem_specific: Option<serde_json::Value>,
}

/// Package information in `PyPA` format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaPackage {
    /// Package ecosystem (e.g., "PyPI")
    pub ecosystem: String,

    /// Package name
    pub name: String,

    /// Package URL if available
    pub purl: Option<String>,
}

/// Version range specification in `PyPA` format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaRange {
    /// Range type (e.g., "ECOSYSTEM")
    #[serde(rename = "type")]
    pub range_type: String,

    /// Repository URL for version control ranges
    pub repo: Option<String>,

    /// Events defining the range (introduced, fixed, etc.)
    pub events: Vec<PypaEvent>,

    /// Database-specific information
    #[serde(default)]
    pub database_specific: Option<serde_json::Value>,
}

/// A version event in a `PyPA` range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaEvent {
    /// Version where event occurs
    pub introduced: Option<String>,

    /// Version where issue is fixed
    pub fixed: Option<String>,

    /// Last affected version
    pub last_affected: Option<String>,

    /// Version limit
    pub limit: Option<String>,
}

/// Reference information in `PyPA` format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaReference {
    /// Reference type (e.g., "ADVISORY", "FIX", "WEB", "ARTICLE")
    #[serde(rename = "type")]
    pub ref_type: String,

    /// Reference URL
    pub url: String,
}

/// Severity information in `PyPA` format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PypaSeverity {
    /// Severity type (e.g., `CVSS_V3`)
    #[serde(rename = "type")]
    pub severity_type: String,

    /// Severity score
    pub score: String,
}

/// Client for downloading `PyPA` advisory database
pub(super) struct PypaClient {
    client: Client,
}

impl PypaClient {
    /// Create a new `PyPA` client
    pub(super) fn new() -> Self {
        let client = Client::builder()
            .user_agent(format!("uv/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client }
    }

    /// Download the `PyPA` advisory database ZIP file
    pub(super) async fn download_advisory_database(&self) -> Result<Vec<u8>> {
        debug!(
            "Downloading PyPA advisory database from {}",
            PYPA_ADVISORY_DB_URL
        );

        let response = self.client.get(PYPA_ADVISORY_DB_URL).send().await?;

        if !response.status().is_success() {
            return Err(AuditError::DatabaseDownload(
                response.error_for_status().unwrap_err(),
            ));
        }

        let bytes = response.bytes().await?;
        debug!("Downloaded advisory database: {} bytes", bytes.len());

        Ok(bytes.to_vec())
    }
}

impl Default for PypaClient {
    fn default() -> Self {
        Self::new()
    }
}

/// `PyPA` Advisory Database source for vulnerability data
pub struct PypaSource {
    cache: AuditCache,
    client: PypaClient,
    no_cache: bool,
}

impl PypaSource {
    /// Create a new `PyPA` source
    pub fn new(cache: AuditCache, no_cache: bool) -> Self {
        Self {
            cache,
            client: PypaClient::new(),
            no_cache,
        }
    }

    /// Get cache entry for `PyPA` database
    fn cache_entry(&self) -> CacheEntry {
        self.cache.cache().entry(
            uv_cache::CacheBucket::VulnerabilityDatabase,
            "pypa",
            "advisories.zip",
        )
    }

    /// Download and parse `PyPA` advisory database
    async fn download_and_parse_database(&self) -> Result<Vec<PypaAdvisory>> {
        let cache_entry = self.cache_entry();

        // Check cache first unless no_cache is set
        let zip_data = if !self.no_cache && cache_entry.path().exists() {
            debug!("Using cached PyPA database");
            fs_err::read(cache_entry.path())?
        } else {
            debug!("Downloading PyPA advisory database");
            let data = self.client.download_advisory_database().await?;

            // Cache the downloaded data
            if !self.no_cache {
                fs_err::create_dir_all(cache_entry.dir())?;
                fs_err::write(cache_entry.path(), &data)?;
            }

            data
        };

        self.parse_zip_database(&zip_data).await
    }

    /// Parse `PyPA` advisory database from ZIP data
    async fn parse_zip_database(&self, zip_data: &[u8]) -> Result<Vec<PypaAdvisory>> {
        use async_zip::base::read::mem::ZipFileReader;
        use futures::AsyncReadExt;

        let reader = ZipFileReader::new(zip_data.to_vec()).await?;
        let mut advisories = Vec::new();
        let mut parsed_count = 0;
        let mut error_count = 0;

        debug!(
            "Processing ZIP file with {} entries",
            reader.file().entries().len()
        );

        for i in 0..reader.file().entries().len() {
            let entry = reader.file().entries().get(i).ok_or_else(|| {
                AuditError::DatabaseIntegrity(format!("Failed to get ZIP entry {i}"))
            })?;

            let filename = entry.filename().as_str().map_err(|e| {
                AuditError::DatabaseIntegrity(format!("Failed to read ZIP entry filename: {e}"))
            })?;

            // Skip directories and non-YAML files
            if entry.dir().map_err(AuditError::ZipExtraction)?
                || !std::path::Path::new(filename)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml"))
            {
                continue;
            }

            // Only process PyPI advisories (skip other ecosystems)
            if !filename.contains("vulns/") {
                continue;
            }

            let mut entry_reader = reader.reader_with_entry(i).await?;
            let mut content = String::new();
            entry_reader
                .read_to_string(&mut content)
                .await
                .map_err(AuditError::Cache)?;

            // Parse as PyPA YAML format
            match Self::parse_advisory(&content) {
                Ok(pypa_advisory) => {
                    // Only include PyPI advisories that have affected packages
                    let package_names = Self::extract_package_names(&pypa_advisory);
                    if !package_names.is_empty() {
                        advisories.push(pypa_advisory);
                        parsed_count += 1;
                    }
                }
                Err(e) => {
                    debug!("Failed to parse advisory from {}: {}", filename, e);
                    error_count += 1;
                }
            }
        }

        debug!(
            "Parsed {} PyPA advisories ({} errors) from ZIP file",
            parsed_count, error_count
        );

        Ok(advisories)
    }

    /// Parse a `PyPA` YAML advisory from string content
    pub fn parse_advisory(content: &str) -> Result<PypaAdvisory> {
        serde_yaml::from_str(content)
            .map_err(|e| AuditError::PypaAdvisoryParse("PyPA YAML parse error".to_string(), e))
    }

    /// Convert `PyPA` advisories to vulnerability database
    fn convert_advisories(advisories: Vec<PypaAdvisory>) -> Result<VulnerabilityDatabase> {
        let mut package_vulnerabilities: HashMap<String, Vec<Vulnerability>> = HashMap::new();

        for advisory in advisories {
            let package_names = Self::extract_package_names(&advisory);

            for package_name in package_names {
                let vulnerability = Self::to_vulnerability(&advisory, &package_name)?;
                package_vulnerabilities
                    .entry(package_name.to_string())
                    .or_default()
                    .push(vulnerability);
            }
        }

        Ok(VulnerabilityDatabase::from_package_map(
            package_vulnerabilities,
        ))
    }

    /// Convert `PyPA` advisory directly to internal Vulnerability format
    pub fn to_vulnerability(
        pypa: &PypaAdvisory,
        package_name: &PackageName,
    ) -> Result<Vulnerability> {
        // Determine severity from CVSS score or other indicators
        let severity = Self::determine_severity(pypa);

        // Extract affected version ranges for the specific package
        let affected_versions = Self::extract_version_ranges(pypa, package_name)?;

        // Extract fixed versions for the specific package
        let fixed_versions = Self::extract_fixed_versions(pypa, package_name)?;

        // Extract reference URLs
        let references = pypa.references.iter().map(|r| r.url.clone()).collect();

        // Parse timestamps
        let published = pypa
            .published
            .as_ref()
            .and_then(|s| Timestamp::from_str(s).ok());

        let modified = pypa
            .modified
            .as_ref()
            .and_then(|s| Timestamp::from_str(s).ok());

        // Get CVSS score from severity info
        let cvss_score = Self::extract_cvss_score(pypa);

        Ok(Vulnerability {
            id: pypa.id.clone(),
            summary: pypa.summary.clone().unwrap_or_else(|| {
                // Truncate details to create a summary if none exists
                let details = &pypa.details;
                if details.len() > 120 {
                    format!("{}...", &details[..117])
                } else {
                    details.clone()
                }
            }),
            description: Some(pypa.details.clone()),
            severity,
            affected_versions,
            fixed_versions,
            references,
            cvss_score,
            published,
            modified,
            source: Some("pypa-zip".to_string()),
        })
    }

    /// Determine severity level from `PyPA` advisory
    fn determine_severity(pypa: &PypaAdvisory) -> Severity {
        // Check for CVSS score first
        if let Some(cvss_score) = Self::extract_cvss_score(pypa) {
            return match cvss_score {
                score if score >= 9.0 => Severity::Critical,
                score if score >= 7.0 => Severity::High,
                score if score >= 4.0 => Severity::Medium,
                _ => Severity::Low,
            };
        }

        // Fallback to keyword-based severity detection
        let text = format!("{} {}", pypa.summary.as_deref().unwrap_or(""), pypa.details);
        let text_lower = text.to_lowercase();

        if text_lower.contains("critical")
            || text_lower.contains("rce")
            || text_lower.contains("remote code execution")
        {
            Severity::Critical
        } else if text_lower.contains("high")
            || text_lower.contains("sql injection")
            || text_lower.contains("xss")
        {
            Severity::High
        } else if text_lower.contains("medium")
            || text_lower.contains("csrf")
            || text_lower.contains("privilege escalation")
        {
            Severity::Medium
        } else {
            Severity::Low
        }
    }

    /// Extract CVSS score from `PyPA` advisory
    fn extract_cvss_score(pypa: &PypaAdvisory) -> Option<f32> {
        pypa.severity
            .iter()
            .filter_map(|sev| {
                if sev.severity_type.contains("CVSS") {
                    sev.score.parse::<f32>().ok()
                } else {
                    None
                }
            })
            .fold(None, |max_score, score| match max_score {
                None => Some(score),
                Some(max) => Some(score.max(max)),
            })
    }

    /// Extract version ranges from `PyPA` advisory for a specific package
    fn extract_version_ranges(
        pypa: &PypaAdvisory,
        package_name: &PackageName,
    ) -> Result<Vec<VersionRange>> {
        let mut ranges = Vec::new();

        for affected in &pypa.affected {
            // Check if this affected entry is for our package
            if affected.package.ecosystem != "PyPI" {
                continue;
            }

            let affected_package_name =
                PackageName::from_str(&affected.package.name).map_err(|e| {
                    AuditError::InvalidDependency(format!(
                        "Invalid package name {}: {}",
                        affected.package.name, e
                    ))
                })?;

            if affected_package_name != *package_name {
                continue;
            }

            // Add explicit versions as individual ranges
            if let Some(versions) = &affected.versions {
                for version_str in versions {
                    if let Ok(version) = Version::from_str(version_str) {
                        ranges.push(VersionRange {
                            min: Some(version.clone()),
                            max: Some(version.clone()),
                            constraint: format!("=={version}"),
                        });
                    }
                }
            }

            // Process version ranges
            for range in &affected.ranges {
                if range.range_type == "ECOSYSTEM" {
                    let version_range = Self::parse_pypa_range(range);
                    ranges.push(version_range);
                }
            }
        }

        Ok(ranges)
    }

    /// Parse a `PyPA` range into our internal format
    fn parse_pypa_range(range: &PypaRange) -> VersionRange {
        let mut min_version: Option<Version> = None;
        let mut max_version: Option<Version> = None;

        for event in &range.events {
            if let Some(introduced) = &event.introduced {
                if introduced == "0" {
                    min_version = Some(Version::new([0, 0, 0]));
                } else if let Ok(version) = Version::from_str(introduced) {
                    min_version = Some(version);
                }
            }

            if let Some(fixed) = &event.fixed {
                if let Ok(version) = Version::from_str(fixed) {
                    max_version = Some(version);
                }
            }
        }

        // Build constraint string
        let constraint = match (&min_version, &max_version) {
            (Some(min), Some(max)) => format!(">={min},<{max}"),
            (Some(min), None) => format!(">={min}"),
            (None, Some(max)) => format!("<{max}"),
            (None, None) => "*".to_string(),
        };

        VersionRange {
            min: min_version,
            max: max_version,
            constraint,
        }
    }

    /// Extract fixed versions from `PyPA` advisory for a specific package
    fn extract_fixed_versions(
        pypa: &PypaAdvisory,
        package_name: &PackageName,
    ) -> Result<Vec<Version>> {
        let mut fixed_versions = Vec::new();

        for affected in &pypa.affected {
            // Check if this affected entry is for our package
            if affected.package.ecosystem != "PyPI" {
                continue;
            }

            let affected_package_name =
                PackageName::from_str(&affected.package.name).map_err(|e| {
                    AuditError::InvalidDependency(format!(
                        "Invalid package name {}: {}",
                        affected.package.name, e
                    ))
                })?;

            if affected_package_name != *package_name {
                continue;
            }

            for range in &affected.ranges {
                for event in &range.events {
                    if let Some(fixed) = &event.fixed {
                        if let Ok(version) = Version::from_str(fixed) {
                            fixed_versions.push(version);
                        }
                    }
                }
            }
        }

        // Deduplicate and sort fixed versions
        fixed_versions.sort();
        fixed_versions.dedup();

        Ok(fixed_versions)
    }

    /// Extract package names from `PyPA` advisory
    pub fn extract_package_names(pypa: &PypaAdvisory) -> Vec<PackageName> {
        pypa.affected
            .iter()
            .filter_map(|affected| {
                if affected.package.ecosystem == "PyPI" {
                    PackageName::from_str(&affected.package.name).ok()
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if `PyPA` advisory affects a specific package
    pub fn affects_package(pypa: &PypaAdvisory, package_name: &PackageName) -> bool {
        pypa.affected.iter().any(|affected| {
            affected.package.ecosystem == "PyPI"
                && PackageName::from_str(&affected.package.name)
                    .map(|name| name == *package_name)
                    .unwrap_or(false)
        })
    }
}

#[async_trait]
impl VulnerabilityProvider for PypaSource {
    fn name(&self) -> &'static str {
        "pypa-zip"
    }

    async fn fetch_vulnerabilities(
        &self,
        _packages: &[(String, String)],
    ) -> Result<VulnerabilityDatabase> {
        // Download and parse the entire PyPA database
        let advisories = self.download_and_parse_database().await?;

        // Convert PyPA advisories to vulnerability database
        Self::convert_advisories(advisories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;
    use uv_cache::Cache;

    fn create_test_cache() -> AuditCache {
        let temp_dir = TempDir::new().unwrap();
        let cache = Cache::from_path(temp_dir.path()).init().unwrap();
        AuditCache::new(cache)
    }

    #[test]
    fn test_pypa_advisory_parsing() {
        let yaml = r#"
id: PYSEC-2007-1
details: The internationalization (i18n) framework in Django 0.91, 0.95, 0.95.1, and 0.96, and as used in other products such as PyLucid, when the USE_I18N option and the i18n component are enabled, allows remote attackers to cause a denial of service (memory consumption) via many HTTP requests with large Accept-Language headers.
affected:
- package:
    name: django
    ecosystem: PyPI
    purl: pkg:pypi/django
  ranges:
  - type: ECOSYSTEM
    events:
    - introduced: "0"
    - fixed: "1.1"
  versions:
  - 1.0.1
  - 1.0.2
  - 1.0.3
  - 1.0.4
references:
- type: ARTICLE
  url: http://www.djangoproject.com/weblog/2007/oct/26/security-fix
- type: ADVISORY
  url: http://secunia.com/advisories/27435
aliases:
- CVE-2007-5712
modified: "2021-07-15T02:22:07.728618Z"
published: "2007-10-30T19:46:00Z"
"#;

        let _source = PypaSource::new(create_test_cache(), true);

        let advisory = PypaSource::parse_advisory(yaml).unwrap();
        assert_eq!(advisory.id, "PYSEC-2007-1");
        assert!(advisory.details.contains("Django"));
        assert_eq!(advisory.affected.len(), 1);
        assert_eq!(advisory.aliases.len(), 1);
        assert_eq!(advisory.aliases[0], "CVE-2007-5712");
    }

    #[test]
    fn test_package_name_extraction() {
        let yaml = r"
id: TEST-1
details: Test
affected:
- package:
    ecosystem: PyPI
    name: django
- package:
    ecosystem: npm
    name: not-python
- package:
    ecosystem: PyPI
    name: flask
references: []
";

        let _source = PypaSource::new(create_test_cache(), true);

        let pypa_advisory = PypaSource::parse_advisory(yaml).unwrap();
        let package_names = PypaSource::extract_package_names(&pypa_advisory);

        assert_eq!(package_names.len(), 2);
        assert!(package_names.contains(&PackageName::from_str("django").unwrap()));
        assert!(package_names.contains(&PackageName::from_str("flask").unwrap()));
    }

    #[test]
    fn test_package_name_extraction_with_invalid_names() {
        let yaml = r"
id: TEST-INVALID
details: Test with invalid package names
affected:
- package:
    ecosystem: PyPI
    name: valid-package
- package:
    ecosystem: PyPI
    name: invalid-package-with-@#$%^&*()
- package:
    ecosystem: PyPI
    name: another-valid-package
references: []
";

        let _source = PypaSource::new(create_test_cache(), true);

        let pypa_advisory = PypaSource::parse_advisory(yaml).unwrap();
        let package_names = PypaSource::extract_package_names(&pypa_advisory);

        // Should only extract valid package names, ignoring invalid ones
        assert_eq!(package_names.len(), 2);
        assert!(package_names.contains(&PackageName::from_str("valid-package").unwrap()));
        assert!(package_names.contains(&PackageName::from_str("another-valid-package").unwrap()));
    }

    #[test]
    fn test_package_name_extraction_filters_non_pypi() {
        let yaml = r"
id: TEST-ECOSYSTEMS
details: Test with multiple ecosystems
affected:
- package:
    ecosystem: PyPI
    name: django
- package:
    ecosystem: npm
    name: express
- package:
    ecosystem: Go
    name: gorilla/mux
- package:
    ecosystem: PyPI
    name: flask
- package:
    ecosystem: Maven
    name: org.springframework:spring-core
references: []
";

        let _source = PypaSource::new(create_test_cache(), true);

        let pypa_advisory = PypaSource::parse_advisory(yaml).unwrap();
        let package_names = PypaSource::extract_package_names(&pypa_advisory);

        // Should only extract PyPI packages
        assert_eq!(package_names.len(), 2);
        assert!(package_names.contains(&PackageName::from_str("django").unwrap()));
        assert!(package_names.contains(&PackageName::from_str("flask").unwrap()));
    }

    #[test]
    fn test_to_vulnerability_conversion() {
        let yaml = "
id: PYSEC-2025-TEST
details: Test vulnerability conversion
affected:
- package:
    ecosystem: PyPI
    name: requests
  ranges:
  - type: ECOSYSTEM
    events:
    - introduced: \"2.0.0\"
    - fixed: \"2.31.0\"
references:
- type: ADVISORY
  url: https://example.com/advisory
aliases:
- CVE-2024-12345
modified: \"2025-01-14T21:22:18.665005Z\"
published: \"2025-01-14T19:15:32Z\"
";

        let _source = PypaSource::new(create_test_cache(), true);

        let pypa_advisory = PypaSource::parse_advisory(yaml).unwrap();
        let package_name = PackageName::from_str("requests").unwrap();
        let vulnerability = PypaSource::to_vulnerability(&pypa_advisory, &package_name).unwrap();

        // Verify conversion
        assert_eq!(vulnerability.id, "PYSEC-2025-TEST");
        assert_eq!(vulnerability.summary, "Test vulnerability conversion");
        assert_eq!(
            vulnerability.description,
            Some("Test vulnerability conversion".to_string())
        );
        assert_eq!(vulnerability.source, Some("pypa-zip".to_string()));
        assert_eq!(vulnerability.references.len(), 1);
    }
}
