use thiserror::Error;
use url::Url;

use puffin_package::metadata;

#[derive(Debug, Error)]
pub enum Error {
    /// An invalid URL was provided.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{0}` was not found in the registry.")]
    PackageNotFound(String),

    /// The metadata file could not be parsed.
    #[error(transparent)]
    MetadataParseError(#[from] metadata::Error),

    /// The metadata file was not found in the registry.
    #[error("File `{0}` was not found in the registry.")]
    FileNotFound(String),

    /// The resource was not found in the registry.
    #[error("Resource `{0}` was not found in the registry.")]
    ResourceNotFound(Url),

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

impl Error {
    pub fn from_json_err(err: serde_json::Error, url: String) -> Self {
        Self::BadJson { source: err, url }
    }
}
