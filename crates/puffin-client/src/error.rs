use std::io;

use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;
use thiserror::Error;
use url::Url;

use distribution_filename::{WheelFilename, WheelFilenameError};
use puffin_normalize::PackageName;

#[derive(Debug, Error)]
pub enum Error {
    /// An invalid URL was provided.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    /// Dist-info error
    #[error(transparent)]
    InstallWheel(#[from] install_wheel_rs::Error),

    #[error("{0} isn't available locally, but making network requests to registries was banned.")]
    NoIndex(String),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{0}` was not found in the registry.")]
    PackageNotFound(String),

    /// The metadata file could not be parsed.
    #[error("Couldn't parse metadata of {0} from {1}")]
    MetadataParseError(WheelFilename, String, #[source] pypi_types::Error),

    /// The metadata file was not found in the registry.
    #[error("File `{0}` was not found in the registry at {1}.")]
    FileNotFound(String, #[source] reqwest::Error),

    /// A generic request error happened while making a request. Refer to the
    /// error message for more details.
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    /// A generic request middleware error happened while making a request.
    /// Refer to the error message for more details.
    #[error(transparent)]
    RequestMiddlewareError(#[from] reqwest_middleware::Error),

    #[error("Received some unexpected JSON from {url}")]
    BadJson { source: serde_json::Error, url: Url },

    #[error(transparent)]
    AsyncHttpRangeReader(#[from] AsyncHttpRangeReaderError),

    #[error("Expected a single .dist-info directory in {0}, found {1}")]
    InvalidDistInfo(WheelFilename, String),

    #[error("{0} is not a valid wheel filename")]
    WheelFilename(#[from] WheelFilenameError),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("The wheel {0} is not a valid zip file")]
    Zip(WheelFilename, #[source] ZipError),

    #[error("Failed to write to the client cache")]
    IO(#[from] io::Error),

    /// An [`io::Error`] with a filename attached
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),

    #[error("Failed to serialize response to cache")]
    SerdeJson(#[from] serde_json::Error),
}

impl Error {
    pub fn from_json_err(err: serde_json::Error, url: Url) -> Self {
        Self::BadJson { source: err, url }
    }
}
