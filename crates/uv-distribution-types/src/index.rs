use std::path::Path;
use std::str::FromStr;

use thiserror::Error;
use url::Url;

use uv_auth::{AuthPolicy, Credentials};

use crate::index_name::{IndexName, IndexNameError};
use crate::origin::Origin;
use crate::{IndexUrl, IndexUrlError};

#[derive(Debug, Clone, Hash, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
    // /// The type of the index.
    // ///
    // /// Indexes can either be PEP 503-compliant (i.e., a registry implementing the Simple API) or
    // /// structured as a flat list of distributions (e.g., `--find-links`). In both cases, indexes
    // /// can point to either local or remote resources.
    // #[serde(default)]
    // pub r#type: IndexKind,
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
    pub publish_url: Option<Url>,
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
}

// #[derive(
//     Default, Debug, Copy, Clone, Hash, Eq, PartialEq, serde::Serialize, serde::Deserialize,
// )]
// #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
// pub enum IndexKind {
//     /// A PEP 503 and/or PEP 691-compliant index.
//     #[default]
//     Simple,
//     /// An index containing a list of links to distributions (e.g., `--find-links`).
//     Flat,
// }

impl Index {
    /// Initialize an [`Index`] from a pip-style `--index-url`.
    pub fn from_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            explicit: false,
            default: true,
            origin: None,
            publish_url: None,
            authenticate: AuthPolicy::default(),
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
            publish_url: None,
            authenticate: AuthPolicy::default(),
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
            publish_url: None,
            authenticate: AuthPolicy::default(),
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
    pub fn raw_url(&self) -> &Url {
        self.url.url()
    }

    /// Return the root [`Url`] of the index, if applicable.
    ///
    /// For indexes with a `/simple` endpoint, this is simply the URL with the final segment
    /// removed. This is useful, e.g., for credential propagation to other endpoints on the index.
    pub fn root_url(&self) -> Option<Url> {
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
                    publish_url: None,
                    authenticate: AuthPolicy::default(),
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
            publish_url: None,
            authenticate: AuthPolicy::default(),
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
