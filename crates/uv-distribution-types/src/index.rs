use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use uv_auth::{AuthPolicy, Credentials};
use uv_redacted::DisplaySafeUrl;

use crate::index_name::{IndexName, IndexNameError};
use crate::origin::Origin;
use crate::{IndexStatusCodeStrategy, IndexUrl, IndexUrlError, SerializableStatusCode};

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    #[serde(rename = "publish-url")]
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
    #[serde(default, rename = "ignore-error-codes")]
    pub ignore_error_codes: Option<Vec<SerializableStatusCode>>,
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
    /// Initialize an [`Index`] from a pip-style `--index-url`.
    pub fn from_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            explicit: false,
            default: true,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
        }
    }

    /// Initialize an [`Index`] from a pip-style `--extra-index-url`.
    pub fn from_extra_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
        }
    }

    /// Initialize an [`Index`] from a pip-style `--find-links`.
    pub fn from_find_links(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Flat,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
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
        }
    }
}

impl FromStr for Index {
    type Err = IndexSourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Determine whether the source is prefixed with a name, as in `name=https://pypi.org/simple`.
        if let Some((name, url)) = s.split_once('=') {
            if !name.chars().any(|c| c == ':') {
                let name = IndexName::from_str(name)?;
                let url = IndexUrl::from_str(url)?;
                return Ok(Self {
                    name: Some(name),
                    url,
                    explicit: false,
                    default: false,
                    origin: None,
                    format: IndexFormat::Simple,
                    publish_url: None,
                    authenticate: AuthPolicy::default(),
                    ignore_error_codes: None,
                });
            }
        }

        // Otherwise, assume the source is a URL.
        let url = IndexUrl::from_str(s)?;
        Ok(Self {
            name: None,
            url,
            explicit: false,
            default: false,
            origin: None,
            format: IndexFormat::Simple,
            publish_url: None,
            authenticate: AuthPolicy::default(),
            ignore_error_codes: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_environment_variable_expansion() {
        // Test URL with environment variables that have defaults
        // This way we don't need to set environment variables
        let toml_content = r#"
            name = "test-index"
            url = "https://${TEST_INDEX_HOST:-example.com}:${TEST_INDEX_PORT:-8080}/simple"
            publish-url = "https://${TEST_PUBLISH_HOST:-upload.example.com}/upload"
            explicit = true
        "#;

        let index: Index = toml::from_str(toml_content).expect("Failed to deserialize index");

        assert_eq!(index.name.as_ref().unwrap().as_ref(), "test-index");
        assert_eq!(index.url.to_string(), "https://example.com:8080/simple");
        assert_eq!(
            index.publish_url.as_ref().unwrap().to_string(),
            "https://upload.example.com/upload"
        );
        assert!(index.explicit);
    }

    #[test]
    fn test_index_without_environment_variables() {
        // Test normal URL without environment variables
        let toml_content = r#"
            name = "normal-index"
            url = "https://pypi.org/simple"
            default = true
        "#;

        let index: Index = toml::from_str(toml_content).expect("Failed to deserialize index");

        assert_eq!(index.name.as_ref().unwrap().as_ref(), "normal-index");
        assert_eq!(index.url.to_string(), "https://pypi.org/simple");
        assert!(index.default);
        assert!(!index.explicit);
    }

    #[test]
    fn test_index_missing_environment_variable() {
        // Test with missing environment variable - should fail gracefully
        let toml_content = r#"
            name = "missing-var-index"
            url = "https://${MISSING_VAR}/simple"
        "#;

        let result: Result<Index, _> = toml::from_str(toml_content);
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Failed to expand environment variables"));
    }

    #[test]
    fn test_index_tilde_expansion() {
        // Test tilde expansion for local paths
        let toml_content = r#"
            name = "local-index"
            url = "~/my-index"
        "#;

        let index: Index = toml::from_str(toml_content).expect("Failed to deserialize index");

        assert_eq!(index.name.as_ref().unwrap().as_ref(), "local-index");
        // The URL should have tilde expanded to the home directory
        let url_str = index.url.to_string();
        assert!(
            !url_str.contains('~'),
            "Tilde should be expanded: {url_str}"
        );
    }

    #[test]
    fn test_index_home_environment_variable() {
        // Test using HOME environment variable which should always exist
        let toml_content = r#"
            name = "home-index"
            url = "file://${HOME}/my-local-packages"
        "#;

        let index: Index = toml::from_str(toml_content).expect("Failed to deserialize index");

        assert_eq!(index.name.as_ref().unwrap().as_ref(), "home-index");
        // The URL should have HOME expanded to the actual home directory
        let url_str = index.url.to_string();
        assert!(url_str.starts_with("file://"));
        assert!(!url_str.contains("${HOME}"));
        assert!(url_str.contains("/my-local-packages"));
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

impl<'de> Deserialize<'de> for Index {
    fn deserialize<D>(deserializer: D) -> Result<Index, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct IndexRaw {
            name: Option<IndexName>,
            url: String,
            #[serde(default)]
            explicit: bool,
            #[serde(default)]
            default: bool,
            #[serde(default)]
            format: IndexFormat,
            publish_url: Option<String>,
            #[serde(default)]
            authenticate: AuthPolicy,
            #[serde(default)]
            ignore_error_codes: Option<Vec<SerializableStatusCode>>,
        }

        let raw = IndexRaw::deserialize(deserializer)?;

        // Expand environment variables in the URL
        let expanded_url = shellexpand::full(&raw.url).map_err(|e| {
            D::Error::custom(format!(
                "Failed to expand environment variables in URL '{}': {}",
                raw.url, e
            ))
        })?;

        // Parse the expanded URL
        let url = IndexUrl::parse(&expanded_url, None).map_err(|e| {
            D::Error::custom(format!("Failed to parse URL '{expanded_url}': {e}"))
        })?;

        // Expand environment variables in publish_url if present
        let publish_url = if let Some(publish_url_str) = raw.publish_url {
            let expanded_publish_url = shellexpand::full(&publish_url_str).map_err(|e| {
                D::Error::custom(format!(
                    "Failed to expand environment variables in publish URL '{publish_url_str}': {e}"
                ))
            })?;

            Some(expanded_publish_url.parse().map_err(|e| {
                D::Error::custom(format!(
                    "Failed to parse publish URL '{expanded_publish_url}': {e}"
                ))
            })?)
        } else {
            None
        };

        Ok(Index {
            name: raw.name,
            url,
            explicit: raw.explicit,
            default: raw.default,
            origin: None,
            format: raw.format,
            publish_url,
            authenticate: raw.authenticate,
            ignore_error_codes: raw.ignore_error_codes,
        })
    }
}
