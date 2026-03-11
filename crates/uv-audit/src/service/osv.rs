//! Types and interfaces for interacting with [OSV] as a vulnerability service.
//!
//! We use OSV's `/v1/querybatch` endpoint to collect vulnerability IDs for all
//! dependencies in a single round-trip (handling pagination as needed), then
//! fetch full vulnerability records from `/v1/vulns/{id}` concurrently.
//!
//! [OSV]: https://osv.dev/

use rustc_hash::{FxHashMap, FxHashSet};
use std::str::FromStr as _;
use tracing::trace;

use crate::types;
use futures::{StreamExt as _, TryStreamExt as _};
use jiff::Timestamp;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use uv_configuration::Concurrency;
use uv_pep440::Version;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

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
#[derive(Debug, Clone, Serialize)]
struct Package {
    /// The package's name.
    name: String,
    /// The package's ecosystem.
    /// For our purposes, this will always be "PyPI".
    ecosystem: String,
}

/// Query request for a single package.
#[derive(Debug, Clone, Serialize)]
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

/// The type of a reference in an OSV vulnerability record.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum ReferenceType {
    Advisory,
    Article,
    Detection,
    Discussion,
    Report,
    Fix,
    Introduced,
    Package,
    Evidence,
    Web,
    /// Some other reference type. We don't expect these in OSV v1 records,
    /// but we include it for forward compatibility.
    #[serde(other)]
    Other,
}

/// A reference for more information about a vulnerability.
#[derive(Debug, Clone, Deserialize)]
struct Reference {
    #[serde(rename = "type")]
    reference_type: ReferenceType,
    url: DisplaySafeUrl,
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
    references: Option<Vec<Reference>>,
}

/// Request body for the batch query API.
#[derive(Debug, Clone, Serialize)]
struct QueryBatchRequest {
    queries: Vec<QueryRequest>,
}

/// A summary of a vulnerability returned by the batch query API.
/// Note: the batch query API only returns IDs and modification timestamps, not full records.
#[derive(Debug, Clone, Deserialize)]
struct VulnSummary {
    id: String,
}

/// One result entry in a batch query response, corresponding to one input query.
#[derive(Debug, Clone, Deserialize)]
struct QueryBatchResult {
    #[serde(default)]
    vulns: Vec<VulnSummary>,
    next_page_token: Option<String>,
}

/// Response from a batch query.
#[derive(Debug, Clone, Deserialize)]
struct QueryBatchResponse {
    results: Vec<QueryBatchResult>,
}

/// Represents [OSV](https://osv.dev/), an open-source vulnerability database.
pub struct Osv {
    base_url: DisplaySafeUrl,
    client: ClientWithMiddleware,
    concurrency: Concurrency,
}

impl Default for Osv {
    fn default() -> Self {
        Self {
            base_url: DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid"),
            client: ClientWithMiddleware::default(),
            concurrency: Concurrency::default(),
        }
    }
}

impl Osv {
    /// Create a new OSV client with the given HTTP client and optional base URL.
    ///
    /// If no base URL is provided, the client will default to the official OSV API endpoint.
    pub fn new(
        client: ClientWithMiddleware,
        base_url: Option<DisplaySafeUrl>,
        concurrency: Concurrency,
    ) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| {
                DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid")
            }),
            client,
            concurrency,
        }
    }

    pub async fn query_batch(
        &self,
        dependencies: &[types::Dependency],
    ) -> Result<Vec<types::Finding>, Error> {
        if dependencies.is_empty() {
            return Ok(vec![]);
        }

        // Accumulated (dependency, vuln_id) pairs across all pages.
        let mut dep_vuln_ids: Vec<(&types::Dependency, String)> = Vec::new();

        // Pending queries: (dependency, page_token). Initially one per dependency with no token.
        let mut pending: Vec<(&types::Dependency, Option<String>)> =
            dependencies.iter().map(|dep| (dep, None)).collect();

        loop {
            let request = QueryBatchRequest {
                queries: pending
                    .iter()
                    .map(|(dep, page_token)| QueryRequest {
                        package: Package {
                            name: dep.name().to_string(),
                            ecosystem: "PyPI".to_string(),
                        },
                        version: dep.version().to_string(),
                        page_token: page_token.clone(),
                    })
                    .collect(),
            };

            let url = self
                .base_url
                .join("v1/querybatch")
                .map_err(|e| Error::Url(self.base_url.clone(), e))?;
            let batch_response: QueryBatchResponse = self
                .client
                .post(url.as_ref())
                .json(&request)
                .send()
                .await?
                .error_for_status()
                .map_err(reqwest_middleware::Error::Reqwest)?
                .json()
                .await
                .map_err(reqwest_middleware::Error::Reqwest)?;

            let mut next_pending = Vec::new();
            for ((dep, _), result) in pending.iter().zip(batch_response.results.iter()) {
                dep_vuln_ids.extend(result.vulns.iter().map(|v| (*dep, v.id.clone())));
                if let Some(token) = &result.next_page_token {
                    next_pending.push((*dep, Some(token.clone())));
                }
            }

            if next_pending.is_empty() {
                break;
            }
            pending = next_pending;
        }

        // Collect unique vuln IDs to minimize fetches.
        let unique_ids: FxHashSet<_> = dep_vuln_ids.iter().map(|(_, id)| id.clone()).collect();

        // Fetch full vulnerability records concurrently.
        let vuln_details = futures::stream::iter(unique_ids)
            .map(async |id| {
                let vuln = self.fetch_vuln(&id).await?;
                Ok::<(String, Vulnerability), Error>((id, vuln))
            })
            .buffer_unordered(self.concurrency.downloads)
            .try_collect::<FxHashMap<String, Vulnerability>>()
            .await?;

        // Build findings from the accumulated (dependency, vuln_id) pairs.
        let findings = dep_vuln_ids
            .iter()
            .filter_map(|(dep, vuln_id)| {
                vuln_details
                    .get(vuln_id)
                    .map(|vuln| Self::vulnerability_to_finding(dep, vuln.clone()))
            })
            .collect();

        Ok(findings)
    }

    /// Fetch a full vulnerability record by ID from OSV.
    async fn fetch_vuln(&self, id: &str) -> Result<Vulnerability, Error> {
        let url = self
            .base_url
            .join(&format!("v1/vulns/{id}"))
            .map_err(|e| Error::Url(self.base_url.clone(), e))?;
        let vuln: Vulnerability = self
            .client
            .get(url.as_ref())
            .send()
            .await?
            .error_for_status()
            .map_err(reqwest_middleware::Error::Reqwest)?
            .json()
            .await
            .map_err(reqwest_middleware::Error::Reqwest)?;
        Ok(vuln)
    }

    /// Convert an OSV Vulnerability record to a Finding.
    fn vulnerability_to_finding(
        dependency: &types::Dependency,
        vuln: Vulnerability,
    ) -> types::Finding {
        // Extract a link for the advisory. We prefer the first
        // `ADVISORY` reference, then the first `WEB` reference, and then
        // finally we synthesize a URL of `https://osv.dev/vulnerability/<id>`
        // where `<id>` is the vulnerability's ID.
        let link = vuln
            .references
            .as_ref()
            .and_then(|references| {
                references
                    .iter()
                    .find(|reference| matches!(reference.reference_type, ReferenceType::Advisory))
                    .or_else(|| {
                        references.iter().find(|reference| {
                            matches!(reference.reference_type, ReferenceType::Web)
                        })
                    })
                    .map(|reference| reference.url.clone())
            })
            .unwrap_or_else(|| {
                DisplaySafeUrl::parse(&format!("https://osv.dev/vulnerability/{}", vuln.id))
                    .expect("impossible: synthesized URL is invalid")
            });

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
            .map(types::VulnerabilityID::new)
            .collect();

        types::Finding::Vulnerability(
            types::Vulnerability::new(
                dependency.clone(),
                types::VulnerabilityID::new(vuln.id),
                vuln.summary,
                vuln.details,
                Some(link),
                fix_versions,
                aliases,
                vuln.published,
                Some(vuln.modified),
            )
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use reqwest_middleware::ClientWithMiddleware;
    use serde_json::json;
    use uv_configuration::Concurrency;
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

    /// Ensure that `query_batch` returns the correct findings for a batch of dependencies
    /// with no pagination (simple case).
    #[tokio::test]
    async fn test_query_batch_basic() {
        let server = MockServer::start().await;

        // Querybatch request for both packages.
        Mock::given(method("POST"))
            .and(path("/v1/querybatch"))
            .and(body_json(json!({
                "queries": [
                    {
                        "package": { "name": "package-a", "ecosystem": "PyPI" },
                        "version": "1.0.0",
                    },
                    {
                        "package": { "name": "package-b", "ecosystem": "PyPI" },
                        "version": "2.0.0",
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    { "vulns": [{ "id": "VULN-1", "modified": "2026-01-01T00:00:00Z" }] },
                    { "vulns": [{ "id": "VULN-2", "modified": "2026-01-02T00:00:00Z" }] }
                ]
            })))
            .mount(&server)
            .await;

        // Individual vuln detail requests.
        Mock::given(method("GET"))
            .and(path("/v1/vulns/VULN-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "VULN-1",
                "modified": "2026-01-01T00:00:00Z",
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/vulns/VULN-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "VULN-2",
                "modified": "2026-01-02T00:00:00Z",
            })))
            .mount(&server)
            .await;

        let osv = Osv::new(
            ClientWithMiddleware::default(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
        );

        let dependencies = vec![
            Dependency::new(
                PackageName::from_str("package-a").unwrap(),
                Version::from_str("1.0.0").unwrap(),
            ),
            Dependency::new(
                PackageName::from_str("package-b").unwrap(),
                Version::from_str("2.0.0").unwrap(),
            ),
        ];

        let findings = osv
            .query_batch(&dependencies)
            .await
            .expect("Failed to query batch");

        insta::assert_debug_snapshot!(findings, @r#"
        [
            Vulnerability(
                Vulnerability {
                    dependency: Dependency {
                        name: PackageName(
                            "package-a",
                        ),
                        version: "1.0.0",
                    },
                    id: VulnerabilityID(
                        "VULN-1",
                    ),
                    summary: None,
                    description: None,
                    link: Some(
                        DisplaySafeUrl {
                            scheme: "https",
                            cannot_be_a_base: false,
                            username: "",
                            password: None,
                            host: Some(
                                Domain(
                                    "osv.dev",
                                ),
                            ),
                            port: None,
                            path: "/vulnerability/VULN-1",
                            query: None,
                            fragment: None,
                        },
                    ),
                    fix_versions: [],
                    aliases: [],
                    published: None,
                    modified: Some(
                        2026-01-01T00:00:00Z,
                    ),
                },
            ),
            Vulnerability(
                Vulnerability {
                    dependency: Dependency {
                        name: PackageName(
                            "package-b",
                        ),
                        version: "2.0.0",
                    },
                    id: VulnerabilityID(
                        "VULN-2",
                    ),
                    summary: None,
                    description: None,
                    link: Some(
                        DisplaySafeUrl {
                            scheme: "https",
                            cannot_be_a_base: false,
                            username: "",
                            password: None,
                            host: Some(
                                Domain(
                                    "osv.dev",
                                ),
                            ),
                            port: None,
                            path: "/vulnerability/VULN-2",
                            query: None,
                            fragment: None,
                        },
                    ),
                    fix_versions: [],
                    aliases: [],
                    published: None,
                    modified: Some(
                        2026-01-02T00:00:00Z,
                    ),
                },
            ),
        ]
        "#);

        // 1 querybatch + 2 vuln detail fetches.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            3,
            "Expected one querybatch request and two vuln detail requests"
        );
    }

    /// Ensure that `query_batch` correctly handles pagination: only the deps whose results
    /// included a `next_page_token` are re-queried, with their respective tokens.
    #[tokio::test]
    async fn test_query_batch_pagination() {
        let server = MockServer::start().await;

        // First querybatch request: both packages, no page tokens.
        Mock::given(method("POST"))
            .and(path("/v1/querybatch"))
            .and(body_json(json!({
                "queries": [
                    {
                        "package": { "name": "package-a", "ecosystem": "PyPI" },
                        "version": "1.0.0",
                    },
                    {
                        "package": { "name": "package-b", "ecosystem": "PyPI" },
                        "version": "2.0.0",
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    {
                        "vulns": [{ "id": "VULN-1", "modified": "2026-01-01T00:00:00Z" }],
                        "next_page_token": "tok1"
                    },
                    {
                        "vulns": [{ "id": "VULN-2", "modified": "2026-01-02T00:00:00Z" }]
                    }
                ]
            })))
            .mount(&server)
            .await;

        // Second querybatch request: only package-a with page token.
        Mock::given(method("POST"))
            .and(path("/v1/querybatch"))
            .and(body_json(json!({
                "queries": [
                    {
                        "package": { "name": "package-a", "ecosystem": "PyPI" },
                        "version": "1.0.0",
                        "page_token": "tok1",
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    { "vulns": [{ "id": "VULN-3", "modified": "2026-01-03T00:00:00Z" }] }
                ]
            })))
            .mount(&server)
            .await;

        // Individual vuln detail requests.
        Mock::given(method("GET"))
            .and(path("/v1/vulns/VULN-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "VULN-1",
                "modified": "2026-01-01T00:00:00Z",
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/vulns/VULN-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "VULN-2",
                "modified": "2026-01-02T00:00:00Z",
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/vulns/VULN-3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "VULN-3",
                "modified": "2026-01-03T00:00:00Z",
            })))
            .mount(&server)
            .await;

        let osv = Osv::new(
            ClientWithMiddleware::default(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
        );

        let dependencies = vec![
            Dependency::new(
                PackageName::from_str("package-a").unwrap(),
                Version::from_str("1.0.0").unwrap(),
            ),
            Dependency::new(
                PackageName::from_str("package-b").unwrap(),
                Version::from_str("2.0.0").unwrap(),
            ),
        ];

        let findings = osv
            .query_batch(&dependencies)
            .await
            .expect("Failed to query batch");

        // package-a has VULN-1 (page 1) and VULN-3 (page 2); package-b has VULN-2.
        assert_eq!(findings.len(), 3);

        let mut ids: Vec<&str> = findings
            .iter()
            .map(|f| match f {
                Finding::Vulnerability(v) => v.id.as_str(),
                Finding::ProjectStatus(_) => unreachable!(),
            })
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, ["VULN-1", "VULN-2", "VULN-3"]);

        // 2 querybatch requests + 3 vuln detail fetches.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            5,
            "Expected two querybatch requests and three vuln detail requests"
        );
    }
}
