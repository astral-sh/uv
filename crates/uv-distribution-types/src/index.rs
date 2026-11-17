use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use http::{HeaderValue, StatusCode};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use uv_auth::{AuthPolicy, Credentials, CredentialsFromUrlError};
use uv_normalize::PackageName;
use uv_pypi_types::HashAlgorithm;
use uv_redacted::SafeUrl;
use uv_redacted::policy::Stripped;
use uv_small_str::SmallString;

use crate::exclude_newer::ExcludeNewerOverride;
use crate::index_name::{IndexName, IndexNameError};
use crate::origin::Origin;
use crate::{IndexStatusCodeStrategy, IndexUrl, IndexUrlError, SerializableStatusCode};

/// HTTP status and cache policies borrowed from a package index.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexRequestPolicy<'a> {
    index: Option<&'a Index>,
}

impl<'a> IndexRequestPolicy<'a> {
    /// Return the status codes that let a search continue to the next index.
    pub fn status_code_strategy(&self) -> IndexStatusCodeStrategy<'a> {
        let Some(index) = self.index else {
            return IndexStatusCodeStrategy::Default;
        };
        match index.ignore_error_codes.as_deref() {
            Some(status_codes) => {
                IndexStatusCodeStrategy::IgnoreConfiguredErrorCodes { status_codes }
            }
            None => IndexStatusCodeStrategy::from_index_url(index.raw_url()),
        }
    }

    /// Return whether this status code is explicitly ignored, excluding built-in defaults.
    pub fn ignores_error_code(&self, status_code: StatusCode) -> bool {
        self.status_code_strategy()
            .explicitly_ignores_error_code(status_code)
    }

    /// Return the cache control header for Simple API requests.
    pub fn simple_api_cache_control(&self) -> Option<HeaderValue> {
        self.index?.cache_control.as_ref()?.api.clone()
    }

    /// Return the cache control header for artifact downloads, including index-specific defaults.
    pub fn artifact_cache_control(&self) -> Option<HeaderValue> {
        let index = self.index?;
        self.configured_files_cache_control()
            .or_else(|| IndexCacheControl::artifact_cache_control(index.raw_url()))
    }

    /// Return the file cache control header only when the user configured it.
    pub fn configured_files_cache_control(&self) -> Option<HeaderValue> {
        self.index?.cache_control.as_ref()?.files.clone()
    }
}

/// Cache control configuration for an index.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IndexCacheControl {
    /// Cache control header for Simple API requests.
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub(crate) api: Option<HeaderValue>,
    /// Cache control header for file downloads.
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub(crate) files: Option<HeaderValue>,
}

impl IndexCacheControl {
    /// Return the default files cache control headers for the given index URL, if applicable.
    fn artifact_cache_control(url: &SafeUrl) -> Option<HeaderValue> {
        let dominated_by_pytorch_or_nvidia = url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("download.pytorch.org")
                || host.eq_ignore_ascii_case("pypi.nvidia.com")
        });
        if dominated_by_pytorch_or_nvidia {
            // Some wheels in the PyTorch registry were accidentally uploaded with `no-cache,no-store,must-revalidate`.
            // The PyTorch team plans to correct this in the future, but in the meantime we override
            // the cache control headers to allow caching of static files.
            //
            // See: https://github.com/pytorch/pytorch/pull/149218
            //
            // The same issue applies to files hosted on `pypi.nvidia.com`.
            Some(HeaderValue::from_static(
                "max-age=365000000, immutable, public",
            ))
        } else {
            None
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct IndexCacheControlRef<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    api: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<&'a str>,
}

impl Serialize for IndexCacheControl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        IndexCacheControlRef {
            api: self.api.as_ref().map(|api| {
                api.to_str()
                    .expect("cache-control.api is always parsed from a string")
            }),
            files: self.files.as_ref().map(|files| {
                files
                    .to_str()
                    .expect("cache-control.files is always parsed from a string")
            }),
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IndexCacheControlWire {
    api: Option<SmallString>,
    files: Option<SmallString>,
}

impl<'de> Deserialize<'de> for IndexCacheControl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = IndexCacheControlWire::deserialize(deserializer)?;

        let api = wire
            .api
            .map(|api| {
                HeaderValue::from_str(api.as_ref()).map_err(|_| {
                    serde::de::Error::custom(
                        "`cache-control.api` must be a valid HTTP header value",
                    )
                })
            })
            .transpose()?;
        let files = wire
            .files
            .map(|files| {
                HeaderValue::from_str(files.as_ref()).map_err(|_| {
                    serde::de::Error::custom(
                        "`cache-control.files` must be a valid HTTP header value",
                    )
                })
            })
            .transpose()?;

        Ok(Self { api, files })
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub struct Index {
    /// The name of the index.
    ///
    /// Index names can be used to reference indexes elsewhere in the configuration. For example,
    /// you can pin a package to a specific index by name:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pytorch"
    /// url = "https://download.pytorch.org/whl/cu130"
    ///
    /// [tool.uv.sources]
    /// torch = { index = "pytorch" }
    /// ```
    pub name: Option<IndexName>,
    /// The URL of the index.
    ///
    /// Expects to receive a URL (e.g., `https://pypi.org/simple`) or a local path.
    pub url: IndexUrl,
    /// The URL prefix from which this index serves package files.
    ///
    /// When this index is proxied, uv uses its `artifact-base-url` and the proxy's
    /// `artifact-base-url` to download packages without changing the URLs recorded in the
    /// lockfile.
    ///
    /// PyPI defaults to `https://files.pythonhosted.org/packages/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_base_url: Option<SafeUrl<Stripped>>,
    /// Mark the index as explicit.
    ///
    /// Explicit indexes will _only_ be used when explicitly requested via a `[tool.uv.sources]`
    /// definition, as in:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pytorch"
    /// url = "https://download.pytorch.org/whl/cu130"
    /// explicit = true
    ///
    /// [tool.uv.sources]
    /// torch = { index = "pytorch" }
    /// ```
    #[serde(default)]
    pub explicit: bool,
    /// Mark the index as the default index.
    ///
    /// By default, uv uses PyPI as the default index, such that even if additional indexes are
    /// defined via `[[tool.uv.index]]`, PyPI will still be used as a fallback for packages that
    /// aren't found elsewhere. To disable the PyPI default, set `default = true` on at least one
    /// other index.
    ///
    /// Marking an index as default will move it to the front of the list of indexes, such that it
    /// is given the highest priority when resolving packages.
    #[serde(default)]
    pub default: bool,
    /// The origin of the index (e.g., a CLI flag, a user-level configuration file, etc.).
    #[serde(skip)]
    pub origin: Option<Origin>,
    /// The format used by the index.
    ///
    /// Indexes can either be PEP 503-compliant (i.e., a PyPI-style registry implementing the Simple
    /// API) or structured as a flat list of distributions (e.g., `--find-links`). In both cases,
    /// indexes can point to either local or remote resources.
    #[serde(default)]
    pub format: IndexFormat,
    /// The URL of the upload endpoint.
    ///
    /// When using `uv publish --index <name>`, this URL is used for publishing.
    ///
    /// A configuration for the default index PyPI would look as follows:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pypi"
    /// url = "https://pypi.org/simple"
    /// publish-url = "https://upload.pypi.org/legacy/"
    /// ```
    pub publish_url: Option<SafeUrl<Stripped>>,
    /// When uv should use authentication for requests to the index.
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "my-index"
    /// url = "https://<omitted>/simple"
    /// authenticate = "always"
    /// ```
    #[serde(default)]
    pub authenticate: AuthPolicy,
    /// Status codes that uv should ignore when deciding whether to continue resolution after a
    /// request to this index fails.
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "my-index"
    /// url = "https://<omitted>/simple"
    /// ignore-error-codes = [401, 403]
    /// ```
    #[serde(default)]
    pub ignore_error_codes: Option<Vec<SerializableStatusCode>>,
    /// Cache control configuration for this index.
    ///
    /// When set, these headers will override the server's cache control headers
    /// for both package metadata requests and artifact downloads.
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "my-index"
    /// url = "https://<omitted>/simple"
    /// cache-control = { api = "max-age=600", files = "max-age=3600" }
    /// ```
    #[serde(default)]
    pub cache_control: Option<IndexCacheControl>,
    /// The hash algorithm that must be used for distributions resolved from this index.
    ///
    /// If a distribution does not advertise a hash using this algorithm, lockfile generation
    /// will fail.
    ///
    /// This option is in preview and may change in any future release.
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "my-index"
    /// url = "https://<omitted>/simple"
    /// hash-algorithm = "sha256"
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_algorithm: Option<IndexHashAlgorithm>,
    /// An index-specific `exclude-newer` cutoff.
    ///
    /// Accepts the same date, timestamp, and duration values as the global `exclude-newer`
    /// setting. Set this to `false` to disable `exclude-newer` for this index entirely.
    ///
    /// When set to a value, packages resolved from this index will use that cutoff instead of the
    /// globally-specified value, unless a package-specific `exclude-newer-package` override is
    /// present.
    ///
    /// This option is in preview and may change in any future release.
    ///
    /// ```toml
    /// [tool.uv]
    /// exclude-newer = "2025-01-01T00:00:00Z"
    ///
    /// [[tool.uv.index]]
    /// name = "internal"
    /// url = "https://internal.example.com/simple"
    /// exclude-newer = "7 days"
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "ExcludeNewerOverride"))]
    pub exclude_newer: Option<ExcludeNewerOverride>,
    /// Package-specific cutoffs for distributions resolved from this index.
    ///
    /// For each package, accepts the same date, timestamp, and duration values as the global
    /// `exclude-newer` setting. Set a package's value to `false` to disable `exclude-newer` for that
    /// package on this index.
    ///
    /// These override the index's `exclude-newer` value and the global cutoff. Top-level
    /// `exclude-newer-package` settings take precedence over these overrides.
    ///
    /// This option is in preview and may change in any future release.
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "internal"
    /// url = "https://internal.example.com/simple"
    /// exclude-newer = "7 days"
    /// exclude-newer-package = { example = false }
    /// ```
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub exclude_newer_package: BTreeMap<PackageName, ExcludeNewerOverride>,
}

/// A proxy for a package index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProxyIndex {
    /// The proxy's name, used for credential lookup.
    pub name: Option<IndexName>,
    /// The proxy's Simple API endpoint.
    pub url: IndexUrl,
    /// The upstream index's name.
    pub proxy_for: IndexName,
    /// The URL prefix under which the proxy serves package files.
    pub artifact_base_url: SafeUrl<Stripped>,
    /// When to authenticate requests to the proxy's Simple API.
    #[serde(default)]
    pub authenticate: AuthPolicy,
    /// HTTP status codes to ignore when querying the proxy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_error_codes: Option<Vec<SerializableStatusCode>>,
    /// Cache control headers for the proxy's Simple API and package files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<IndexCacheControl>,
}

impl ProxyIndex {
    /// Create a proxy for an upstream index.
    pub fn new(url: IndexUrl, proxy_for: IndexName, artifact_base_url: SafeUrl<Stripped>) -> Self {
        Self {
            name: None,
            url,
            proxy_for,
            artifact_base_url,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
        }
    }

    /// Promote automatic authentication when configured credentials will be stripped on persistence.
    #[must_use]
    pub fn with_promoted_auth_policy(mut self) -> Self {
        if self.authenticate == AuthPolicy::Auto
            && index_has_credentials(self.name.as_ref(), &self.url)
        {
            self.authenticate = AuthPolicy::Always;
        }
        self
    }
}

#[derive(Debug, Error)]
#[error("Failed to parse credentials in index URL: {url}")]
pub struct IndexCredentialsError {
    url: SafeUrl,
    #[source]
    source: CredentialsFromUrlError,
}

fn index_has_credentials(name: Option<&IndexName>, url: &IndexUrl) -> bool {
    name.is_some_and(|name| Credentials::from_env(name.to_env_var()).is_some())
        || !url.url().username().is_empty()
        || url.url().password().is_some()
}

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            name,
            url,
            artifact_base_url,
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
            hash_algorithm,
            exclude_newer,
            exclude_newer_package,
        } = self;
        *url == other.url
            && *name == other.name
            && *artifact_base_url == other.artifact_base_url
            && *explicit == other.explicit
            && *default == other.default
            && *format == other.format
            && *publish_url == other.publish_url
            && *authenticate == other.authenticate
            && *ignore_error_codes == other.ignore_error_codes
            && *cache_control == other.cache_control
            && *hash_algorithm == other.hash_algorithm
            && *exclude_newer == other.exclude_newer
            && *exclude_newer_package == other.exclude_newer_package
    }
}

impl Eq for Index {}

impl PartialOrd for Index {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Index {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let Self {
            name,
            url,
            artifact_base_url,
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
            hash_algorithm,
            exclude_newer,
            exclude_newer_package,
        } = self;
        url.cmp(&other.url)
            .then_with(|| name.cmp(&other.name))
            .then_with(|| artifact_base_url.cmp(&other.artifact_base_url))
            .then_with(|| explicit.cmp(&other.explicit))
            .then_with(|| default.cmp(&other.default))
            .then_with(|| format.cmp(&other.format))
            .then_with(|| publish_url.cmp(&other.publish_url))
            .then_with(|| authenticate.cmp(&other.authenticate))
            .then_with(|| ignore_error_codes.cmp(&other.ignore_error_codes))
            .then_with(|| cache_control.cmp(&other.cache_control))
            .then_with(|| hash_algorithm.cmp(&other.hash_algorithm))
            .then_with(|| exclude_newer.cmp(&other.exclude_newer))
            .then_with(|| exclude_newer_package.cmp(&other.exclude_newer_package))
    }
}

impl std::hash::Hash for Index {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            name,
            url,
            artifact_base_url,
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
            hash_algorithm,
            exclude_newer,
            exclude_newer_package,
        } = self;
        url.hash(state);
        name.hash(state);
        artifact_base_url.hash(state);
        explicit.hash(state);
        default.hash(state);
        format.hash(state);
        publish_url.hash(state);
        authenticate.hash(state);
        ignore_error_codes.hash(state);
        cache_control.hash(state);
        hash_algorithm.hash(state);
        exclude_newer.hash(state);
        exclude_newer_package.hash(state);
    }
}

#[derive(
    Default,
    Debug,
    Copy,
    Clone,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum IndexFormat {
    /// A PyPI-style index implementing the Simple Repository API.
    #[default]
    Simple,
    /// A `--find-links`-style index containing a flat list of wheels and source distributions.
    Flat,
}

/// A hash algorithm that can be required for distributions resolved from an index.
#[derive(
    Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum IndexHashAlgorithm {
    Md5,
    Sha256,
    Sha384,
    Sha512,
    Blake2b,
}

impl From<IndexHashAlgorithm> for HashAlgorithm {
    fn from(value: IndexHashAlgorithm) -> Self {
        match value {
            IndexHashAlgorithm::Md5 => Self::Md5,
            IndexHashAlgorithm::Sha256 => Self::Sha256,
            IndexHashAlgorithm::Sha384 => Self::Sha384,
            IndexHashAlgorithm::Sha512 => Self::Sha512,
            IndexHashAlgorithm::Blake2b => Self::Blake2b256,
        }
    }
}

impl Index {
    /// Initialize an [`Index`] from a pip-style `--index-url`.
    pub fn from_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            artifact_base_url: None,
            explicit: false,
            default: true,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
            hash_algorithm: None,
            exclude_newer: None,
            exclude_newer_package: BTreeMap::new(),
        }
    }

    /// Initialize an [`Index`] from a pip-style `--extra-index-url`.
    pub fn from_extra_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            artifact_base_url: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
            hash_algorithm: None,
            exclude_newer: None,
            exclude_newer_package: BTreeMap::new(),
        }
    }

    /// Initialize an [`Index`] from a pip-style `--find-links`.
    pub fn from_find_links(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            artifact_base_url: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Flat,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
            hash_algorithm: None,
            exclude_newer: None,
            exclude_newer_package: BTreeMap::new(),
        }
    }

    /// Set the [`Origin`] of the index.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Return the [`IndexUrl`] of the index.
    pub fn url(&self) -> &IndexUrl {
        &self.url
    }

    /// Return the underlying [`SafeUrl`] of the index.
    pub fn raw_url(&self) -> &SafeUrl {
        self.url.url()
    }

    /// Return the root [`SafeUrl`] of the index, if applicable.
    ///
    /// For indexes with a `/simple` endpoint, this is simply the URL with the final segment
    /// removed. This is useful, e.g., for credential propagation to other endpoints on the index.
    pub fn root_url(&self) -> Option<SafeUrl> {
        self.url.root()
    }

    /// If credentials are available (via the URL or environment) and [`AuthPolicy`] is
    /// [`AuthPolicy::Auto`], promote to [`AuthPolicy::Always`] so that future operations
    /// (e.g., `uv tool upgrade`) know that authentication is required even after the credentials
    /// are stripped from the stored URL.
    #[must_use]
    pub fn with_promoted_auth_policy(mut self) -> Self {
        if matches!(self.authenticate, AuthPolicy::Auto) && self.has_credentials() {
            self.authenticate = AuthPolicy::Always;
        }
        self
    }

    /// Return whether credentials are configured for the index.
    ///
    /// This only checks for the presence of credentials. It intentionally avoids decoding URL
    /// credentials, since this is used to preserve the authentication policy after credentials are
    /// removed from the stored URL; parsing errors are reported when the credentials are retrieved.
    fn has_credentials(&self) -> bool {
        index_has_credentials(self.name.as_ref(), &self.url)
    }

    /// Retrieve the credentials for the index, either from the environment, or from the URL itself.
    pub fn credentials(&self) -> Result<Option<Credentials>, IndexCredentialsError> {
        if let Some(name) = &self.name
            && let Some(credentials) = Credentials::from_env(name.to_env_var())
        {
            return Ok(Some(credentials));
        }

        Credentials::from_url(self.url.url()).map_err(|source| IndexCredentialsError {
            url: self.url.url().clone(),
            source,
        })
    }

    /// Resolve the index relative to the given root directory.
    pub fn relative_to(mut self, root_dir: &Path) -> Result<Self, IndexUrlError> {
        if let IndexUrl::Path(ref url) = self.url
            && let Some(given) = url.given()
        {
            self.url = IndexUrl::parse(given, Some(root_dir))?;
        }
        Ok(self)
    }

    /// Borrow the HTTP request policy for this index.
    pub(crate) fn request_policy(&self) -> IndexRequestPolicy<'_> {
        IndexRequestPolicy { index: Some(self) }
    }

    /// Return the package-specific cutoff for this index, falling back to its default cutoff.
    pub(crate) fn exclude_newer(&self, package: &PackageName) -> Option<&ExcludeNewerOverride> {
        self.exclude_newer_package
            .get(package)
            .or(self.exclude_newer.as_ref())
    }
}

impl From<IndexUrl> for Index {
    fn from(value: IndexUrl) -> Self {
        Self {
            name: None,
            url: value,
            artifact_base_url: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
            hash_algorithm: None,
            exclude_newer: None,
            exclude_newer_package: BTreeMap::new(),
        }
    }
}

impl FromStr for Index {
    type Err = IndexSourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Determine whether the source is prefixed with a name, as in `name=https://pypi.org/simple`.
        if let Some((name, url)) = s.split_once('=')
            && !name.chars().any(|c| c == ':')
        {
            let name = IndexName::from_str(name)?;
            let url = IndexUrl::from_str(url)?;
            return Ok(Self {
                name: Some(name),
                url,
                artifact_base_url: None,
                explicit: false,
                default: false,
                origin: None,
                format: IndexFormat::Simple,
                publish_url: None,
                authenticate: AuthPolicy::default(),
                ignore_error_codes: None,
                cache_control: None,
                hash_algorithm: None,
                exclude_newer: None,
                exclude_newer_package: BTreeMap::new(),
            });
        }

        // Otherwise, assume the source is a URL.
        let url = IndexUrl::from_str(s)?;
        Ok(Self {
            name: None,
            url,
            artifact_base_url: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
            hash_algorithm: None,
            exclude_newer: None,
            exclude_newer_package: BTreeMap::new(),
        })
    }
}

/// An [`IndexUrl`] along with the metadata necessary to query the index.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct IndexMetadata {
    /// The URL of the index.
    pub url: IndexUrl,
    /// The format used by the index.
    pub format: IndexFormat,
}

impl IndexMetadata {
    /// Consume the [`IndexMetadata`] and return the [`IndexUrl`].
    pub fn into_url(self) -> IndexUrl {
        self.url
    }
}

/// A reference to an [`IndexMetadata`].
#[derive(Debug, Copy, Clone)]
pub struct IndexMetadataRef<'a> {
    /// The URL of the index.
    pub url: &'a IndexUrl,
    /// The format used by the index.
    pub format: IndexFormat,
}

impl IndexMetadata {
    /// Return the [`IndexUrl`] of the index.
    pub fn url(&self) -> &IndexUrl {
        &self.url
    }
}

impl<'a> From<&'a Index> for IndexMetadataRef<'a> {
    fn from(value: &'a Index) -> Self {
        Self {
            url: &value.url,
            format: value.format,
        }
    }
}

impl<'a> From<&'a IndexMetadata> for IndexMetadataRef<'a> {
    fn from(value: &'a IndexMetadata) -> Self {
        Self {
            url: &value.url,
            format: value.format,
        }
    }
}

impl From<IndexUrl> for IndexMetadata {
    fn from(value: IndexUrl) -> Self {
        Self {
            url: value,
            format: IndexFormat::Simple,
        }
    }
}

impl<'a> From<&'a IndexUrl> for IndexMetadataRef<'a> {
    fn from(value: &'a IndexUrl) -> Self {
        Self {
            url: value,
            format: IndexFormat::Simple,
        }
    }
}

/// Wire type for deserializing an [`Index`] with validation.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct IndexWire {
    name: Option<IndexName>,
    url: IndexUrl,
    #[serde(default)]
    artifact_base_url: Option<SafeUrl<Stripped>>,
    #[serde(default)]
    explicit: bool,
    #[serde(default)]
    default: bool,
    #[serde(default)]
    format: IndexFormat,
    publish_url: Option<SafeUrl>,
    #[serde(default)]
    authenticate: AuthPolicy,
    #[serde(default)]
    ignore_error_codes: Option<Vec<SerializableStatusCode>>,
    #[serde(default)]
    cache_control: Option<IndexCacheControl>,
    #[serde(default)]
    hash_algorithm: Option<IndexHashAlgorithm>,
    #[serde(default)]
    exclude_newer: Option<ExcludeNewerOverride>,
    #[serde(default)]
    exclude_newer_package: BTreeMap<PackageName, ExcludeNewerOverride>,
}

impl<'de> Deserialize<'de> for Index {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = IndexWire::deserialize(deserializer)?;

        if wire.explicit && wire.name.is_none() {
            return Err(serde::de::Error::custom(format!(
                "An index with `explicit = true` requires a `name`: {}",
                wire.url
            )));
        }

        Ok(Self {
            name: wire.name,
            url: wire.url,
            artifact_base_url: wire.artifact_base_url,
            explicit: wire.explicit,
            default: wire.default,
            origin: None,
            format: wire.format,
            publish_url: wire.publish_url.map(|url| url.with_policy(Stripped)),
            authenticate: wire.authenticate,
            ignore_error_codes: wire.ignore_error_codes,
            cache_control: wire.cache_control,
            hash_algorithm: wire.hash_algorithm,
            exclude_newer: wire.exclude_newer,
            exclude_newer_package: wire.exclude_newer_package,
        })
    }
}

/// An error that can occur when parsing an [`Index`].
#[derive(Error, Debug)]
pub enum IndexSourceError {
    #[error(transparent)]
    Url(#[from] IndexUrlError),
    #[error(transparent)]
    IndexName(#[from] IndexNameError),
    #[error("Index included a name, but the name was empty")]
    EmptyName,
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;
    use http::HeaderValue;

    use crate::IndexLocations;

    #[test]
    fn test_proxy_index_serialization_redacts_artifact_credentials()
    -> Result<(), Box<dyn std::error::Error>> {
        let index: ProxyIndex = toml::from_str(
            r#"
            name = "socket"
            url = "https://simple-user:simple-secret@proxy.example.com/simple/"
            artifact-base-url = "https://artifact-user:artifact-secret@proxy.example.com/files/"
            proxy-for = "pypi"
            "#,
        )?;

        assert_eq!(index.name.as_deref(), Some("socket"));
        assert_eq!(index.proxy_for.as_ref(), "pypi");
        assert_eq!(index.url.url().username(), "simple-user");
        assert_eq!(index.url.url().password(), Some("simple-secret"));
        assert_eq!(index.artifact_base_url.as_url().username(), "artifact-user");
        assert_eq!(
            index.artifact_base_url.as_url().password(),
            Some("artifact-secret")
        );

        let serialized = toml::to_string(&index)?;
        insta::assert_snapshot!(serialized, @r#"
        name = "socket"
        url = "https://proxy.example.com/simple/"
        proxy-for = "pypi"
        artifact-base-url = "https://proxy.example.com/files/"
        authenticate = "auto"
        "#);

        let debug = format!("{index:?}");
        assert!(!debug.contains("simple-secret"));
        assert!(!debug.contains("artifact-secret"));

        let restored: ProxyIndex = toml::from_str(&serialized)?;
        assert_eq!(restored.name.as_deref(), Some("socket"));
        assert_eq!(restored.proxy_for.as_ref(), "pypi");
        assert!(restored.url.url().username().is_empty());
        assert!(restored.url.url().password().is_none());
        let artifact_base = restored.artifact_base_url;
        assert!(artifact_base.as_url().username().is_empty());
        assert!(artifact_base.as_url().password().is_none());

        Ok(())
    }

    #[test]
    fn test_index_allows_colons_and_at_signs_in_artifact_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let index: ProxyIndex = toml::from_str(
            r#"
            name = "socket"
            url = "https://proxy.example.com/simple/"
            artifact-base-url = "https://proxy.example.com/files/project:build@nightly/"
            proxy-for = "pypi"
            "#,
        )?;

        assert_eq!(
            index.artifact_base_url.as_url().as_str(),
            "https://proxy.example.com/files/project:build@nightly/"
        );
        let mut upstream =
            Index::from_index_url(IndexUrl::parse("https://pypi.org/simple/", None)?);
        upstream.name = Some("pypi".parse()?);
        IndexLocations::new(vec![upstream], vec![index], Vec::new(), false)?;

        Ok(())
    }

    #[test]
    fn test_index_cache_control_headers() {
        // Test that cache control headers are properly parsed from TOML
        let toml_str = r#"
            name = "test-index"
            url = "https://test.example.com/simple"
            cache-control = { api = "max-age=600", files = "max-age=3600" }
        "#;

        let index: Index = toml::from_str(toml_str).unwrap();
        assert_eq!(index.name.as_ref().unwrap().as_ref(), "test-index");
        assert!(index.cache_control.is_some());
        assert_eq!(index.exclude_newer, None);
        let cache_control = index.cache_control.as_ref().unwrap();
        assert_eq!(
            cache_control.api,
            Some(HeaderValue::from_static("max-age=600"))
        );
        assert_eq!(
            cache_control.files,
            Some(HeaderValue::from_static("max-age=3600"))
        );
    }

    #[test]
    fn test_index_without_cache_control() {
        // Test that indexes work without cache control headers
        let toml_str = r#"
            name = "test-index"
            url = "https://test.example.com/simple"
        "#;

        let index: Index = toml::from_str(toml_str).unwrap();
        assert_eq!(index.name.as_ref().unwrap().as_ref(), "test-index");
        assert_eq!(index.cache_control, None);
        assert_eq!(index.exclude_newer, None);
    }

    #[test]
    fn test_index_partial_cache_control() {
        // Test that cache control can have just one field
        let toml_str = r#"
            name = "test-index"
            url = "https://test.example.com/simple"
            cache-control = { api = "max-age=300" }
        "#;

        let index: Index = toml::from_str(toml_str).unwrap();
        assert_eq!(index.name.as_ref().unwrap().as_ref(), "test-index");
        assert!(index.cache_control.is_some());
        assert_eq!(index.exclude_newer, None);
        let cache_control = index.cache_control.as_ref().unwrap();
        assert_eq!(
            cache_control.api,
            Some(HeaderValue::from_static("max-age=300"))
        );
        assert_eq!(cache_control.files, None);
    }

    #[test]
    fn test_index_invalid_api_cache_control() {
        let toml_str = r#"
            name = "test-index"
            url = "https://test.example.com/simple"
            cache-control = { api = "max-age=600\n" }
        "#;

        let err = toml::from_str::<Index>(toml_str).unwrap_err();
        assert!(
            err.to_string()
                .contains("`cache-control.api` must be a valid HTTP header value")
        );
    }

    #[test]
    fn test_index_invalid_files_cache_control() {
        let toml_str = r#"
            name = "test-index"
            url = "https://test.example.com/simple"
            cache-control = { files = "max-age=3600\n" }
        "#;

        let err = toml::from_str::<Index>(toml_str).unwrap_err();
        assert!(
            err.to_string()
                .contains("`cache-control.files` must be a valid HTTP header value")
        );
    }

    #[test]
    fn test_index_exclude_newer_disable() {
        let toml_str = r#"
            name = "internal"
            url = "https://internal.example.com/simple"
            exclude-newer = false
        "#;

        let index: Index = toml::from_str(toml_str).unwrap();
        assert_eq!(index.name.as_ref().unwrap().as_ref(), "internal");
        assert_eq!(index.exclude_newer, Some(ExcludeNewerOverride::Disabled));
    }

    #[test]
    fn test_index_exclude_newer_relative() {
        let toml_str = r#"
            name = "internal"
            url = "https://internal.example.com/simple"
            exclude-newer = "7 days"
        "#;

        let index: Index = toml::from_str(toml_str).unwrap();
        assert_eq!(index.name.as_ref().unwrap().as_ref(), "internal");
        assert_matches!(index.exclude_newer, Some(ExcludeNewerOverride::Enabled(_)));
    }
}
