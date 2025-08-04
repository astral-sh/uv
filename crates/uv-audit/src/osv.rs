use crate::{AuditError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;
use uv_normalize::PackageName;
use uv_pep440::Version;

/// The OSV API base URL for fetching vulnerability data
const OSV_API_BASE: &str = "https://api.osv.dev/v1";

/// URL for downloading the `PyPA` advisory database
const PYPA_ADVISORY_DB_URL: &str = "https://github.com/pypa/advisory-database/archive/main.zip";

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

    /// Download the `PyPA` advisory database ZIP file
    pub async fn download_advisory_database(&self) -> Result<Vec<u8>> {
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

    /// Query OSV API for vulnerabilities affecting specific packages
    pub async fn query_packages(
        &self,
        packages: &[(PackageName, Version)],
    ) -> Result<Vec<OsvAdvisory>> {
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
    ) -> Result<Vec<OsvAdvisory>> {
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
            return Err(AuditError::DatabaseDownload(
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
