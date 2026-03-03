//! Types and interfaces for interacting with [OSV] as a vulnerability service.
//!
//! Note: OSV supports a batched query API, but with significant limitations
//! that make it unsuitable for our purpose (namely, it doesn't include
//! anything except vulnerability IDs and last-modified information). As
//! a result, our current OSV backend only implements and uses the
//! single-query API.
//!
//! [OSV]: https://osv.dev/

use std::str::FromStr as _;
use tracing::trace;

use jiff::Timestamp;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use uv_pep440::Version;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

use crate::types::{Dependency, Finding, VulnerabilityID};

const API_BASE: &str = "https://api.osv.dev/";

/// Errors during OSV service interactions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error during an HTTP request, including middleware errors.
    #[error(transparent)]
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    /// An error when constructing the URL for an API request.
    #[error("Invalid API URL: {0}")]
    Url(DisplaySafeUrl, #[source] DisplaySafeUrlError),
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

/// The type of a version range in an OSV vulnerability record.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum RangeType {
    /// The versions in events are SemVer 2.0 versions.
    Semver,
    /// The versions in events are ecosystem-specific.
    /// In our context, this means they're PEP 440 versions.
    Ecosystem,
    /// The versions in events are full-length Git SHAs.
    Git,
    /// Some other range type. We don't expect these in OSV v1 records,
    /// but we include it for forward compatibility.
    /// NOTE: In principle we could use `untagged` here and capture the unknown
    /// type, but there's no value at the moment to doing this (since our processing
    /// of OSV records is limited to just ECOSYSTEM ranges).
    #[serde(other)]
    Other,
}

/// Version range for affected packages.
#[derive(Debug, Clone, Deserialize)]
struct Range {
    #[serde(rename = "type")]
    range_type: RangeType,
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

    /// Query OSV for vulnerabilities affecting the given dependency.
    pub async fn query(&self, dependency: &Dependency) -> Result<Vec<Finding>, Error> {
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

            // TODO: Technically the error here is unreachable, since `base_url` is valid by construction
            // and the path component is statically valid. We could perhaps just replace it with
            // an `expect`.
            let url = self
                .base_url
                .join("v1/query")
                .map_err(|e| Error::Url(self.base_url.clone(), e))?;
            let response = self
                .client
                .post(url.as_ref())
                .json(&request)
                .header("Content-Type", "application/json")
                .send()
                .await?;

            let query_response: QueryResponse = response
                .error_for_status()
                .map_err(reqwest_middleware::Error::Reqwest)?
                .json()
                .await
                .map_err(reqwest_middleware::Error::Reqwest)?;

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

    /// Convert an OSV Vulnerability record to a Finding.
    fn vulnerability_to_finding(dependency: &Dependency, vuln: Vulnerability) -> Finding {
        // Extract fix versions from affected ranges
        let fix_versions = vuln
            .affected
            .iter()
            .flatten()
            .flat_map(|affected| affected.ranges.iter().flatten())
            .filter(|range| matches!(range.range_type, RangeType::Ecosystem))
            .flat_map(|range| &range.events)
            .filter_map(|event| match event {
                // TODO: Warn on a malformed version string rather than silently skipping it.
                // Alternatively, we could propagate the raw version string in the finding and
                // leave it to the callsite to process into PEP 440 versions.
                Event::Fixed(fixed) => {
                    if let Ok(fixed) = Version::from_str(fixed) {
                        Some(fixed)
                    } else {
                        trace!(
                            "Skipping invalid (non-PEP 440) version in OSV record {id}: {fixed}",
                            id = vuln.id,
                        );
                        None
                    }
                }
                _ => None,
            })
            .collect();

        // Extract aliases
        let aliases = vuln
            .aliases
            .unwrap_or_default()
            .into_iter()
            .map(VulnerabilityID::new)
            .collect();

        let description = vuln.summary.or(vuln.details).unwrap_or(vuln.id.clone());

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

    use reqwest_middleware::ClientWithMiddleware;
    use serde_json::json;
    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_redacted::DisplaySafeUrl;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::service::osv::RangeType;
    use crate::types::{Dependency, Finding};

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

    #[test]
    fn test_deserialize_rangetype() {
        let json = r#"[
          "SEMVER",
          "ECOSYSTEM",
          "GIT",
          "OTHER",
          "UNKNOWN_TYPE"
        ]"#;

        let types: Vec<RangeType> =
            serde_json::from_str(json).expect("Failed to deserialize range types");

        insta::assert_debug_snapshot!(types, @"
        [
            Semver,
            Ecosystem,
            Git,
            Other,
            Other,
        ]
        ");
    }

    /// Ensure that we properly handle pagination in the OSV API, i.e. that we
    /// make multiple requests if necessary and use the page token.
    #[tokio::test]
    async fn test_query_pagination() {
        let server = MockServer::start().await;

        // First request: no page token.
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .and(body_json(json!({
                "package": {
                    "name": "foobar",
                    "ecosystem": "PyPI",
                },
                "version": "1.2.3",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "vulns": [
                    {
                        "id": "VULN-1",
                        "modified": "2026-01-01T00:00:00Z",
                        "published": "2026-01-01T00:00:00Z",
                    }
                ],
                "next_page_token": "token"
            })))
            .mount(&server)
            .await;

        // Second request: with page token.
        Mock::given(method("POST"))
            .and(path("/v1/query"))
            .and(body_json(json!({
                "package": {
                    "name": "foobar",
                    "ecosystem": "PyPI",
                },
                "version": "1.2.3",
                "page_token": "token",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "vulns": [
                    {
                        "id": "VULN-2",
                        "modified": "2026-01-02T00:00:00Z",
                        "published": "2026-01-02T00:00:00Z",
                    }
                ],
            })))
            .mount(&server)
            .await;

        let osv = Osv::new(
            ClientWithMiddleware::default(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
        );

        // Our findings should include vulnerabilities from both pages.
        let findings = osv
            .query(&Dependency::new(
                PackageName::from_str("foobar").unwrap(),
                Version::from_str("1.2.3").unwrap(),
            ))
            .await
            .expect("Failed to query OSV");

        insta::assert_debug_snapshot!(findings, @r#"
        [
            Vulnerability {
                dependency: Dependency {
                    name: PackageName(
                        "foobar",
                    ),
                    version: "1.2.3",
                },
                id: VulnerabilityID(
                    "VULN-1",
                ),
                description: "VULN-1",
                fix_versions: [],
                aliases: [],
                published: Some(
                    2026-01-01T00:00:00Z,
                ),
                modified: Some(
                    2026-01-01T00:00:00Z,
                ),
            },
            Vulnerability {
                dependency: Dependency {
                    name: PackageName(
                        "foobar",
                    ),
                    version: "1.2.3",
                },
                id: VulnerabilityID(
                    "VULN-2",
                ),
                description: "VULN-2",
                fix_versions: [],
                aliases: [],
                published: Some(
                    2026-01-02T00:00:00Z,
                ),
                modified: Some(
                    2026-01-02T00:00:00Z,
                ),
            },
        ]
        "#);

        // Ensure our mock server received both requests.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "Expected to receive two requests"
        );
    }

    /// Ensure that we can query and receive a known vulnerability from the OSV API.
    #[cfg(feature = "test-osv")]
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
}
