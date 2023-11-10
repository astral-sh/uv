use std::io;

use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;
use thiserror::Error;

use distribution_filename::{WheelFilename, WheelFilenameError};

#[derive(Debug, Error)]
pub enum Error {
    /// An invalid URL was provided.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error("{0} isn't available locally, but making network requests to registries was banned.")]
    NoIndex(String),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{0}` was not found in the registry.")]
    PackageNotFound(String),

    /// The metadata file could not be parsed.
    #[error(transparent)]
    MetadataParseError(#[from] pypi_types::Error),

    /// The metadata file was not found in the registry.
    #[error("File `{0}` was not found in the registry at {1}.")]
    FileNotFound(String, #[source] reqwest_middleware::Error),

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

    #[error(transparent)]
    AsyncHttpRangeReader(#[from] AsyncHttpRangeReaderError),

    #[error("Expected a single .dist-info directory in {0}, found {1}")]
    InvalidDistInfo(WheelFilename, String),

    #[error("{0} is not a valid wheel filename")]
    WheelFilename(#[from] WheelFilenameError),

    #[error("The wheel {0} is not a valid zip file")]
    Zip(WheelFilename, #[source] ZipError),

    #[error(transparent)]
    IO(#[from] io::Error),
}

impl Error {
    pub fn from_json_err(err: serde_json::Error, url: String) -> Self {
        Self::BadJson { source: err, url }
    }
}
