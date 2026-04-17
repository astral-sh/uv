//! Types and interfaces for interacting with [OSV] as a vulnerability service.
//!
//! We use OSV's `/v1/querybatch` endpoint to collect vulnerability IDs for all
//! dependencies in a single round-trip (handling pagination as needed), then
//! fetch full vulnerability records from `/v1/vulns/{id}` concurrently.
//!
//! [OSV]: https://osv.dev/

use std::path::Path;
use std::str::FromStr as _;
use std::sync::LazyLock;
use std::time::Duration;

use indexmap::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

use crate::types::{self, VulnerabilityID};
use futures::{StreamExt as _, TryStreamExt as _};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_client::{CacheControl, CachedClient, CachedClientError};
use uv_configuration::Concurrency;
use uv_pep440::Version;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

pub static API_BASE: LazyLock<DisplaySafeUrl> = LazyLock::new(|| {
    DisplaySafeUrl::parse("https://api.osv.dev/").expect("impossible: embedded URL is invalid")
});

/// Errors during OSV service interactions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error from the cached HTTP client.
    #[error(transparent)]
    Client(#[from] uv_client::Error),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Range {
    #[serde(rename = "type")]
    range_type: RangeType,
    events: Vec<Event>,
}

/// Package affected by a vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Affected {
    ranges: Option<Vec<Range>>,
    // TODO: Enable these fields if/when they contain information that's
    // useful to us, e.g. metadata that constrains a vulnerability to specific
    // Python runtime versions, specific distributions of a version, etc.
    // ecosystem_specific: Option<serde_json::Value>,
    // database_specific: Option<serde_json::Value>,
}

/// The type of a reference in an OSV vulnerability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Reference {
    #[serde(rename = "type")]
    reference_type: ReferenceType,
    url: DisplaySafeUrl,
}

/// A full vulnerability record from OSV.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Filter for OSV queries.
#[derive(Debug, Copy, Clone)]
pub enum Filter {
    /// Return all vulnerabilities.
    All,
    /// Return only vulnerabilities matching the `MAL-` prefix.
    Malware,
}

impl Filter {
    /// Returns `true` if the given vulnerability ID matches this filter.
    fn matches(self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Malware => id.starts_with("MAL-"),
        }
    }
}

/// Maximum age for cached batch query results (vulnerability IDs per package).
const QUERY_CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 60);

/// Synthetic `Cache-Control` header for vulnerability record caching (1 hour).
///
/// This is injected into responses from OSV (which sends no cache headers)
/// so that the [`CachedClient`] middleware handles caching transparently.
static VULN_CACHE_CONTROL: LazyLock<http::HeaderValue> =
    LazyLock::new(|| "max-age=3600".parse().expect("valid header value"));

/// Returns `true` if the file at `path` exists and was modified within `max_age`.
fn is_cache_fresh(path: &Path, max_age: Duration) -> bool {
    path.metadata()
        .and_then(|m| m.modified())
        .map(|mtime| mtime.elapsed().is_ok_and(|age| age < max_age))
        .unwrap_or(false)
}

/// Represents [OSV](https://osv.dev/), an open-source vulnerability database.
pub struct Osv {
    base_url: DisplaySafeUrl,
    client: CachedClient,
    concurrency: Concurrency,
    cache: Cache,
}

impl Osv {
    /// Create a new OSV client with the given cached HTTP client and optional base URL.
    ///
    /// If no base URL is provided, the client will default to the official OSV API endpoint.
    /// Positive batch query results are cached to disk. Individual vulnerability records
    /// are cached transparently by the [`CachedClient`].
    pub fn new(
        client: CachedClient,
        base_url: Option<DisplaySafeUrl>,
        concurrency: Concurrency,
        cache: Cache,
    ) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| API_BASE.clone()),
            client,
            concurrency,
            cache,
        }
    }

    /// Return a [`CacheEntry`] for batch query results for a given package and version.
    fn query_cache_entry(&self, name: &str, version: &str) -> CacheEntry {
        let bucket = self.cache.bucket(CacheBucket::Audit);
        CacheEntry::new(
            bucket.join("osv").join("query").join(name),
            format!("{version}.msgpack"),
        )
    }

    /// Return a [`CacheEntry`] for a full vulnerability record.
    fn vuln_cache_entry(&self, id: &str) -> CacheEntry {
        let bucket = self.cache.bucket(CacheBucket::Audit);
        CacheEntry::new(bucket.join("osv").join("vulns"), format!("{id}.msgpack"))
    }

    /// Read cached vulnerability IDs for a package, if fresh.
    ///
    /// Batch query results use manual caching with a TTL check because the
    /// [`CachedClient`] middleware does not support POST requests.
    fn read_cached_query_ids(&self, name: &str, version: &str) -> Option<Vec<String>> {
        let entry = self.query_cache_entry(name, version);
        if !is_cache_fresh(entry.path(), QUERY_CACHE_MAX_AGE) {
            return None;
        }
        let data = fs_err::read(entry.path()).ok()?;
        rmp_serde::from_slice::<Vec<String>>(&data).ok()
    }

    /// Write vulnerability IDs to cache for a package. Only called for non-empty results.
    fn write_cached_query_ids(&self, name: &str, version: &str, ids: &[String]) {
        let entry = self.query_cache_entry(name, version);
        if let Err(err) = fs_err::create_dir_all(entry.dir()) {
            trace!(
                "Failed to create cache directory {}: {err}",
                entry.dir().display()
            );
            return;
        }
        let Ok(data) = rmp_serde::to_vec(ids) else {
            return;
        };
        if let Err(err) = uv_fs::write_atomic_sync(entry.path(), &data) {
            trace!(
                "Failed to write query cache {}: {err}",
                entry.path().display()
            );
        }
    }

    /// Query OSV for vulnerabilities affecting the given dependencies, returning only vulnerability IDs.
    ///
    /// Returns a mapping from each input dependency to the set of vulnerability IDs affecting it.
    pub async fn query_identifiers<'a>(
        &self,
        dependencies: &'a [types::Dependency],
        filter: Filter,
    ) -> Result<IndexMap<&'a types::Dependency, FxHashSet<VulnerabilityID>>, Error> {
        if dependencies.is_empty() {
            return Ok(IndexMap::default());
        }

        let mut result_map: IndexMap<&types::Dependency, FxHashSet<VulnerabilityID>> =
            IndexMap::default();

        // Check cache for each dependency, splitting into hits and misses.
        // Cache stores unfiltered IDs so results are reusable across filter modes.
        let mut pending: Vec<(&types::Dependency, Option<String>)> = Vec::new();
        for dep in dependencies {
            if let Some(cached_ids) =
                self.read_cached_query_ids(dep.name().as_ref(), &dep.version().to_string())
            {
                trace!(
                    "Cache hit for {name}=={version} ({n} vuln IDs)",
                    name = dep.name(),
                    version = dep.version(),
                    n = cached_ids.len(),
                );
                let ids: FxHashSet<VulnerabilityID> = cached_ids
                    .into_iter()
                    .filter(|id| filter.matches(id))
                    .map(VulnerabilityID::new)
                    .collect();
                result_map.insert(dep, ids);
            } else {
                pending.push((dep, None));
            }
        }

        // Query OSV for cache misses only, using the uncached client for POST.
        if !pending.is_empty() {
            // Track unfiltered IDs per dependency for caching.
            let mut unfiltered_ids: FxHashMap<&types::Dependency, Vec<String>> =
                FxHashMap::default();

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
                    .uncached()
                    .for_host(&url)
                    .raw_client()
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
                for ((dep, _), batch_result) in pending.iter().zip(batch_response.results.iter()) {
                    // Collect unfiltered IDs for caching.
                    let raw_ids = unfiltered_ids.entry(dep).or_default();
                    raw_ids.extend(batch_result.vulns.iter().map(|v| v.id.clone()));

                    // Collect filtered IDs for the result.
                    let ids = result_map.entry(dep).or_default();
                    ids.extend(
                        batch_result
                            .vulns
                            .iter()
                            .filter(|v| filter.matches(&v.id))
                            .map(|v| VulnerabilityID::new(v.id.clone())),
                    );
                    if let Some(token) = &batch_result.next_page_token {
                        next_pending.push((*dep, Some(token.clone())));
                    }
                }

                if next_pending.is_empty() {
                    break;
                }
                pending = next_pending;
            }

            // Cache positive results (non-empty vuln ID sets) for future lookups.
            for (dep, ids) in &unfiltered_ids {
                if !ids.is_empty() {
                    self.write_cached_query_ids(
                        dep.name().as_ref(),
                        &dep.version().to_string(),
                        ids,
                    );
                }
            }
        }

        Ok(result_map)
    }

    /// Query OSV for vulnerabilities affecting the given dependencies, returning full vulnerability records.
    pub async fn query_batch(
        &self,
        dependencies: &[types::Dependency],
        filter: Filter,
    ) -> Result<Vec<types::Finding>, Error> {
        let dep_vuln_ids = self.query_identifiers(dependencies, filter).await?;

        // Collect unique vuln IDs to minimize fetches.
        let unique_ids: FxHashSet<_> = dep_vuln_ids
            .values()
            .flat_map(|ids| ids.iter())
            .cloned()
            .collect();

        // Fetch full vulnerability records concurrently.
        let vuln_details = futures::stream::iter(unique_ids)
            .map(async |id| {
                let vuln = self.fetch_vuln(id.as_str()).await?;
                Ok::<(VulnerabilityID, Vulnerability), Error>((id, vuln))
            })
            .buffer_unordered(self.concurrency.downloads)
            .try_collect::<FxHashMap<VulnerabilityID, Vulnerability>>()
            .await?;

        // Build findings in dependency order (preserved by IndexMap).
        let mut findings = Vec::new();
        for (dep, vuln_ids) in &dep_vuln_ids {
            for vuln_id in vuln_ids {
                if let Some(vuln) = vuln_details.get(vuln_id) {
                    findings.push(Self::vulnerability_to_finding(dep, vuln.clone()));
                }
            }
        }

        Ok(findings)
    }

    /// Fetch a full vulnerability record by ID from OSV.
    ///
    /// Caching is handled transparently by the [`CachedClient`] middleware using
    /// a synthetic `Cache-Control: max-age=3600` header, since OSV itself does
    /// not send caching headers.
    async fn fetch_vuln(&self, id: &str) -> Result<Vulnerability, Error> {
        let url = self
            .base_url
            .join(&format!("v1/vulns/{id}"))
            .map_err(|e| Error::Url(self.base_url.clone(), e))?;

        let cache_entry = self.vuln_cache_entry(id);
        let req = self
            .client
            .uncached()
            .for_host(&url)
            .raw_client()
            .get(url.as_ref())
            .build()
            .map_err(reqwest_middleware::Error::Reqwest)?;

        let vuln: Vulnerability = self
            .client
            .get_serde(
                req,
                &cache_entry,
                CacheControl::Override(VULN_CACHE_CONTROL.clone()),
                async |response| response.json::<Vulnerability>().await,
            )
            .await
            .map_err(|err| match err {
                CachedClientError::Client(err) => Error::Client(err),
                CachedClientError::Callback { err, .. } => {
                    Error::ReqwestMiddleware(reqwest_middleware::Error::Reqwest(err))
                }
            })?;

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

    use serde_json::json;
    use uv_cache::Cache;
    use uv_client::{BaseClientBuilder, CachedClient};
    use uv_configuration::Concurrency;
    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_redacted::DisplaySafeUrl;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::service::osv::{Filter, RangeType};
    use crate::types::{Dependency, Finding};

    use super::Event;
    use super::Osv;

    /// Create a [`CachedClient`] suitable for tests (no retries, no cache).
    fn test_client() -> CachedClient {
        CachedClient::new(
            BaseClientBuilder::default()
                .build()
                .expect("Failed to build test client"),
        )
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

    /// Ensure that `query_identifiers` returns the correct vulnerability ID mapping.
    #[tokio::test]
    async fn test_query_identifiers() {
        let server = MockServer::start().await;

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
                    { "vulns": [
                        { "id": "VULN-1", "modified": "2026-01-01T00:00:00Z" },
                        { "id": "VULN-3", "modified": "2026-01-03T00:00:00Z" }
                    ] },
                    { "vulns": [
                        { "id": "VULN-2", "modified": "2026-01-02T00:00:00Z" }
                    ] }
                ]
            })))
            .mount(&server)
            .await;

        let osv = Osv::new(
            test_client(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
            Cache::temp().unwrap(),
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

        let identifiers = osv
            .query_identifiers(&dependencies, Filter::All)
            .await
            .expect("Failed to query identifiers");

        // package-a should have VULN-1 and VULN-3.
        let pkg_a_ids = identifiers.get(&dependencies[0]).unwrap();
        let mut pkg_a_sorted: Vec<_> = pkg_a_ids
            .iter()
            .map(crate::types::VulnerabilityID::as_str)
            .collect();
        pkg_a_sorted.sort_unstable();
        assert_eq!(pkg_a_sorted, ["VULN-1", "VULN-3"]);

        // package-b should have VULN-2.
        let pkg_b_ids = identifiers.get(&dependencies[1]).unwrap();
        let pkg_b_sorted: Vec<_> = pkg_b_ids
            .iter()
            .map(crate::types::VulnerabilityID::as_str)
            .collect();
        assert_eq!(pkg_b_sorted, ["VULN-2"]);

        // Only 1 querybatch request, no vuln detail fetches.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "Expected one querybatch request"
        );
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
            test_client(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
            Cache::temp().unwrap(),
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
            .query_batch(&dependencies, Filter::All)
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
            test_client(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
            Cache::temp().unwrap(),
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
            .query_batch(&dependencies, Filter::All)
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

    /// Ensure that `query_batch` with `Filter::Malware` only fetches full records for `MAL-`
    /// prefixed vulnerability IDs, skipping non-malware vulnerabilities entirely.
    #[tokio::test]
    async fn test_query_batch_malware_filter() {
        let server = MockServer::start().await;

        // Querybatch returns both a MAL- and a non-MAL vulnerability.
        Mock::given(method("POST"))
            .and(path("/v1/querybatch"))
            .and(body_json(json!({
                "queries": [
                    {
                        "package": { "name": "package-a", "ecosystem": "PyPI" },
                        "version": "1.0.0",
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    {
                        "vulns": [
                            { "id": "MAL-2026-1234", "modified": "2026-01-01T00:00:00Z" },
                            { "id": "GHSA-xxxx-yyyy", "modified": "2026-01-02T00:00:00Z" }
                        ]
                    }
                ]
            })))
            .mount(&server)
            .await;

        // Only the MAL- vuln should be fetched.
        Mock::given(method("GET"))
            .and(path("/v1/vulns/MAL-2026-1234"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "MAL-2026-1234",
                "modified": "2026-01-01T00:00:00Z",
            })))
            .mount(&server)
            .await;

        let osv = Osv::new(
            test_client(),
            Some(DisplaySafeUrl::parse(&server.uri()).unwrap()),
            Concurrency::default(),
            Cache::temp().unwrap(),
        );

        let dependencies = vec![Dependency::new(
            PackageName::from_str("package-a").unwrap(),
            Version::from_str("1.0.0").unwrap(),
        )];

        let findings = osv
            .query_batch(&dependencies, Filter::Malware)
            .await
            .expect("Failed to query batch");

        let [Finding::Vulnerability(v)] = findings.as_slice() else {
            panic!("Expected exactly one vulnerability finding");
        };

        assert_eq!(v.id.as_str(), "MAL-2026-1234");

        // 1 querybatch + 1 vuln detail fetch (GHSA- was skipped).
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "Expected one querybatch request and one vuln detail request (non-MAL skipped)"
        );
    }
}
