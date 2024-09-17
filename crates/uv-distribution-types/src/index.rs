use crate::{IndexUrl, IndexUrlError};
use std::str::FromStr;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Hash, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
    pub name: Option<String>,
    /// The URL of the index.
    ///
    /// Expects to receive a URL (e.g., `https://pypi.org/simple`) or a local path.
    pub url: IndexUrl,
    /// Mark the index as explicit.
    ///
    /// Explicit indexes will _only_ be used when explicitly enabled via a `[tool.uv.sources]`
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
    // /// The type of the index.
    // ///
    // /// Indexes can either be PEP 503-compliant (i.e., a registry implementing the Simple API) or
    // /// structured as a flat list of distributions (e.g., `--find-links`). In both cases, indexes
    // /// can point to either local or remote resources.
    // #[serde(default)]
    // pub r#type: IndexKind,
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
        }
    }

    /// Initialize an [`Index`] from a pip-style `--extra-index-url`.
    pub fn from_extra_index_url(url: IndexUrl) -> Self {
        Self {
            url,
            name: None,
            explicit: false,
            default: false,
        }
    }

    /// Return the [`IndexUrl`] of the index.
    pub fn url(&self) -> &IndexUrl {
        &self.url
    }

    /// Return the raw [`URL`] of the index.
    pub fn raw_url(&self) -> &Url {
        self.url.url()
    }
}

impl FromStr for Index {
    type Err = IndexSourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Determine whether the source is prefixed with a name, as in `name=https://pypi.org/simple`.
        if let Some((name, url)) = s.split_once('=') {
            if name.is_empty() {
                return Err(IndexSourceError::EmptyName);
            }

            if name.chars().all(char::is_alphanumeric) {
                let url = IndexUrl::from_str(url)?;
                return Ok(Self {
                    name: Some(name.to_string()),
                    url,
                    explicit: false,
                    default: false,
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
        })
    }
}

/// An error that can occur when parsing an [`Index`].
#[derive(Error, Debug)]
pub enum IndexSourceError {
    #[error(transparent)]
    Url(#[from] IndexUrlError),
    #[error("Index included a name, but the name was empty")]
    EmptyName,
}
