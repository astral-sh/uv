use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;
use url::Url;

use distribution_filename::{WheelFilename, WheelFilenameError};
use uv_normalize::PackageName;

use crate::html;
use crate::middleware::OfflineError;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error {
    kind: Box<ErrorKind>,
}

impl Error {
    pub fn into_kind(self) -> ErrorKind {
        *self.kind
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub(crate) fn from_json_err(err: serde_json::Error, url: Url) -> Self {
        ErrorKind::BadJson { source: err, url }.into()
    }

    pub(crate) fn from_html_err(err: html::Error, url: Url) -> Self {
        ErrorKind::BadHtml { source: err, url }.into()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error {
            kind: Box::new(kind),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    /// An invalid URL was provided.
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    /// A base URL could not be joined with a possibly relative URL.
    #[error(transparent)]
    JoinRelativeError(#[from] pypi_types::JoinRelativeError),

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
    WheelFilename(#[source] WheelFilenameError),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("The wheel {0} is not a valid zip file")]
    Zip(WheelFilename, #[source] ZipError),

    #[error("Failed to write to the client cache")]
    CacheWrite(#[source] std::io::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Cache deserialization failed")]
    Decode(#[source] rmp_serde::decode::Error),

    #[error("Cache serialization failed")]
    Encode(#[source] rmp_serde::encode::Error),

    /// An [`io::Error`] with a filename attached
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),

    #[error("Missing `Content-Type` header for {0}")]
    MissingContentType(Url),

    #[error("Invalid `Content-Type` header for {0}")]
    InvalidContentTypeHeader(Url, #[source] http::header::ToStrError),

    #[error("Unsupported `Content-Type` \"{1}\" for {0}. Expected JSON or HTML.")]
    UnsupportedMediaType(Url, String),

    #[error("Reading from cache archive failed: {0}")]
    ArchiveRead(String),

    #[error("Writing to cache archive failed: {0}")]
    ArchiveWrite(#[source] crate::rkyvutil::SerializerError),

    #[error("Network connectivity is disabled, but the requested data wasn't found in the cache for: `{0}`")]
    Offline(String),
}

impl ErrorKind {
    pub(crate) fn from_middleware(err: reqwest_middleware::Error) -> Self {
        if let reqwest_middleware::Error::Middleware(ref underlying) = err {
            if let Some(err) = underlying.downcast_ref::<OfflineError>() {
                return ErrorKind::Offline(err.url().to_string());
            }
        }

        if let reqwest_middleware::Error::Reqwest(err) = err {
            return ErrorKind::RequestError(err);
        }

        ErrorKind::RequestMiddlewareError(err)
    }
}
