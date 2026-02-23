//! Types and interfaces for interacting with [OSV] as a vulnerability service.
//!
//! Note: OSV supports a batched query API, but with significant limitations
//! that make it unsuitable for our purpose (namely, it doesn't include
//! anything except vulnerability IDs and last-modified information). As
//! a result, our current OSV backend only implements and uses the
//! single-query API.
//!
//! [OSV]: https://osv.dev/

use jiff::Timestamp;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

use crate::{
    service::VulnerabilityService,
    types::{Dependency, Finding},
};

const API_BASE: &str = "https://api.osv.dev/";

/// Errors during OSV service interactions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error during an HTTP request, including middleware errors.
    #[error(transparent)]
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    /// An error when parsing the OSV API response.
    #[error("Invalid OSV API response: {0}")]
    Api(#[from] serde_json::Error),
    /// An error when constructing the URL for an API request.
    #[error("Invalid API URL: {0}")]
    Url(#[from] DisplaySafeUrlError),
}

/// Package specification for OSV queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Package {
    /// The package's name.
    name: String,
    /// The package's ecosystem.
    /// For our purposes, this will always be "PyPI".
    ecosystem: String,
}

/// Query request for a single package.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryRequest {
    package: Package,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_token: Option<String>,
}

/// Event in a vulnerability range.
/// Per the OSV schema, each event object contains exactly one of these event types.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Event {
    /// A version that introduces the vulnerability.
    Introduced(#[allow(dead_code)] String),
    /// A version that fixes the vulnerability.
    Fixed(String),
    /// The last known affected version.
    LastAffected(#[allow(dead_code)] String),
    /// An upper limit on the range.
    Limit(#[allow(dead_code)] String),
}

/// Version range for affected packages.
#[derive(Debug, Clone, Deserialize)]
struct Range {
    #[serde(rename = "type")]
    range_type: String,
    events: Vec<Event>,
}

/// Package affected by a vulnerability.
#[derive(Debug, Clone, Deserialize)]
struct Affected {
    ranges: Option<Vec<Range>>,
    // TODO: Enable these fields if/when they contain information that's
    // useful to us, e.g. metadata that constrains a vulnerability to specific
    // Python runtime versions, specific distributions of a version, etc.
    // ecosystem_specific: Option<serde_json::Value>,
    // database_specific: Option<serde_json::Value>,
}

/// A full vulnerability record from OSV.
#[derive(Debug, Clone, Deserialize)]
struct Vulnerability {
    id: String,
    modified: Timestamp,
    // Note: While the OSV spec says schema_version is required for versions >= 1.0.0,
    // some older records in the database don't have it, so we make it optional.
    // TODO: We could validate that this is 1.x, but the value of doing
    // so is probably limited given that we're strictly checking the shape
    // of the response anyways.
    #[allow(dead_code)]
    schema_version: Option<String>,
    summary: Option<String>,
    details: Option<String>,
    published: Option<Timestamp>,
    affected: Option<Vec<Affected>>,
    aliases: Option<Vec<String>>,
}

/// Response from a single query.
#[derive(Debug, Clone, Deserialize)]
struct QueryResponse {
    #[serde(default)]
    vulns: Vec<Vulnerability>,
    next_page_token: Option<String>,
}

/// Represents [OSV](https://osv.dev/), an open-source vulnerability database.
pub struct Osv {
    base_url: DisplaySafeUrl,
    client: ClientWithMiddleware,
}

impl Default for Osv {
    fn default() -> Self {
        Self {
            base_url: DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid"),
            client: ClientWithMiddleware::default(),
        }
    }
}

#[async_trait::async_trait]
impl VulnerabilityService for Osv {
    type Error = Error;

    async fn query(&self, dependency: &Dependency) -> Result<Vec<Finding>, Self::Error> {
        let mut all_vulnerabilities = Vec::new();
        let mut page_token: Option<String> = None;

        // Loop to handle pagination
        loop {
            let request = QueryRequest {
                package: Package {
                    name: dependency.name().to_string(),
                    ecosystem: "PyPI".to_string(),
                },
                version: dependency.version().to_string(),
                page_token: page_token.clone(),
            };

            let url = self.base_url.join("v1/query")?;
            let response = self
                .client
                .post(url.as_ref())
                .body(serde_json::to_string(&request)?)
                .header("Content-Type", "application/json")
                .send()
                .await?;

            let response = response
                .error_for_status()
                .map_err(reqwest_middleware::Error::Reqwest)?;
            let query_response: QueryResponse = serde_json::from_str(
                &response
                    .text()
                    .await
                    .map_err(reqwest_middleware::Error::Reqwest)?,
            )?;

            all_vulnerabilities.extend(query_response.vulns);

            // Check if there are more pages
            match query_response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        let findings = all_vulnerabilities
            .into_iter()
            .map(|vuln| Self::vulnerability_to_finding(dependency, vuln))
            .collect::<Vec<_>>();

        Ok(findings)
    }
}

impl Osv {
    /// Create a new OSV client with the given HTTP client and optional base URL.
    ///
    /// If no base URL is provided, the client will default to the official OSV API endpoint.
    pub fn new(client: ClientWithMiddleware, base_url: Option<DisplaySafeUrl>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| {
                DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid")
            }),
            client,
        }
    }

    /// Convert an OSV Vulnerability record to a Finding.
    fn vulnerability_to_finding(dependency: &Dependency, vuln: Vulnerability) -> Finding {
        use crate::types::VulnerabilityID;
        use std::str::FromStr;
        use uv_pep440::Version;

        // Extract fix versions from affected ranges
        let fix_versions = vuln
            .affected
            .as_ref()
            .and_then(|affected_list| {
                affected_list.iter().find_map(|affected| {
                    affected.ranges.as_ref().and_then(|ranges| {
                        ranges.iter().find_map(|range| {
                            (range.range_type == "ECOSYSTEM")
                                .then(|| {
                                    range.events.iter().find_map(|event| {
                                        // TODO: Warn on a malformed version string rather than silently skipping it.
                                        // Alternatively, we could propagate the raw version string in the finding and
                                        // leave it to the callsite to process into PEP 440 versions.
                                        match event {
                                            Event::Fixed(fixed) => Version::from_str(fixed).ok(),
                                            _ => None,
                                        }
                                    })
                                })
                                .flatten()
                        })
                    })
                })
            })
            .into_iter()
            .collect();

        // Extract aliases
        let aliases = vuln
            .aliases
            .unwrap_or_default()
            .into_iter()
            .map(VulnerabilityID::new)
            .collect();

        let description = vuln
            .summary
            .or(vuln.details)
            .unwrap_or_else(|| format!("Vulnerability {}", vuln.id));

        Finding::Vulnerability {
            dependency: dependency.clone(),
            id: VulnerabilityID::new(vuln.id),
            description,
            fix_versions,
            aliases,
            published: vuln.published,
            modified: Some(vuln.modified),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;
    use uv_pep440::Version;

    use crate::service::VulnerabilityService;
    use crate::types::Dependency;
    use crate::types::Finding;

    use super::API_BASE;
    use super::Event;
    use super::Osv;

    /// Ensures that the default OSV client is configured with our default OSV API base URL.
    #[test]
    fn test_osv_default() {
        let osv = Osv::default();
        assert_eq!(osv.base_url.as_str(), API_BASE);
    }

    #[test]
    fn test_deserialize_events() {
        let json = r#"[{ "introduced": "0" }, { "fixed": "46.0.5" }]"#;
        let events: Vec<Event> = serde_json::from_str(json).expect("Failed to deserialize events");

        insta::assert_debug_snapshot!(events, @r#"
        [
            Introduced(
                "0",
            ),
            Fixed(
                "46.0.5",
            ),
        ]
        "#);
    }

    /// Ensure that we can query and receive a known vulnerability from the OSV API.
    #[tokio::test]
    async fn test_query() {
        let osv = Osv::default();
        let package = PackageName::from_str("cryptography").unwrap();
        let version = Version::from_str("46.0.4").unwrap();
        let dependency = Dependency::new(package, version);

        let findings = osv.query(&dependency).await.unwrap();
        assert!(
            !findings.is_empty(),
            "Expected to find at least one vulnerability"
        );

        // We know GHSA-r6ph-v2qm-q3c2 exists for cryptography 46.0.4.
        let finding = findings
            .iter()
            .find(|finding| match finding {
                Finding::Vulnerability { id, .. } => id.as_str() == "GHSA-r6ph-v2qm-q3c2",
                Finding::ProjectStatus { .. } => false,
            })
            .expect("Expected to find GHSA-r6ph-v2qm-q3c2 vulnerability");

        insta::assert_debug_snapshot!(finding, @r#"
        Vulnerability {
            dependency: Dependency {
                name: PackageName(
                    "cryptography",
                ),
                version: "46.0.4",
            },
            id: VulnerabilityID(
                "GHSA-r6ph-v2qm-q3c2",
            ),
            description: "cryptography Vulnerable to a Subgroup Attack Due to Missing Subgroup Validation for SECT Curves",
            fix_versions: [
                "46.0.5",
            ],
            aliases: [
                VulnerabilityID(
                    "CVE-2026-26007",
                ),
            ],
            published: Some(
                2026-02-10T21:27:06Z,
            ),
            modified: Some(
                2026-02-11T15:58:46.005582Z,
            ),
        }
        "#);
    }

    /// Ensure that we can query a batch of packages and receive known vulnerabilities.
    #[tokio::test]
    async fn test_query_batch() {
        let osv = Osv::default();

        // Set up two dependencies with known vulnerabilities
        let cryptography_package = PackageName::from_str("cryptography").unwrap();
        let cryptography_version = Version::from_str("46.0.4").unwrap();
        let cryptography_dep = Dependency::new(cryptography_package, cryptography_version);

        let requests_package = PackageName::from_str("requests").unwrap();
        let requests_version = Version::from_str("2.32.3").unwrap();
        let requests_dep = Dependency::new(requests_package, requests_version);

        let dependencies = vec![cryptography_dep.clone(), requests_dep.clone()];

        let results = osv.query_batch(&dependencies).await.unwrap();

        // Verify we got results for both packages
        assert_eq!(results.len(), 2, "Expected results for both packages");

        // Check cryptography findings
        let cryptography_findings = results
            .get(&cryptography_dep)
            .expect("Expected findings for cryptography");
        assert!(
            !cryptography_findings.is_empty(),
            "Expected to find at least one vulnerability for cryptography"
        );

        let cryptography_finding = cryptography_findings
            .iter()
            .find(|finding| match finding {
                Finding::Vulnerability { id, .. } => id.as_str() == "GHSA-r6ph-v2qm-q3c2",
                Finding::ProjectStatus { .. } => false,
            })
            .expect("Expected to find GHSA-r6ph-v2qm-q3c2 vulnerability for cryptography");

        insta::assert_debug_snapshot!(cryptography_finding, @r#"
        Vulnerability {
            dependency: Dependency {
                name: PackageName(
                    "cryptography",
                ),
                version: "46.0.4",
            },
            id: VulnerabilityID(
                "GHSA-r6ph-v2qm-q3c2",
            ),
            description: "cryptography Vulnerable to a Subgroup Attack Due to Missing Subgroup Validation for SECT Curves",
            fix_versions: [
                "46.0.5",
            ],
            aliases: [
                VulnerabilityID(
                    "CVE-2026-26007",
                ),
            ],
            published: Some(
                2026-02-10T21:27:06Z,
            ),
            modified: Some(
                2026-02-11T15:58:46.005582Z,
            ),
        }
        "#);

        // Check requests findings
        let requests_findings = results
            .get(&requests_dep)
            .expect("Expected findings for requests");
        assert!(
            !requests_findings.is_empty(),
            "Expected to find at least one vulnerability for requests"
        );

        let requests_finding = requests_findings
            .iter()
            .find(|finding| match finding {
                Finding::Vulnerability { id, .. } => id.as_str() == "GHSA-9hjg-9r4m-mvj7",
                Finding::ProjectStatus { .. } => false,
            })
            .expect("Expected to find GHSA-9hjg-9r4m-mvj7 vulnerability for requests");

        insta::assert_debug_snapshot!(requests_finding, @r#"
        Vulnerability {
            dependency: Dependency {
                name: PackageName(
                    "requests",
                ),
                version: "2.32.3",
            },
            id: VulnerabilityID(
                "GHSA-9hjg-9r4m-mvj7",
            ),
            description: "Requests vulnerable to .netrc credentials leak via malicious URLs",
            fix_versions: [
                "2.32.4",
            ],
            aliases: [
                VulnerabilityID(
                    "CVE-2024-47081",
                ),
            ],
            published: Some(
                2025-06-09T19:06:08Z,
            ),
            modified: Some(
                2026-02-04T03:44:00.676479Z,
            ),
        }
        "#);
    }
}
