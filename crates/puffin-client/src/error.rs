use std::io;

use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;
use thiserror::Error;
use url::Url;

use crate::html;
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
    MetadataParseError(WheelFilename, String, #[source] Box<pypi_types::Error>),

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

    #[error("Received some unexpected HTML from {url}")]
    BadHtml { source: html::Error, url: Url },

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
    CacheWrite(#[source] io::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("Cache deserialization failed")]
    Decode(#[from] rmp_serde::decode::Error),

    #[error("Cache serialization failed")]
    Encode(#[from] rmp_serde::encode::Error),

    /// An [`io::Error`] with a filename attached
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),

    #[error("Missing `Content-Type` header for {0}")]
    MissingContentType(Url),

    #[error("Invalid `Content-Type` header for {0}")]
    InvalidContentTypeHeader(Url, #[source] http::header::ToStrError),

    #[error("Unsupported `Content-Type` \"{1}\" for {0}. Expected JSON or HTML.")]
    UnsupportedMediaType(Url, String),

    #[error("Failed to read find links directory")]
    FindLinks(#[source] io::Error),
}

impl Error {
    pub(crate) fn from_json_err(err: serde_json::Error, url: Url) -> Self {
        Self::BadJson { source: err, url }
    }

    pub(crate) fn from_html_err(err: html::Error, url: Url) -> Self {
        Self::BadHtml { source: err, url }
    }
}
