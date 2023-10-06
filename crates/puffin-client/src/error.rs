use thiserror::Error;
use url::Url;

use puffin_package::metadata;

#[derive(Debug, Error)]
pub enum PypiClientError {
    /// An invalid URL was provided.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{1}` was not found in registry {0}.")]
    PackageNotFound(Url, String),

    /// The metadata file could not be parsed.
    #[error(transparent)]
    MetadataParseError(#[from] metadata::Error),

    /// The metadata file was not found in the registry.
    #[error("File `{1}` was not found in registry {0}.")]
    FileNotFound(Url, String),

    /// A generic request error happened while making a request. Refer to the
    /// error message for more details.
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    /// A generic request middleware error happened while making a request.
    /// Refer to the error message for more details.
    #[error(transparent)]
    RequestMiddlewareError(#[from] reqwest_middleware::Error),

    #[error("Received some unexpected JSON: {source}")]
    BadJson {
        source: serde_json::Error,
        url: String,
    },
}

impl PypiClientError {
    pub fn from_json_err(err: serde_json::Error, url: String) -> Self {
        Self::BadJson { source: err, url }
    }
}
