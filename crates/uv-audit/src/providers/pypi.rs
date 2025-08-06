use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, warn};
use uv_cache::CacheEntry;

use crate::{
    AuditCache, AuditError, Result, Severity, VersionRange, Vulnerability, VulnerabilityDatabase,
    VulnerabilityProvider,
};

/// PyPI JSON API source for vulnerability data
pub struct PypiSource {
    cache: AuditCache,
    no_cache: bool,
    client: reqwest::Client,
}

impl PypiSource {
    /// Create a new PyPI source
    pub fn new(cache: AuditCache, no_cache: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            cache,
            no_cache,
            client,
        }
    }

    /// Get cache entry for a package/version
    fn cache_entry(&self, name: &str, version: &str) -> CacheEntry {
        self.cache.cache().entry(
            uv_cache::CacheBucket::VulnerabilityDatabase,
            format!("pypi/{name}/{version}"),
            "vulns.json",
        )
    }

    /// Fetch vulnerability data from PyPI for a single package
    async fn fetch_package_vulnerabilities(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Vec<Vulnerability>> {
        let cache_entry = self.cache_entry(name, version);

        // Check cache first unless no_cache is set
        if !self.no_cache && cache_entry.path().exists() {
            if let Ok(content) = fs_err::read(cache_entry.path()) {
                if let Ok(vulns) = serde_json::from_slice::<Vec<Vulnerability>>(&content) {
                    debug!("Using cached PyPI vulnerabilities for {} {}", name, version);
                    return Ok(vulns);
                }
            }
        }

        // Fetch from PyPI API
        let url = format!("https://pypi.org/pypi/{name}/{version}/json");
        debug!("Fetching vulnerabilities from PyPI: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(AuditError::DatabaseDownload)?;

        if !response.status().is_success() {
            if response.status() == 404 {
                // Package not found - return empty vulnerabilities
                return Ok(vec![]);
            }
            return Err(AuditError::Anyhow(anyhow::anyhow!(
                "PyPI API returned error: {}",
                response.status()
            )));
        }

        let data: PypiPackageResponse = response
            .json()
            .await
            .map_err(AuditError::DatabaseDownload)?;

        let vulnerabilities = data
            .vulnerabilities
            .unwrap_or_default()
            .into_iter()
            .map(|vuln| Self::convert_pypi_vulnerability(name, vuln))
            .collect::<Vec<_>>();

        // Cache the result
        if !self.no_cache {
            fs_err::create_dir_all(cache_entry.dir())?;
            let content = serde_json::to_vec(&vulnerabilities)?;
            fs_err::write(cache_entry.path(), content)?;
        }

        Ok(vulnerabilities)
    }

    /// Convert PyPI vulnerability format to internal format
    fn convert_pypi_vulnerability(package: &str, vuln: PypiVulnerability) -> Vulnerability {
        let severity = Self::map_severity(&vuln);

        // Extract affected version ranges from details or use current version
        let affected_versions = Self::extract_affected_ranges(&vuln);

        // Convert fixed_in strings to Versions
        let fixed_versions = vuln
            .fixed_in
            .unwrap_or_default()
            .iter()
            .filter_map(|v| uv_pep440::Version::from_str(v).ok())
            .collect();

        Vulnerability {
            id: vuln.id.clone(),
            summary: vuln.summary.unwrap_or_else(|| vuln.details.clone()),
            description: Some(vuln.details),
            severity,
            affected_versions,
            fixed_versions,
            references: vec![
                vuln.link
                    .unwrap_or_else(|| format!("https://pypi.org/project/{package}/")),
            ],
            cvss_score: None,
            published: None,
            modified: None,
            source: Some("pypi".to_string()),
        }
    }

    /// Map PyPI severity to internal severity
    fn map_severity(vuln: &PypiVulnerability) -> Severity {
        // PyPI doesn't provide severity directly, try to infer from aliases (CVE, GHSA)
        if let Some(aliases) = &vuln.aliases {
            for alias in aliases {
                if alias.starts_with("GHSA-") || alias.contains("CRITICAL") {
                    return Severity::Critical;
                }
                if alias.contains("HIGH") {
                    return Severity::High;
                }
                if alias.contains("MEDIUM") || alias.contains("MODERATE") {
                    return Severity::Medium;
                }
            }
        }

        // Default to medium if we can't determine
        Severity::Medium
    }

    /// Extract affected version ranges from vulnerability details
    fn extract_affected_ranges(vuln: &PypiVulnerability) -> Vec<VersionRange> {
        // PyPI doesn't provide structured affected ranges
        // We'll need to parse them from the details text or use fixed_in as a hint

        if let Some(fixed_in) = &vuln.fixed_in {
            if let Some(first_fixed) = fixed_in.first() {
                // Assume all versions before the first fixed version are affected
                if let Ok(version) = uv_pep440::Version::from_str(first_fixed) {
                    return vec![VersionRange {
                        min: None,
                        max: Some(version),
                        constraint: format!("<{first_fixed}"),
                    }];
                }
            }
        }

        // If we can't determine ranges, return an empty vec
        // This means all versions are potentially affected
        vec![]
    }

    /// Create a future for fetching package vulnerabilities  
    async fn fetch_package_future(
        &self,
        name: String,
        version: String,
    ) -> (String, String, Result<Vec<Vulnerability>>) {
        let result = self.fetch_package_vulnerabilities(&name, &version).await;
        (name, version, result)
    }
}

#[async_trait]
impl VulnerabilityProvider for PypiSource {
    fn name(&self) -> &'static str {
        "pypi"
    }

    async fn fetch_vulnerabilities(
        &self,
        packages: &[(String, String)],
    ) -> Result<VulnerabilityDatabase> {
        debug!(
            "Fetching vulnerabilities for {} packages from PyPI",
            packages.len()
        );

        // Fetch vulnerabilities for all packages concurrently with rate limiting
        const MAX_CONCURRENT_REQUESTS: usize = 15; // Limit to avoid overwhelming PyPI API

        let mut futures = FuturesUnordered::new();
        let mut package_iter = packages.iter().cloned();
        let mut successful_fetches = 0;
        let mut failed_fetches = 0;
        let mut vuln_map = HashMap::new();

        // Start initial batch of requests
        for _ in 0..MAX_CONCURRENT_REQUESTS.min(packages.len()) {
            if let Some((name, version)) = package_iter.next() {
                futures.push(self.fetch_package_future(name, version));
            }
        }

        // Process results as they complete, maintaining rate limit
        while let Some((name, version, result)) = futures.next().await {
            // Start a new request if there are more packages to process
            if let Some((next_name, next_version)) = package_iter.next() {
                futures.push(self.fetch_package_future(next_name, next_version));
            }

            match result {
                Ok(vulns) => {
                    successful_fetches += 1;
                    if !vulns.is_empty() {
                        debug!(
                            "Found {} vulnerabilities for {} {}",
                            vulns.len(),
                            name,
                            version
                        );
                        vuln_map.insert(name, vulns);
                    }
                }
                Err(e) => {
                    failed_fetches += 1;
                    warn!(
                        "Failed to fetch vulnerabilities for {} {}: {}",
                        name, version, e
                    );
                }
            }
        }

        debug!(
            "PyPI vulnerability processing complete: {} successful, {} failed, {} packages with vulnerabilities",
            successful_fetches,
            failed_fetches,
            vuln_map.len()
        );

        Ok(VulnerabilityDatabase::from_package_map(vuln_map))
    }
}

/// PyPI API response structure
#[derive(Debug, Deserialize, Serialize)]
struct PypiPackageResponse {
    info: PypiPackageInfo,
    #[serde(default)]
    vulnerabilities: Option<Vec<PypiVulnerability>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PypiPackageInfo {
    name: String,
    version: String,
}

/// PyPI vulnerability structure
#[derive(Debug, Deserialize, Serialize)]
struct PypiVulnerability {
    id: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    details: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    fixed_in: Option<Vec<String>>,
    #[serde(default)]
    link: Option<String>,
}
