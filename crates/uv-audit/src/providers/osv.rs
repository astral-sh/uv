use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, warn};
use uv_cache::CacheEntry;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{
    AuditCache, AuditError, Result, Severity, VersionRange, Vulnerability, VulnerabilityDatabase,
    VulnerabilityProvider,
};

/// The OSV API base URL for fetching vulnerability data
const OSV_API_BASE: &str = "https://api.osv.dev/v1";

/// An OSV (Open Source Vulnerabilities) advisory record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvAdvisory {
    /// Unique vulnerability identifier
    pub id: String,

    /// Vulnerability summary
    pub summary: String,

    /// Detailed description
    pub details: Option<String>,

    /// Affected packages and versions
    pub affected: Vec<OsvAffected>,

    /// Reference URLs
    pub references: Vec<OsvReference>,

    /// Severity information
    pub severity: Vec<OsvSeverity>,

    /// Publication timestamp
    pub published: Option<String>,

    /// Last modification timestamp
    pub modified: Option<String>,

    /// Database-specific fields
    pub database_specific: Option<serde_json::Value>,
}

/// Affected package information in an OSV advisory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvAffected {
    /// Package information
    pub package: OsvPackage,

    /// Version ranges affected
    pub ranges: Vec<OsvRange>,

    /// Specific versions affected
    pub versions: Option<Vec<String>>,

    /// Ecosystem-specific database information
    pub database_specific: Option<serde_json::Value>,

    /// Ecosystem-specific fields
    pub ecosystem_specific: Option<serde_json::Value>,
}

/// Package information in OSV format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvPackage {
    /// Package ecosystem (e.g., "PyPI")
    pub ecosystem: String,

    /// Package name
    pub name: String,

    /// Package URL if available
    pub purl: Option<String>,
}

/// Version range specification in OSV format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvRange {
    /// Range type (e.g., "ECOSYSTEM")
    #[serde(rename = "type")]
    pub range_type: String,

    /// Repository URL for version control ranges
    pub repo: Option<String>,

    /// Events defining the range (introduced, fixed, etc.)
    pub events: Vec<OsvEvent>,

    /// Database-specific information
    pub database_specific: Option<serde_json::Value>,
}

/// A version event in an OSV range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvEvent {
    /// Version where event occurs
    pub introduced: Option<String>,

    /// Version where issue is fixed
    pub fixed: Option<String>,

    /// Last affected version
    pub last_affected: Option<String>,

    /// Version limit
    pub limit: Option<String>,
}

/// Reference information in OSV format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvReference {
    /// Reference type (e.g., "ADVISORY", "FIX", "WEB")
    #[serde(rename = "type")]
    pub ref_type: String,

    /// Reference URL
    pub url: String,
}

/// Severity information in OSV format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvSeverity {
    /// Severity type (e.g., `CVSS_V3`)
    #[serde(rename = "type")]
    pub severity_type: String,

    /// Severity score
    pub score: String,
}

/// Client for interacting with the OSV API and advisory database
pub struct OsvClient {
    client: Client,
}

impl OsvClient {
    /// Create a new OSV client
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(format!("uv/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client }
    }

    /// Query OSV API for vulnerabilities affecting specific packages
    pub async fn query_packages(
        &self,
        packages: &[(PackageName, Version)],
    ) -> crate::Result<Vec<OsvAdvisory>> {
        const BATCH_SIZE: usize = 10;

        debug!("Querying OSV API for {} packages", packages.len());

        let mut all_advisories = Vec::new();

        for batch in packages.chunks(BATCH_SIZE) {
            let mut batch_advisories = Vec::new();

            for (package_name, version) in batch {
                match self.query_package(package_name, version).await {
                    Ok(mut advisories) => {
                        batch_advisories.append(&mut advisories);
                    }
                    Err(e) => {
                        debug!("Failed to query package {}: {}", package_name, e);
                        // Continue with other packages rather than failing entirely
                    }
                }

                // Rate limiting: small delay between requests
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            all_advisories.extend(batch_advisories);
        }

        Ok(all_advisories)
    }

    /// Query OSV API for vulnerabilities affecting a specific package
    pub async fn query_package(
        &self,
        package_name: &PackageName,
        version: &Version,
    ) -> crate::Result<Vec<OsvAdvisory>> {
        debug!("Querying OSV API for package {}@{}", package_name, version);

        let query = serde_json::json!({
            "package": {
                "ecosystem": "PyPI",
                "name": package_name.to_string()
            },
            "version": version.to_string()
        });

        let response = self
            .client
            .post(format!("{OSV_API_BASE}/query"))
            .json(&query)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(crate::AuditError::DatabaseDownload(
                response.error_for_status().unwrap_err(),
            ));
        }

        let result: serde_json::Value = response.json().await?;

        let empty_vec: Vec<serde_json::Value> = vec![];
        let advisories: Vec<OsvAdvisory> = result
            .get("vulns")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec)
            .iter()
            .filter_map(|advisory| {
                serde_json::from_value::<OsvAdvisory>(advisory.clone())
                    .map_err(|e| {
                        debug!("Failed to parse OSV advisory: {}", e);
                        e
                    })
                    .ok()
            })
            .collect();

        debug!(
            "Found {} advisories for {}@{}",
            advisories.len(),
            package_name,
            version
        );

        Ok(advisories)
    }

    /// Get the underlying HTTP client
    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for working with OSV data
impl OsvAdvisory {
    /// Extract affected PyPI packages from this advisory
    pub fn pypi_packages(&self) -> Vec<&OsvAffected> {
        self.affected
            .iter()
            .filter(|affected| affected.package.ecosystem == "PyPI")
            .collect()
    }

    /// Get the highest severity score if available
    pub fn max_cvss_score(&self) -> Option<f32> {
        self.severity
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

    /// Check if this advisory affects a specific package version
    pub fn affects_version(&self, package_name: &PackageName, version: &Version) -> bool {
        for affected in self.pypi_packages() {
            if affected.package.name != package_name.to_string() {
                continue;
            }

            // Check if version is explicitly listed
            if let Some(versions) = &affected.versions {
                if versions.contains(&version.to_string()) {
                    return true;
                }
            }

            // Check if version falls within any affected range
            for range in &affected.ranges {
                if Self::version_in_range(version, range) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a version falls within an OSV range
    fn version_in_range(version: &Version, range: &OsvRange) -> bool {
        if range.range_type != "ECOSYSTEM" {
            return false;
        }

        let mut introduced: Option<Version> = None;
        let mut fixed: Option<Version> = None;

        for event in &range.events {
            if let Some(introduced_str) = &event.introduced {
                if introduced_str == "0" {
                    introduced = Some(Version::new([0, 0, 0]));
                } else if let Ok(v) = introduced_str.parse() {
                    introduced = Some(v);
                }
            }

            if let Some(fixed_str) = &event.fixed {
                if let Ok(v) = fixed_str.parse() {
                    fixed = Some(v);
                }
            }
        }

        // Check if version is within the vulnerable range
        let after_introduced = introduced.is_none_or(|intro| version >= &intro);
        let before_fixed = fixed.is_none_or(|fix| version < &fix);

        after_introduced && before_fixed
    }
}

/// OSV.dev API source for vulnerability data
pub struct OsvSource {
    cache: AuditCache,
    no_cache: bool,
    client: reqwest::Client,
}

impl OsvSource {
    /// Create a new OSV source
    pub fn new(cache: AuditCache, no_cache: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        Self {
            cache,
            no_cache,
            client,
        }
    }

    /// Get cache entry for OSV batch with package-specific key
    fn cache_entry(&self, packages: &[(String, String)]) -> CacheEntry {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Create a hash of the package set to differentiate cache entries
        let mut hasher = DefaultHasher::new();
        for (name, version) in packages {
            name.hash(&mut hasher);
            version.hash(&mut hasher);
        }
        let package_hash = hasher.finish();

        self.cache.cache().entry(
            uv_cache::CacheBucket::VulnerabilityDatabase,
            "osv/batch",
            format!("vulns-{package_hash:x}.json"),
        )
    }

    /// Convert OSV advisory to internal vulnerability format
    fn convert_osv_vulnerability(advisory: OsvAdvisory) -> Option<Vulnerability> {
        // Extract package name from affected entries
        if advisory.affected.is_empty() {
            return None;
        }
        let first_affected = &advisory.affected[0];

        // Map OSV severity
        let severity = Self::map_osv_severity(&advisory);

        // Extract version ranges
        let affected_versions = Self::extract_osv_ranges(first_affected);

        // Extract fixed versions
        let fixed_versions = Self::extract_fixed_versions(first_affected);

        // Build references
        let mut references = vec![];
        for reference in advisory.references {
            references.push(reference.url);
        }
        // Add OSV URL as a reference
        references.push(format!("https://osv.dev/vulnerability/{}", advisory.id));

        Some(Vulnerability {
            id: advisory.id,
            summary: if advisory.summary.is_empty() {
                advisory
                    .details
                    .clone()
                    .unwrap_or_else(|| "OSV advisory".to_string())
            } else {
                advisory.summary
            },
            description: advisory.details,
            severity,
            affected_versions,
            fixed_versions,
            references,
            cvss_score: None,
            published: advisory
                .published
                .and_then(|s| jiff::Timestamp::from_str(&s).ok()),
            modified: advisory
                .modified
                .and_then(|s| jiff::Timestamp::from_str(&s).ok()),
            source: Some("osv".to_string()),
        })
    }

    /// Map OSV severity to internal severity
    fn map_osv_severity(advisory: &OsvAdvisory) -> Severity {
        for severity in &advisory.severity {
            let score = &severity.score;
            // CVSS v3 scoring
            if score.contains("CRITICAL") || score.contains("9.") || score.contains("10.") {
                return Severity::Critical;
            }
            if score.contains("HIGH") || score.contains("7.") || score.contains("8.") {
                return Severity::High;
            }
            if score.contains("MEDIUM")
                || score.contains("4.")
                || score.contains("5.")
                || score.contains("6.")
            {
                return Severity::Medium;
            }
            if score.contains("LOW") {
                return Severity::Low;
            }
        }

        // Check database_specific field for severity hints
        if let Some(db_specific) = &advisory.database_specific {
            let data_str = db_specific.to_string().to_uppercase();
            if data_str.contains("CRITICAL") {
                return Severity::Critical;
            }
            if data_str.contains("HIGH") {
                return Severity::High;
            }
            if data_str.contains("MEDIUM") || data_str.contains("MODERATE") {
                return Severity::Medium;
            }
            if data_str.contains("LOW") {
                return Severity::Low;
            }
        }

        Severity::Medium
    }

    /// Extract version ranges from OSV affected entry
    fn extract_osv_ranges(affected: &OsvAffected) -> Vec<VersionRange> {
        let mut ranges = vec![];

        for range in &affected.ranges {
            if range.range_type != "ECOSYSTEM" && range.range_type != "SEMVER" {
                continue;
            }

            let mut min_version = None;
            let mut max_version = None;

            for event in &range.events {
                if let Some(intro) = &event.introduced {
                    if intro != "0" {
                        if let Ok(version) = uv_pep440::Version::from_str(intro) {
                            min_version = Some(version);
                        }
                    }
                }
                if let Some(fix) = &event.fixed {
                    if let Ok(version) = uv_pep440::Version::from_str(fix) {
                        max_version = Some(version);
                    }
                }
            }

            let constraint = match (&min_version, &max_version) {
                (Some(min), Some(max)) => format!(">={min},<{max}"),
                (Some(min), None) => format!(">={min}"),
                (None, Some(max)) => format!("<{max}"),
                (None, None) => "*".to_string(),
            };

            ranges.push(VersionRange {
                min: min_version,
                max: max_version,
                constraint,
            });
        }

        ranges
    }

    /// Extract fixed versions from OSV affected entry
    fn extract_fixed_versions(affected: &OsvAffected) -> Vec<uv_pep440::Version> {
        let mut fixed_versions = vec![];

        for range in &affected.ranges {
            for event in &range.events {
                if let Some(fixed) = &event.fixed {
                    if let Ok(version) = uv_pep440::Version::from_str(fixed) {
                        fixed_versions.push(version);
                    }
                }
            }
        }

        fixed_versions
    }

    /// Fetch full vulnerability details for a specific vulnerability ID
    async fn fetch_vulnerability_details(&self, vuln_id: &str) -> Result<Option<OsvAdvisory>> {
        let response = self
            .client
            .get(format!("https://api.osv.dev/v1/vulns/{vuln_id}"))
            .send()
            .await
            .map_err(AuditError::DatabaseDownload)?;

        if response.status() == 404 {
            return Ok(None);
        }

        if !response.status().is_success() {
            warn!(
                "OSV API returned error {} for vulnerability {}",
                response.status(),
                vuln_id
            );
            return Ok(None);
        }

        let advisory: OsvAdvisory = response
            .json()
            .await
            .map_err(AuditError::DatabaseDownload)?;

        Ok(Some(advisory))
    }

    /// Create a future for fetching vulnerability details
    async fn fetch_vulnerability_future(
        &self,
        vuln_id: String,
    ) -> (String, Result<Option<OsvAdvisory>>) {
        let result = self.fetch_vulnerability_details(&vuln_id).await;
        (vuln_id, result)
    }
}

#[async_trait]
impl VulnerabilityProvider for OsvSource {
    fn name(&self) -> &'static str {
        "osv"
    }

    async fn fetch_vulnerabilities(
        &self,
        packages: &[(String, String)],
    ) -> Result<VulnerabilityDatabase> {
        let cache_entry = self.cache_entry(packages);

        // Check cache first unless no_cache is set
        if !self.no_cache && cache_entry.path().exists() {
            if let Ok(content) = fs_err::read(cache_entry.path()) {
                if let Ok(db) = serde_json::from_slice::<VulnerabilityDatabase>(&content) {
                    debug!("Using cached OSV vulnerabilities");
                    return Ok(db);
                }
            }
        }

        // Build batch query with version constraints
        let queries: Vec<OsvQuery> = packages
            .iter()
            .map(|(name, version)| {
                debug!(
                    "Building OSV query for package: {} version: {}",
                    name, version
                );
                OsvQuery {
                    package: Some(OsvPackage {
                        name: name.clone(),
                        ecosystem: "PyPI".to_string(),
                        purl: None,
                    }),
                    version: Some(version.clone()),
                }
            })
            .collect();

        debug!("Built {} OSV queries total", queries.len());

        // Split into batches of 1000 (OSV API limit)
        const BATCH_SIZE: usize = 1000;
        let mut all_vulnerability_ids = Vec::new();
        let mut package_vuln_mapping = HashMap::new();

        for batch in queries.chunks(BATCH_SIZE) {
            let request = OsvBatchRequest {
                queries: batch.to_vec(),
            };

            debug!("Querying OSV API with {} packages", batch.len());

            let response = self
                .client
                .post("https://api.osv.dev/v1/querybatch")
                .json(&request)
                .send()
                .await
                .map_err(AuditError::DatabaseDownload)?;

            if !response.status().is_success() {
                warn!("OSV API returned error: {}", response.status());
                continue;
            }

            let response_text = response
                .text()
                .await
                .map_err(AuditError::DatabaseDownload)?;

            debug!("OSV API response body: {}", response_text);

            let batch_response: OsvBatchResponse =
                serde_json::from_str(&response_text).map_err(|e| {
                    warn!("Failed to parse OSV response: {}", e);
                    warn!("Response text: {}", response_text);
                    AuditError::Json(e)
                })?;

            debug!(
                "Successfully parsed batch response with {} results",
                batch_response.results.len()
            );

            // Collect vulnerability IDs and map them to packages
            for (idx, result) in batch_response.results.into_iter().enumerate() {
                let package_name = if let Some(query) = batch.get(idx) {
                    query
                        .package
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "unknown".to_string())
                } else {
                    "unknown".to_string()
                };

                for vuln in result.vulns {
                    debug!(
                        "Found vulnerability {} for package {}",
                        vuln.id, package_name
                    );
                    all_vulnerability_ids.push(vuln.id.clone());
                    package_vuln_mapping.insert(vuln.id, package_name.clone());
                }
            }
        }

        debug!(
            "Found {} vulnerability IDs, fetching full details",
            all_vulnerability_ids.len()
        );

        // Fetch full vulnerability details concurrently
        let mut all_vulnerabilities = HashMap::new();
        let mut successful_fetches = 0;
        let mut failed_fetches = 0;

        // Create concurrent futures with rate limiting
        const MAX_CONCURRENT_REQUESTS: usize = 10; // Limit concurrent requests to avoid overwhelming OSV API
        let mut futures = FuturesUnordered::new();
        let mut vuln_iter = all_vulnerability_ids.clone().into_iter();

        // Start initial batch of requests
        for _ in 0..MAX_CONCURRENT_REQUESTS.min(all_vulnerability_ids.len()) {
            if let Some(vuln_id) = vuln_iter.next() {
                futures.push(self.fetch_vulnerability_future(vuln_id));
            }
        }

        // Process results as they complete, maintaining rate limit
        while let Some((vuln_id, result)) = futures.next().await {
            // Start a new request if there are more vulnerability IDs to process
            if let Some(next_vuln_id) = vuln_iter.next() {
                futures.push(self.fetch_vulnerability_future(next_vuln_id));
            }
            match result {
                Ok(Some(advisory)) => {
                    successful_fetches += 1;
                    match Self::convert_osv_vulnerability(advisory) {
                        Some(vuln) => {
                            let package_name = package_vuln_mapping
                                .get(&vuln_id)
                                .unwrap_or(&"unknown".to_string())
                                .clone();

                            debug!(
                                "Successfully processed vulnerability {} for package {}",
                                vuln_id, package_name
                            );
                            all_vulnerabilities
                                .entry(package_name)
                                .or_insert_with(Vec::new)
                                .push(vuln);
                        }
                        None => {
                            warn!(
                                "Failed to convert OSV advisory to vulnerability: {}",
                                vuln_id
                            );
                        }
                    }
                }
                Ok(None) => {
                    failed_fetches += 1;
                    debug!("No details found for vulnerability: {}", vuln_id);
                }
                Err(e) => {
                    failed_fetches += 1;
                    warn!(
                        "Failed to fetch details for vulnerability {}: {}",
                        vuln_id, e
                    );
                }
            }
        }

        debug!(
            "OSV vulnerability processing complete: {} successful, {} failed, {} total packages with vulnerabilities",
            successful_fetches,
            failed_fetches,
            all_vulnerabilities.len()
        );

        let db = VulnerabilityDatabase::from_package_map(all_vulnerabilities);

        // Cache the result
        if !self.no_cache {
            fs_err::create_dir_all(cache_entry.dir())?;
            let content = serde_json::to_vec(&db)?;
            fs_err::write(cache_entry.path(), content)?;
        }

        Ok(db)
    }
}

/// OSV batch API request
#[derive(Debug, Clone, Serialize)]
struct OsvBatchRequest {
    queries: Vec<OsvQuery>,
}

/// OSV query structure  
#[derive(Debug, Clone, Serialize)]
struct OsvQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<OsvPackage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// OSV batch API response
#[derive(Debug, Deserialize)]
struct OsvBatchResponse {
    results: Vec<OsvResult>,
}

/// OSV query result - batch API returns lightweight data
#[derive(Debug, Deserialize)]
struct OsvResult {
    #[serde(default)]
    vulns: Vec<OsvLightweightVuln>,
}

/// Lightweight vulnerability data from batch API
#[derive(Debug, Deserialize)]
struct OsvLightweightVuln {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_osv_advisory_parsing() {
        let json = r#"{
            "id": "GHSA-test-1234",
            "summary": "Test vulnerability",
            "details": "This is a test vulnerability",
            "affected": [
                {
                    "package": {
                        "ecosystem": "PyPI",
                        "name": "test-package"
                    },
                    "ranges": [
                        {
                            "type": "ECOSYSTEM",
                            "events": [
                                {"introduced": "1.0.0"},
                                {"fixed": "1.2.0"}
                            ]
                        }
                    ]
                }
            ],
            "references": [],
            "severity": [],
            "published": "2023-01-01T00:00:00Z"
        }"#;

        let advisory: OsvAdvisory = serde_json::from_str(json).unwrap();
        assert_eq!(advisory.id, "GHSA-test-1234");
        assert_eq!(advisory.summary, "Test vulnerability");
        assert_eq!(advisory.affected.len(), 1);
    }

    #[test]
    fn test_version_range_check() {
        let json = r#"{
            "id": "GHSA-test-1234",
            "summary": "Test vulnerability",
            "affected": [
                {
                    "package": {
                        "ecosystem": "PyPI",
                        "name": "test-package"
                    },
                    "ranges": [
                        {
                            "type": "ECOSYSTEM",
                            "events": [
                                {"introduced": "1.0.0"},
                                {"fixed": "1.2.0"}
                            ]
                        }
                    ]
                }
            ],
            "references": [],
            "severity": []
        }"#;

        let advisory: OsvAdvisory = serde_json::from_str(json).unwrap();
        let package_name = PackageName::from_str("test-package").unwrap();

        // Version in range should be affected
        let vulnerable_version = Version::from_str("1.1.0").unwrap();
        assert!(advisory.affects_version(&package_name, &vulnerable_version));

        // Version before range should not be affected
        let safe_version = Version::from_str("0.9.0").unwrap();
        assert!(!advisory.affects_version(&package_name, &safe_version));

        // Version after fix should not be affected
        let fixed_version = Version::from_str("1.2.0").unwrap();
        assert!(!advisory.affects_version(&package_name, &fixed_version));
    }
}
