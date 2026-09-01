use std::borrow::Cow;
use std::sync::Arc;

use http::{HeaderValue, StatusCode};
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;
use uv_auth::RealmRef;
use uv_normalize::PackageName;
#[cfg(test)]
use uv_pep508::VerbatimUrl;
use uv_redacted::DisplaySafeUrl;

use crate::index_url::PYPI_ARTIFACT_BASE_URL;
use crate::{
    CanonicalArtifactUrl, File, Index, IndexFormat, IndexLocations, IndexName,
    IndexStatusCodeStrategy, IndexUrl, RegistryFile, ToUrlError,
};

/// An invalid proxy index configuration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProxyIndexConfigError {
    /// The proxy references a canonical Simple API index that is not configured.
    #[error(
        "The proxy index references package index `{name}`, which was not found in your configuration"
    )]
    MissingCanonicalIndex {
        /// The name of the missing canonical package index.
        name: IndexName,
    },

    /// A proxy index has the same name as another configured index.
    #[error("Duplicate index name `{name}`: proxy index `{proxy}` conflicts with index `{index}`")]
    DuplicateIndexName {
        /// The name used by both the proxy and another index.
        name: IndexName,
        /// The proxy index URL, with credentials redacted when displayed.
        proxy: Box<DisplaySafeUrl>,
        /// The conflicting index URL, with credentials redacted when displayed.
        index: Box<DisplaySafeUrl>,
    },

    /// More than one proxy references the same canonical package index.
    #[error(
        "Each index can have only one proxy, but both `{first_proxy}` and `{second_proxy}` are proxies for `{index}`"
    )]
    DuplicateCanonicalIndex {
        /// The canonical package index with multiple proxy declarations.
        index: Box<DisplaySafeUrl>,
        /// The first proxy index URL, with credentials redacted when displayed.
        first_proxy: Box<DisplaySafeUrl>,
        /// The conflicting proxy index URL, with credentials redacted when displayed.
        second_proxy: Box<DisplaySafeUrl>,
    },

    /// A physical proxy must explicitly identify where its artifacts are hosted.
    #[error(
        "Proxy indexes require an `artifact-base-url`, but `{index}` does not have one configured"
    )]
    MissingProxyArtifactBase {
        /// The physical proxy index without a configured artifact base.
        index: Box<DisplaySafeUrl>,
    },

    /// A non-PyPI canonical index must explicitly identify its artifact host.
    #[error(
        "Non-PyPI indexes require an `artifact-base-url` when proxied, but `{index}` does not have one configured"
    )]
    MissingCanonicalArtifactBase {
        /// The canonical package index without a configured artifact base.
        index: Box<DisplaySafeUrl>,
    },

    /// An index or artifact base is not a safe absolute HTTP(S) URL prefix.
    #[error("Invalid proxy URL mapping `{url}`: {reason}")]
    InvalidMapping {
        /// The invalid canonical or physical URL.
        url: Box<DisplaySafeUrl>,
        /// The reason the URL cannot safely be used as a reversible prefix.
        reason: &'static str,
    },
}

/// An error routing an artifact through a configured proxy index.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProxyIndexError {
    /// A selected proxied artifact has no supported hash.
    #[error(
        "Cannot lock `{filename}` for `{package}` from proxy index `{physical}` because it has no supported hash"
    )]
    MissingHash {
        /// The package containing the selected artifact.
        package: PackageName,
        /// The selected wheel or source distribution filename.
        filename: String,
        /// The credential-redacted physical proxy endpoint.
        physical: Box<DisplaySafeUrl>,
    },

    /// A proxied artifact URL does not match the configured artifact base.
    #[error("No proxy artifact URL mapping matches `{url}`")]
    UnmappedUrl {
        /// The unmatched canonical or physical artifact URL.
        url: Box<DisplaySafeUrl>,
    },

    /// A registry artifact location could not be converted to an absolute URL.
    #[error(transparent)]
    InvalidArtifactUrl(#[from] ToUrlError),
}

/// A URL that is safe to use as a reversible route prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UrlPrefix(DisplaySafeUrl);

impl UrlPrefix {
    fn new(url: DisplaySafeUrl) -> Result<Self, ProxyIndexConfigError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ProxyIndexConfigError::InvalidMapping {
                url: Box::new(url),
                reason: "index and artifact base URLs must use HTTP or HTTPS",
            });
        }

        if url.query().is_some() || url.fragment().is_some() {
            return Err(ProxyIndexConfigError::InvalidMapping {
                url: Box::new(url),
                reason: "index and artifact base URLs cannot contain queries or fragments",
            });
        }

        for segment in url.path().split('/') {
            let Ok(decoded) = percent_encoding::percent_decode_str(segment).decode_utf8() else {
                return Err(ProxyIndexConfigError::InvalidMapping {
                    url: Box::new(url),
                    reason: "index and artifact base URLs must contain valid UTF-8 path segments",
                });
            };

            if matches!(decoded.as_ref(), "." | "..")
                || decoded.contains('/')
                || decoded.contains('\\')
                || decoded.contains('\0')
            {
                return Err(ProxyIndexConfigError::InvalidMapping {
                    url: Box::new(url),
                    reason: "index and artifact base URLs cannot contain path traversal or encoded separators",
                });
            }
        }

        Ok(Self(url))
    }

    fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    fn path_suffix<'a>(&self, url: &'a DisplaySafeUrl) -> Option<&'a str> {
        if RealmRef::from(&**url) != RealmRef::from(&*self.0) {
            return None;
        }

        let prefix_path = self.0.path().trim_end_matches('/');
        let candidate_path = url.path();
        if candidate_path == prefix_path {
            return Some("");
        }

        candidate_path
            .strip_prefix(prefix_path)
            .and_then(|suffix| suffix.strip_prefix('/'))
    }
}

/// A package index URL that is safe to use as a route prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexUrlPrefix(IndexUrl);

impl IndexUrlPrefix {
    fn new(url: IndexUrl) -> Result<Self, ProxyIndexConfigError> {
        UrlPrefix::new(url.url().clone())?;
        Ok(Self(url))
    }

    fn as_index_url(&self) -> &IndexUrl {
        &self.0
    }
}

/// An artifact URL prefix safe to use when constructing canonical lockfile URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalArtifactUrlPrefix(UrlPrefix);

impl CanonicalArtifactUrlPrefix {
    fn new(url: DisplaySafeUrl) -> Result<Self, ProxyIndexConfigError> {
        let prefix = UrlPrefix::new(url)?;
        if !prefix.as_url().username().is_empty() || prefix.as_url().password().is_some() {
            return Err(ProxyIndexConfigError::InvalidMapping {
                url: Box::new(prefix.0),
                reason: "canonical URL prefixes cannot contain credentials",
            });
        }

        Ok(Self(prefix))
    }

    fn as_prefix(&self) -> &UrlPrefix {
        &self.0
    }
}

/// The original and proxy base URLs for package downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtifactUrlMapping {
    canonical: CanonicalArtifactUrlPrefix,
    proxy: UrlPrefix,
}

impl ArtifactUrlMapping {
    fn new(canonical: CanonicalArtifactUrlPrefix, proxy: UrlPrefix) -> Self {
        Self { canonical, proxy }
    }

    /// Translate a canonical artifact URL to its configured proxy URL.
    fn to_proxy(&self, url: &CanonicalArtifactUrl) -> Result<DisplaySafeUrl, ProxyIndexError> {
        Self::rewrite(&url.to_url()?, self.canonical.as_prefix(), &self.proxy)
    }

    /// Translate a proxy artifact URL to its configured canonical URL.
    fn to_canonical(&self, url: &DisplaySafeUrl) -> Result<DisplaySafeUrl, ProxyIndexError> {
        Self::rewrite(url, &self.proxy, self.canonical.as_prefix())
    }

    fn rewrite(
        url: &DisplaySafeUrl,
        prefix: &UrlPrefix,
        target: &UrlPrefix,
    ) -> Result<DisplaySafeUrl, ProxyIndexError> {
        let Some(suffix) = prefix.path_suffix(url) else {
            return Err(ProxyIndexError::UnmappedUrl {
                url: Box::new(url.clone()),
            });
        };

        let mut rewritten = target.as_url().clone();
        let target_path = target.as_url().path().trim_end_matches('/');
        if suffix.is_empty() {
            if prefix.as_url().path().ends_with('/') && !target.as_url().path().ends_with('/') {
                rewritten.set_path(&format!("{target_path}/"));
            }
        } else {
            rewritten.set_path(&format!("{target_path}/{suffix}"));
        }
        rewritten.set_query(url.query());
        rewritten.set_fragment(url.fragment());
        Ok(rewritten)
    }
}

/// A validated route from a canonical package index through a configured proxy.
///
/// Unlike [`IndexRoute`], this always has a proxy endpoint, artifact URL mapping, and request policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyRoute {
    canonical: IndexUrlPrefix,
    url: IndexUrlPrefix,
    artifact_mapping: ArtifactUrlMapping,
    request_policy: IndexRequestPolicy,
}

impl ProxyRoute {
    fn new(
        canonical: IndexUrlPrefix,
        url: IndexUrlPrefix,
        artifact_mapping: ArtifactUrlMapping,
        request_policy: IndexRequestPolicy,
    ) -> Self {
        Self {
            canonical,
            url,
            artifact_mapping,
            request_policy,
        }
    }

    /// Return the proxy index URL used for requests, authentication, and caches.
    pub fn effective_url(&self) -> &IndexUrl {
        self.url.as_index_url()
    }

    /// Translate a canonical artifact URL to its configured proxy URL.
    pub fn artifact_url_for_request(
        &self,
        canonical_url: &CanonicalArtifactUrl,
    ) -> Result<DisplaySafeUrl, ProxyIndexError> {
        self.artifact_mapping.to_proxy(canonical_url)
    }

    fn canonicalize_file(&self, file: File) -> Result<RegistryFile, ProxyIndexError> {
        let effective_url = file.url.to_url()?;
        let canonical_url = self.artifact_mapping.to_canonical(&effective_url)?;
        Ok(file.map_url(|_| CanonicalArtifactUrl::from_url(canonical_url)))
    }
}

/// The request policy for an effective package index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexRequestPolicy {
    status_code_strategy: IndexStatusCodeStrategy,
    ignored_error_codes: FxHashSet<StatusCode>,
    simple_api_cache_control: Option<HeaderValue>,
    artifact_cache_control: Option<HeaderValue>,
}

impl From<&Index> for IndexRequestPolicy {
    fn from(index: &Index) -> Self {
        Self {
            status_code_strategy: index.status_code_strategy(),
            ignored_error_codes: index
                .ignore_error_codes
                .iter()
                .flatten()
                .map(|status_code| **status_code)
                .collect(),
            simple_api_cache_control: index.simple_api_cache_control(),
            artifact_cache_control: index.artifact_cache_control(),
        }
    }
}

/// A validated route from a canonical package index to the index used for requests.
///
/// Without a configured proxy, requests use the canonical index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRoute {
    /// The original index URL recorded in the lockfile.
    pub canonical: IndexUrl,
    proxy: Option<Arc<ProxyRoute>>,
}

impl IndexRoute {
    /// Return whether this index is routed through a configured proxy.
    #[cfg(test)]
    fn is_proxy(&self) -> bool {
        self.proxy.is_some()
    }

    /// Resolve a Simple API file into the canonical namespace used for identity and persistence.
    ///
    /// On a proxy route, relative locations are resolved against the Simple API response URL before
    /// translation. The returned file cannot be confused with unresolved response metadata.
    pub fn canonicalize_file(&self, file: File) -> Result<RegistryFile, ProxyIndexError> {
        if let Some(proxy) = &self.proxy {
            proxy.canonicalize_file(file)
        } else {
            Ok(file.map_url(CanonicalArtifactUrl::from_location))
        }
    }
}

impl IndexLocations {
    /// Borrow the configured [`ProxyRoute`] for a canonical index, if any.
    ///
    /// The route contains the configured canonical URL, which may differ from `index` in spelling
    /// or credentials. Use [`Self::route_for`] to retain an owned route with the caller's URL.
    pub fn proxy_route_for(&self, index: &IndexUrl) -> Option<&ProxyRoute> {
        self.find_proxy_route(index).map(AsRef::as_ref)
    }

    fn find_proxy_route(&self, index: &IndexUrl) -> Option<&Arc<ProxyRoute>> {
        self.routes
            .iter()
            .find(|route| route.canonical.as_index_url().is_same_index(index))
    }

    /// Return an owned route that preserves the caller's canonical index URL.
    ///
    /// Prefer [`Self::proxy_route_for`] when the route does not need to outlive these locations.
    pub fn route_for(&self, index: &IndexUrl) -> IndexRoute {
        IndexRoute {
            canonical: index.clone(),
            proxy: self.find_proxy_route(index).cloned(),
        }
    }

    /// Return the configured proxy URL, or the caller's URL for a direct index.
    pub fn effective_url<'a>(&'a self, index: &'a IndexUrl) -> &'a IndexUrl {
        self.proxy_route_for(index)
            .map_or(index, ProxyRoute::effective_url)
    }

    /// Return the status code strategy of the proxy, or of the direct index when no proxy is set.
    pub fn status_code_strategy_for(&self, index: &IndexUrl) -> Cow<'_, IndexStatusCodeStrategy> {
        if let Some(policy) = self.proxy_request_policy(index) {
            Cow::Borrowed(&policy.status_code_strategy)
        } else {
            Cow::Owned(
                self.index_for_url(index)
                    .map(Index::status_code_strategy)
                    .unwrap_or_default(),
            )
        }
    }

    /// Return whether the proxy or direct index explicitly ignores the given status code.
    pub fn ignores_error_code(&self, index: &IndexUrl, status_code: StatusCode) -> bool {
        if let Some(policy) = self.proxy_request_policy(index) {
            policy.ignored_error_codes.contains(&status_code)
        } else {
            self.index_for_url(index)
                .is_some_and(|index| index.ignores_error_code(status_code))
        }
    }

    /// Return the Simple API cache control header of the proxy or direct index, if configured.
    pub fn simple_api_cache_control_for(&self, index: &IndexUrl) -> Option<HeaderValue> {
        if let Some(policy) = self.proxy_request_policy(index) {
            policy.simple_api_cache_control.clone()
        } else {
            self.index_for_url(index)
                .and_then(Index::simple_api_cache_control)
        }
    }

    /// Return the artifact cache control header of the proxy or direct index, if configured.
    pub fn artifact_cache_control_for(&self, index: &IndexUrl) -> Option<HeaderValue> {
        if let Some(policy) = self.proxy_request_policy(index) {
            policy.artifact_cache_control.clone()
        } else {
            self.index_for_url(index)
                .and_then(Index::artifact_cache_control)
        }
    }

    fn proxy_request_policy(&self, index: &IndexUrl) -> Option<&IndexRequestPolicy> {
        self.proxy_route_for(index)
            .map(|proxy| &proxy.request_policy)
    }

    /// Iterate over the configured proxy routes.
    pub fn proxy_routes(&self) -> impl Iterator<Item = &ProxyRoute> {
        self.routes.iter().map(AsRef::as_ref)
    }
}

pub(crate) fn build_routes(
    locations: &IndexLocations,
) -> Result<Vec<Arc<ProxyRoute>>, ProxyIndexConfigError> {
    let mut indexes_by_name: FxHashMap<_, _> = locations
        .configured_indexes()
        .filter_map(|index| index.name.as_ref().map(|name| (name, index.raw_url())))
        .collect();
    let mut routes: Vec<Arc<ProxyRoute>> = Vec::new();

    for proxy in locations.proxy_indexes() {
        let Some(canonical_name) = &proxy.proxy_for else {
            continue;
        };

        if proxy.format != IndexFormat::Simple {
            return Err(ProxyIndexConfigError::InvalidMapping {
                url: Box::new(proxy.url.url().clone()),
                reason: "proxy indexes must use the Simple API format",
            });
        }

        let canonical = find_canonical_index(locations, canonical_name).ok_or_else(|| {
            ProxyIndexConfigError::MissingCanonicalIndex {
                name: canonical_name.clone(),
            }
        })?;

        if let Some(route) = routes.iter().find(|route| {
            route
                .canonical
                .as_index_url()
                .is_same_index(canonical.url())
        }) {
            return Err(ProxyIndexConfigError::DuplicateCanonicalIndex {
                index: Box::new(canonical.url.url().clone()),
                first_proxy: Box::new(route.effective_url().url().clone()),
                second_proxy: Box::new(proxy.url.url().clone()),
            });
        }

        if let Some(name) = proxy.name.as_ref()
            && let Some(index) = indexes_by_name.insert(name, proxy.raw_url())
        {
            return Err(ProxyIndexConfigError::DuplicateIndexName {
                name: name.clone(),
                proxy: Box::new(proxy.raw_url().clone()),
                index: Box::new(index.clone()),
            });
        }

        let canonical_url = IndexUrlPrefix::new(canonical.url.clone())?;
        let proxy_url = IndexUrlPrefix::new(proxy.url.clone())?;
        let physical_url = proxy_url.as_index_url().url();

        let canonical_artifact_base = CanonicalArtifactUrlPrefix::new(artifact_base(canonical)?)?;
        let proxy_artifact_base = proxy.artifact_base_url.clone().ok_or_else(|| {
            ProxyIndexConfigError::MissingProxyArtifactBase {
                index: Box::new(physical_url.clone()),
            }
        })?;
        let proxy_artifact_base = UrlPrefix::new(proxy_artifact_base)?;
        let artifact_mapping =
            ArtifactUrlMapping::new(canonical_artifact_base, proxy_artifact_base);
        routes.push(Arc::new(ProxyRoute::new(
            canonical_url,
            proxy_url,
            artifact_mapping,
            IndexRequestPolicy::from(proxy),
        )));
    }

    Ok(routes)
}

fn find_canonical_index<'a>(locations: &'a IndexLocations, name: &IndexName) -> Option<&'a Index> {
    locations
        .configured_indexes()
        .find(|index| index.format == IndexFormat::Simple && index.name.as_ref() == Some(name))
        .or_else(|| {
            if name.as_ref() != "pypi" {
                return None;
            }

            locations
                .default_index()
                .filter(|index| index.url().is_pypi())
        })
}

fn artifact_base(index: &Index) -> Result<DisplaySafeUrl, ProxyIndexConfigError> {
    if let Some(base) = &index.artifact_base_url {
        return Ok(base.clone());
    }

    if index.url().is_pypi() {
        return Ok(PYPI_ARTIFACT_BASE_URL.clone());
    }

    Err(ProxyIndexConfigError::MissingCanonicalArtifactBase {
        index: Box::new(index.url.url().clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uv_pypi_types::HashDigests;
    use uv_small_str::SmallString;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn url(value: &str) -> Result<DisplaySafeUrl, Box<dyn std::error::Error>> {
        Ok(DisplaySafeUrl::parse(value)?)
    }

    fn index_url(value: &str) -> Result<IndexUrl, Box<dyn std::error::Error>> {
        Ok(IndexUrl::from(VerbatimUrl::from_url(url(value)?)))
    }

    fn as_canonical(url: DisplaySafeUrl) -> CanonicalArtifactUrl {
        CanonicalArtifactUrl::from_url(url)
    }

    fn response_file(url: DisplaySafeUrl) -> File {
        File {
            dist_info_metadata: false,
            filename: SmallString::from("package.whl"),
            hashes: HashDigests::empty(),
            requires_python: None,
            size: None,
            upload_time_utc_ms: None,
            url: crate::FileLocation::AbsoluteUrl(url.into()),
            yanked: None,
            zstd: None,
        }
    }

    fn named_index(
        name: &str,
        index: &str,
        artifact: Option<&str>,
        proxy_for: Option<&str>,
    ) -> Result<Index, Box<dyn std::error::Error>> {
        let mut index = Index::from_extra_index_url(index_url(index)?);
        index.name = Some(name.parse()?);
        index.artifact_base_url = artifact.map(url).transpose()?;
        index.proxy_for = proxy_for.map(str::parse).transpose()?;
        Ok(index)
    }

    fn pypi_proxy() -> Result<Index, Box<dyn std::error::Error>> {
        named_index(
            "socket",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("pypi"),
        )
    }

    fn pypi_locations(proxy: Index) -> Result<IndexLocations, ProxyIndexConfigError> {
        IndexLocations::new(vec![proxy], Vec::new(), false)
    }

    #[test]
    fn proxy_simple_route_maps_implicit_pypi() -> TestResult {
        let locations = pypi_locations(pypi_proxy()?)?;
        let canonical = index_url("https://caller:secret@pypi.org/simple/")?;
        let route = locations.route_for(&canonical);

        assert!(route.is_proxy());
        assert_eq!(route.canonical, canonical);
        let borrowed = locations
            .proxy_route_for(&canonical)
            .ok_or("missing proxy route")?;
        assert_eq!(
            borrowed.effective_url(),
            &index_url("https://proxy.example.com/simple/")?
        );
        assert_eq!(
            borrowed.canonical.as_index_url(),
            &index_url("https://pypi.org/simple")?
        );
        assert_eq!(locations.proxy_routes().count(), 1);
        Ok(())
    }

    #[test]
    fn proxy_route_uses_physical_request_policy() -> TestResult {
        let canonical = index_url("https://pypi.org/simple")?;
        let mut canonical_index = Index::from_index_url(canonical.clone());
        canonical_index.cache_control = Some(crate::IndexCacheControl {
            api: Some(HeaderValue::from_static("max-age=60")),
            files: Some(HeaderValue::from_static("max-age=120")),
        });
        let mut proxy = pypi_proxy()?;
        proxy.ignore_error_codes = Some(vec![serde_json::from_value(serde_json::json!(401))?]);
        proxy.cache_control = Some(crate::IndexCacheControl {
            api: Some(HeaderValue::from_static("no-cache")),
            files: Some(HeaderValue::from_static("max-age=3600")),
        });

        let locations =
            IndexLocations::new(vec![canonical_index.clone(), proxy], Vec::new(), false)?;

        assert!(locations.ignores_error_code(&canonical, StatusCode::UNAUTHORIZED));
        assert!(!locations.ignores_error_code(&canonical, StatusCode::FORBIDDEN));
        assert!(matches!(
            locations.status_code_strategy_for(&canonical).as_ref(),
            IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes }
                if status_codes.contains(&StatusCode::UNAUTHORIZED)
        ));
        assert_eq!(
            locations.simple_api_cache_control_for(&canonical),
            Some(HeaderValue::from_static("no-cache"))
        );
        assert_eq!(
            locations.artifact_cache_control_for(&canonical),
            Some(HeaderValue::from_static("max-age=3600"))
        );

        // A proxy without overrides must not inherit the canonical index's cache settings.
        let locations =
            IndexLocations::new(vec![canonical_index, pypi_proxy()?], Vec::new(), false)?;
        assert_eq!(locations.simple_api_cache_control_for(&canonical), None);
        assert_eq!(locations.artifact_cache_control_for(&canonical), None);
        Ok(())
    }

    #[test]
    fn request_policy_distinguishes_default_and_explicit_ignored_errors() -> TestResult {
        let pytorch = index_url("https://download.pytorch.org/whl/cu118")?;
        let direct = IndexLocations::new(
            vec![Index::from_index_url(pytorch.clone())],
            Vec::new(),
            false,
        )?;
        let mut proxy = pypi_proxy()?;
        proxy.url = pytorch.clone();
        let proxied = pypi_locations(proxy)?;

        for (locations, index) in [
            (direct, pytorch),
            (proxied, index_url("https://pypi.org/simple")?),
        ] {
            assert!(matches!(
                locations.status_code_strategy_for(&index).as_ref(),
                IndexStatusCodeStrategy::IgnoreErrorCodes { status_codes }
                    if status_codes.contains(&StatusCode::FORBIDDEN)
            ));
            assert!(!locations.ignores_error_code(&index, StatusCode::FORBIDDEN));
        }
        Ok(())
    }

    #[test]
    fn proxy_configuration_is_validated_during_deserialization() -> TestResult {
        let proxy = named_index(
            "socket",
            "https://proxy.example.com/simple/",
            None,
            Some("pypi"),
        )?;
        let serialized = serde_json::json!({
            "indexes": [proxy],
            "flat-index": [],
            "no-index": false,
        });

        let error = serde_json::from_value::<IndexLocations>(serialized)
            .expect_err("an invalid proxy configuration should fail deserialization");
        assert_eq!(
            error.to_string(),
            "Proxy indexes require an `artifact-base-url`, but `https://proxy.example.com/simple/` does not have one configured"
        );
        Ok(())
    }

    #[test]
    fn proxy_flat_index_keeps_identity_route() -> TestResult {
        let flat = index_url("https://flat.example.com/packages/")?;
        let locations = IndexLocations::new(
            vec![pypi_proxy()?],
            vec![Index::from_find_links(flat.clone())],
            false,
        )?;
        let route = locations.route_for(&flat);

        assert!(locations.proxy_route_for(&flat).is_none());
        assert_eq!(locations.effective_url(&flat), &flat);
        assert!(!route.is_proxy());
        assert_eq!(route.canonical, flat);
        let artifact = url("https://flat.example.com/packages/package.whl?download=1#sha256=abc")?;
        let file = route.canonicalize_file(response_file(artifact.clone()))?;
        assert_eq!(file.url.to_url()?, artifact);
        Ok(())
    }

    #[test]
    fn proxy_artifact_mapping_preserves_encoded_suffix_and_query() -> TestResult {
        let locations = pypi_locations(pypi_proxy()?)?;
        let route = locations
            .proxy_route_for(&index_url("https://pypi.org/simple/")?)
            .ok_or("missing proxy route")?;
        let canonical = url(
            "https://files.pythonhosted.org/packages/example%20project/package%2B1.whl?download=1#sha256=abc",
        )?;
        let expected = url(
            "https://proxy.example.com/files/example%20project/package%2B1.whl?download=1#sha256=abc",
        )?;

        let canonical = as_canonical(canonical);
        let effective = route.artifact_url_for_request(&canonical)?;
        assert_eq!(effective, expected);
        let canonicalized = route.canonicalize_file(response_file(effective))?;
        assert_eq!(canonicalized.url, canonical);
        Ok(())
    }

    #[test]
    fn proxy_artifact_mapping_requires_a_segment_boundary() -> TestResult {
        let locations = pypi_locations(pypi_proxy()?)?;
        let route = locations
            .proxy_route_for(&index_url("https://pypi.org/simple/")?)
            .ok_or("missing proxy route")?;
        for artifact in [
            "https://files.pythonhosted.org/packages-extra/package.whl",
            "https://unmapped.example.com/files/package.whl",
        ] {
            assert!(matches!(
                route.artifact_url_for_request(&as_canonical(url(artifact)?)),
                Err(ProxyIndexError::UnmappedUrl { .. })
            ));
        }
        assert!(matches!(
            route.canonicalize_file(response_file(url(
                "https://proxy.example.com/unknown/package.whl"
            )?)),
            Err(ProxyIndexError::UnmappedUrl { .. })
        ));
        Ok(())
    }

    #[test]
    fn proxy_custom_index_uses_both_configured_artifact_bases() -> TestResult {
        let canonical = named_index(
            "upstream",
            "https://canonical-user:canonical-secret@upstream.example.com/simple/",
            Some("https://artifacts.example.com/distributions/"),
            None,
        )?;
        let proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("upstream"),
        )?;
        let locations = IndexLocations::new(vec![canonical.clone(), proxy], Vec::new(), false)?;
        let route = locations
            .proxy_route_for(canonical.url())
            .ok_or("missing proxy route")?;
        let canonical_artifact = url("https://artifacts.example.com/distributions/package.whl")?;

        assert_eq!(
            route.canonical.as_index_url().url().username(),
            "canonical-user"
        );
        assert_eq!(
            route.canonical.as_index_url().url().password(),
            Some("canonical-secret")
        );
        let canonical_artifact = as_canonical(canonical_artifact);
        assert_eq!(
            route.artifact_url_for_request(&canonical_artifact)?,
            url("https://proxy.example.com/files/package.whl")?
        );
        assert_eq!(
            route
                .canonicalize_file(response_file(url(
                    "https://proxy.example.com/files/package.whl"
                )?))?
                .url,
            canonical_artifact,
        );
        Ok(())
    }

    #[test]
    fn proxy_index_rejects_missing_artifact_bases_and_duplicate_targets() -> TestResult {
        let missing_physical = named_index(
            "socket",
            "https://proxy.example.com/simple/",
            None,
            Some("pypi"),
        )?;

        assert!(matches!(
            pypi_locations(missing_physical),
            Err(ProxyIndexConfigError::MissingProxyArtifactBase { .. })
        ));

        let canonical = named_index(
            "upstream",
            "https://upstream.example.com/simple/",
            None,
            None,
        )?;
        let proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("upstream"),
        )?;
        assert!(matches!(
            IndexLocations::new(vec![canonical.clone(), proxy], Vec::new(), false),
            Err(ProxyIndexConfigError::MissingCanonicalArtifactBase { .. })
        ));

        for name in ["other-proxy", "socket"] {
            let first = pypi_proxy()?;
            let second = named_index(
                name,
                "https://other-proxy.example.com/simple/",
                Some("https://other-proxy.example.com/files/"),
                Some("pypi"),
            )?;
            assert!(matches!(
                IndexLocations::new(vec![first, second], Vec::new(), false),
                Err(ProxyIndexConfigError::DuplicateCanonicalIndex { .. })
            ));
        }

        Ok(())
    }

    #[test]
    fn proxy_index_rejects_duplicate_index_names() -> TestResult {
        let ordinary = named_index("socket", "https://ordinary.example.com/simple/", None, None)?;
        for indexes in [
            vec![ordinary.clone(), pypi_proxy()?],
            vec![pypi_proxy()?, ordinary],
        ] {
            assert!(matches!(
                IndexLocations::new(indexes, Vec::new(), false),
                Err(ProxyIndexConfigError::DuplicateIndexName { name, proxy, index })
                    if name.as_ref() == "socket"
                        && proxy.as_str() == "https://proxy.example.com/simple/"
                        && index.as_str() == "https://ordinary.example.com/simple/"
            ));
        }

        let canonical = named_index(
            "upstream",
            "https://upstream.example.com/simple/",
            Some("https://upstream.example.com/files/"),
            None,
        )?;
        let duplicate = named_index(
            "socket",
            "https://other-proxy.example.com/simple/",
            Some("https://other-proxy.example.com/files/"),
            Some("upstream"),
        )?;
        assert!(matches!(
            IndexLocations::new(vec![canonical, pypi_proxy()?, duplicate], vec![], false),
            Err(ProxyIndexConfigError::DuplicateIndexName { name, proxy, index })
                if name.as_ref() == "socket"
                    && proxy.as_str() == "https://other-proxy.example.com/simple/"
                    && index.as_str() == "https://proxy.example.com/simple/"
        ));

        Ok(())
    }

    #[test]
    fn proxy_index_requires_a_configured_canonical_index() -> TestResult {
        let unknown_proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("missing"),
        )?;
        let chained_proxy = named_index(
            "other-proxy",
            "https://other-proxy.example.com/simple/",
            Some("https://other-proxy.example.com/files/"),
            Some("socket"),
        )?;
        let mut flat =
            Index::from_find_links(index_url("https://canonical.example.com/packages/")?);
        flat.name = Some("flat".parse()?);
        let flat_proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("flat"),
        )?;

        for (locations, expected_name) in [
            (pypi_locations(unknown_proxy), "missing"),
            (
                IndexLocations::new(vec![pypi_proxy()?, chained_proxy], Vec::new(), false),
                "socket",
            ),
            (
                IndexLocations::new(vec![flat_proxy], vec![flat], false),
                "flat",
            ),
        ] {
            match locations {
                Err(ProxyIndexConfigError::MissingCanonicalIndex { name }) => {
                    assert_eq!(name.as_ref(), expected_name);
                }
                result => {
                    return Err(format!("expected a missing package index, got {result:?}").into());
                }
            }
        }

        Ok(())
    }

    #[test]
    fn proxy_index_rejects_canonical_artifact_credentials() -> TestResult {
        let canonical = named_index(
            "private",
            "https://canonical.example.com/simple/",
            Some("https://user:secret@canonical.example.com/files/"),
            None,
        )?;
        let proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("private"),
        )?;
        let Err(error) = IndexLocations::new(vec![canonical, proxy], Vec::new(), false) else {
            return Err("canonical artifact credentials were unexpectedly accepted".into());
        };
        assert!(matches!(
            error,
            ProxyIndexConfigError::InvalidMapping { .. }
        ));
        assert!(!error.to_string().contains("secret"));
        Ok(())
    }

    #[test]
    fn proxy_index_preserves_physical_authentication_credentials() -> TestResult {
        let proxy = named_index(
            "socket",
            "https://user:secret@proxy.example.com/simple/",
            Some("https://artifact-user:artifact-secret@proxy.example.com/files/"),
            Some("pypi"),
        )?;
        let locations = pypi_locations(proxy)?;
        let route = locations
            .proxy_route_for(&index_url("https://pypi.org/simple/")?)
            .ok_or("missing proxy route")?;
        let canonical_artifact =
            as_canonical(url("https://files.pythonhosted.org/packages/package.whl")?);
        let physical_artifact = route.artifact_url_for_request(&canonical_artifact)?;

        assert_eq!(route.effective_url().url().username(), "user");
        assert_eq!(route.effective_url().url().password(), Some("secret"));
        assert_eq!(physical_artifact.username(), "artifact-user");
        assert_eq!(physical_artifact.password(), Some("artifact-secret"));
        Ok(())
    }

    #[test]
    fn proxy_index_rejects_unsafe_index_and_artifact_bases() -> TestResult {
        for physical in [
            "ftp://proxy.example.com/simple/",
            "file:///proxy/simple/",
            "https://proxy.example.com/simple/?download=1",
            "https://proxy.example.com/simple/#fragment",
            "https://proxy.example.com/simple%2fprivate/",
            "https://proxy.example.com/simple%5cprivate/",
        ] {
            let proxy = named_index("socket", physical, None, Some("pypi"))?;

            assert!(
                matches!(
                    pypi_locations(proxy),
                    Err(ProxyIndexConfigError::InvalidMapping { .. })
                ),
                "unsafe proxy URL was accepted: {physical}"
            );
        }

        for artifact in [
            "ftp://proxy.example.com/files/",
            "https://proxy.example.com/files/?download=1",
            "https://proxy.example.com/files/#fragment",
            "https://proxy.example.com/files%2fprivate/",
            "https://proxy.example.com/files%5cprivate/",
        ] {
            let proxy = named_index(
                "socket",
                "https://proxy.example.com/simple/",
                Some(artifact),
                Some("pypi"),
            )?;

            assert!(
                matches!(
                    pypi_locations(proxy),
                    Err(ProxyIndexConfigError::InvalidMapping { .. })
                ),
                "unsafe proxy artifact base was accepted: {artifact}"
            );
        }

        let mut proxy = pypi_proxy()?;
        proxy.format = IndexFormat::Flat;
        assert!(matches!(
            pypi_locations(proxy),
            Err(ProxyIndexConfigError::InvalidMapping { .. })
        ));

        Ok(())
    }

    #[test]
    fn proxy_index_ignores_proxies_when_indexes_are_disabled() -> TestResult {
        let canonical = named_index(
            "upstream",
            "https://upstream.example.com/simple/",
            Some("https://upstream.example.com/files/"),
            None,
        )?;
        let proxy = named_index(
            "mirror",
            "https://proxy.example.com/simple/",
            Some("https://proxy.example.com/files/"),
            Some("upstream"),
        )?;

        for (locations, expected_canonical) in [
            (
                IndexLocations::new(vec![pypi_proxy()?], Vec::new(), true),
                index_url("https://pypi.org/simple/")?,
            ),
            (
                IndexLocations::new(vec![canonical.clone(), proxy], Vec::new(), true),
                canonical.url().clone(),
            ),
        ] {
            let locations = locations?;
            assert_eq!(locations.proxy_routes().count(), 0);
            assert!(locations.proxy_route_for(&expected_canonical).is_none());
        }

        Ok(())
    }
}
