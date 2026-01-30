use std::borrow::Borrow;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use uv_auth::{AuthPolicy, Credentials};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;

use crate::index_name::{IndexName, IndexNameError};
use crate::origin::Origin;
use crate::{IndexStatusCodeStrategy, IndexUrl, IndexUrlError, SerializableStatusCode};

/// Cache control configuration for an index.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub struct IndexCacheControl {
    /// Cache control header for Simple API requests.
    pub api: Option<SmallString>,
    /// Cache control header for file downloads.
    pub files: Option<SmallString>,
}

impl IndexCacheControl {
    /// Return the default Simple API cache control headers for the given index URL, if applicable.
    pub fn simple_api_cache_control(_url: &Url) -> Option<&'static str> {
        None
    }

    /// Return the default files cache control headers for the given index URL, if applicable.
    pub fn artifact_cache_control(url: &Url) -> Option<&'static str> {
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
            Some("max-age=365000000, immutable, public")
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// url = "https://download.pytorch.org/whl/cu121"
    ///
    /// [tool.uv.sources]
    /// torch = { index = "pytorch" }
    /// ```
    pub name: Option<IndexName>,
    /// The URL of the index.
    ///
    /// Expects to receive a URL (e.g., `https://pypi.org/simple`) or a local path.
    pub url: IndexUrl,
    /// Mark the index as explicit.
    ///
    /// Explicit indexes will _only_ be used when explicitly requested via a `[tool.uv.sources]`
    /// definition, as in:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pytorch"
    /// url = "https://download.pytorch.org/whl/cu121"
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
    pub publish_url: Option<DisplaySafeUrl>,
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
    /// Status codes that uv should ignore when deciding whether
    /// to continue searching in the next index after a failure.
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
}

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            name,
            url,
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
        } = self;
        *url == other.url
            && *name == other.name
            && *explicit == other.explicit
            && *default == other.default
            && *format == other.format
            && *publish_url == other.publish_url
            && *authenticate == other.authenticate
            && *ignore_error_codes == other.ignore_error_codes
            && *cache_control == other.cache_control
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
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
        } = self;
        url.cmp(&other.url)
            .then_with(|| name.cmp(&other.name))
            .then_with(|| explicit.cmp(&other.explicit))
            .then_with(|| default.cmp(&other.default))
            .then_with(|| format.cmp(&other.format))
            .then_with(|| publish_url.cmp(&other.publish_url))
            .then_with(|| authenticate.cmp(&other.authenticate))
            .then_with(|| ignore_error_codes.cmp(&other.ignore_error_codes))
            .then_with(|| cache_control.cmp(&other.cache_control))
    }
}

impl std::hash::Hash for Index {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            name,
            url,
            explicit,
            default,
            origin: _,
            format,
            publish_url,
            authenticate,
            ignore_error_codes,
            cache_control,
        } = self;
        url.hash(state);
        name.hash(state);
        explicit.hash(state);
        default.hash(state);
        format.hash(state);
        publish_url.hash(state);
        authenticate.hash(state);
        ignore_error_codes.hash(state);
        cache_control.hash(state);
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

impl Index {
    /// Initialize an [`Index`] from a url
    pub fn new(url: IndexUrl) -> Self {
        Self {
            name: None,
            url,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
        }
    }

    /// Initialize an [`Index`] from a pip-style `--index-url`.
    pub fn from_index_url(url: IndexUrl) -> Self {
        Self::new(url).with_default()
    }

    /// Initialize an [`Index`] from a pip-style `--extra-index-url`.
    pub fn from_extra_index_url(url: IndexUrl) -> Self {
        Self::new(url)
    }

    /// Initialize an [`Index`] from a pip-style `--find-links`.
    pub fn from_find_links(url: IndexUrl) -> Self {
        Self::new(url).with_format(IndexFormat::Flat)
    }

    /// Try to initialise an index from a name and url CLI argument
    ///
    /// Returns `Ok(None)` if `s` didn't appear to match the right format.
    /// e.g.: `name=https://pypi.org/simple`
    pub fn try_from_named_cli(s: &str) -> Result<Option<Self>, IndexSourceError> {
        if let Some((name, url)) = s.split_once('=')
            && !name.chars().any(|c| c == ':')
        {
            let name = IndexName::from_str(name)?;
            let url = IndexUrl::from_str(url)?;
            Ok(Some(
                Self::new(url).with_name(name).with_origin(Origin::Cli),
            ))
        } else {
            Ok(None)
        }
    }

    /// Parse a default index passed on the command line
    pub fn from_default_index(s: &str) -> Result<Self, IndexSourceError> {
        // See if it looks like a source prefixed with a name
        if let Some(index) = Self::try_from_named_cli(s)? {
            return Ok(index.with_default());
        }

        // Otherwise, assume the source is a URL.
        let url = IndexUrl::from_str(s)?;
        Ok(Self::new(url).with_origin(Origin::Cli).with_default())
    }

    /// Set the [`IndexName`] of the index.
    #[must_use]
    pub fn with_name(mut self, name: IndexName) -> Self {
        self.name = Some(name);
        self
    }

    /// Set the index as a default
    #[must_use]
    pub fn with_default(mut self) -> Self {
        self.default = true;
        self
    }

    /// Set the [`Origin`] of the index.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Set the [`IndexFormat`] of the index.
    #[must_use]
    pub fn with_format(mut self, format: IndexFormat) -> Self {
        self.format = format;
        self
    }

    /// Return the [`IndexUrl`] of the index.
    pub fn url(&self) -> &IndexUrl {
        &self.url
    }

    /// Consume the [`Index`] and return the [`IndexUrl`].
    pub fn into_url(self) -> IndexUrl {
        self.url
    }

    /// Return the raw [`Url`] of the index.
    pub fn raw_url(&self) -> &DisplaySafeUrl {
        self.url.url()
    }

    /// Return the root [`Url`] of the index, if applicable.
    ///
    /// For indexes with a `/simple` endpoint, this is simply the URL with the final segment
    /// removed. This is useful, e.g., for credential propagation to other endpoints on the index.
    pub fn root_url(&self) -> Option<DisplaySafeUrl> {
        self.url.root()
    }

    /// Retrieve the credentials for the index, either from the environment, or from the URL itself.
    pub fn credentials(&self) -> Option<Credentials> {
        // If the index is named, and credentials are provided via the environment, prefer those.
        if let Some(name) = self.name.as_ref() {
            if let Some(credentials) = Credentials::from_env(name.to_env_var()) {
                return Some(credentials);
            }
        }

        // Otherwise, extract the credentials from the URL.
        Credentials::from_url(self.url.url())
    }

    /// Resolve the index relative to the given root directory.
    pub fn relative_to(mut self, root_dir: &Path) -> Result<Self, IndexUrlError> {
        if let IndexUrl::Path(ref url) = self.url {
            if let Some(given) = url.given() {
                self.url = IndexUrl::parse(given, Some(root_dir))?;
            }
        }
        Ok(self)
    }

    /// Return the [`IndexStatusCodeStrategy`] for this index.
    pub fn status_code_strategy(&self) -> IndexStatusCodeStrategy {
        if let Some(ignore_error_codes) = &self.ignore_error_codes {
            IndexStatusCodeStrategy::from_ignored_error_codes(ignore_error_codes)
        } else {
            IndexStatusCodeStrategy::from_index_url(self.url.url())
        }
    }

    /// Return the cache control header for file requests to this index, if any.
    pub fn artifact_cache_control(&self) -> Option<&str> {
        if let Some(artifact_cache_control) = self
            .cache_control
            .as_ref()
            .and_then(|cache_control| cache_control.files.as_deref())
        {
            Some(artifact_cache_control)
        } else {
            IndexCacheControl::artifact_cache_control(self.url.url())
        }
    }

    /// Return the cache control header for API requests to this index, if any.
    pub fn simple_api_cache_control(&self) -> Option<&str> {
        if let Some(api_cache_control) = self
            .cache_control
            .as_ref()
            .and_then(|cache_control| cache_control.api.as_deref())
        {
            Some(api_cache_control)
        } else {
            IndexCacheControl::simple_api_cache_control(self.url.url())
        }
    }
}

impl From<IndexUrl> for Index {
    fn from(value: IndexUrl) -> Self {
        Self {
            name: None,
            url: value,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
            cache_control: None,
        }
    }
}

/// A potentially unresolved index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexArg {
    Resolved(Index),
    Unresolved(IndexName),
}

#[derive(Debug, Error)]
#[error("Could not find an index named `{0}`")]
pub struct ResolveIndexArgError(IndexName);

impl IndexArg {
    /// Parse a non-default index passed on the command line
    pub fn from_cli(s: &str) -> Result<Self, IndexSourceError> {
        // See if it looks like a source prefixed with a name
        if let Some(index) = Index::try_from_named_cli(s)? {
            return Ok(Self::Resolved(index));
        }

        // Consider if it could be just a name
        if let Ok(name) = IndexName::from_str(s) {
            return Ok(Self::Unresolved(name));
        }

        // Otherwise, assume the source is a URL.
        let url = IndexUrl::from_str(s)?;
        Ok(Self::Resolved(Index::new(url).with_origin(Origin::Cli)))
    }

    /// Converts from [`IndexArg`] to [`Option<Index>`].
    ///
    /// Useful when filtering out unresolved indices.
    pub fn index(self) -> Option<Index> {
        match self {
            Self::Resolved(index) => Some(index),
            Self::Unresolved(_) => None,
        }
    }

    /// Attempt to look up the [`IndexArg`] in the passed list of indexes
    ///
    /// The origin is inherited from the index
    pub fn try_resolve<I>(self, indexes: I) -> Result<Index, ResolveIndexArgError>
    where
        I: IntoIterator,
        I::Item: Borrow<Index>,
    {
        match self {
            Self::Resolved(index) => Ok(index),
            Self::Unresolved(unresolved) => {
                if let Some(index) = indexes
                    .into_iter()
                    .find(|index| index.borrow().name.as_ref() == Some(&unresolved))
                {
                    Ok(Index {
                        ..index.borrow().clone()
                    })
                } else {
                    Err(ResolveIndexArgError(unresolved))
                }
            }
        }
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
    /// Return a reference to the [`IndexMetadata`].
    pub fn as_ref(&self) -> IndexMetadataRef<'_> {
        let Self { url, format: kind } = self;
        IndexMetadataRef { url, format: *kind }
    }

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

impl IndexMetadataRef<'_> {
    /// Return the [`IndexUrl`] of the index.
    pub fn url(&self) -> &IndexUrl {
        self.url
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
    use super::*;

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
        let cache_control = index.cache_control.as_ref().unwrap();
        assert_eq!(cache_control.api.as_deref(), Some("max-age=600"));
        assert_eq!(cache_control.files.as_deref(), Some("max-age=3600"));
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
        let cache_control = index.cache_control.as_ref().unwrap();
        assert_eq!(cache_control.api.as_deref(), Some("max-age=300"));
        assert_eq!(cache_control.files, None);
    }
}
