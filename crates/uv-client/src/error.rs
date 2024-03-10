use std::fmt::{Display, Formatter};
use std::ops::Deref;

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
    /// Convert this error into its [`ErrorKind`] variant.
    pub fn into_kind(self) -> ErrorKind {
        *self.kind
    }

    /// Create a new error from a JSON parsing error.
    pub(crate) fn from_json_err(err: serde_json::Error, url: Url) -> Self {
        ErrorKind::BadJson { source: err, url }.into()
    }

    /// Create a new error from an HTML parsing error.
    pub(crate) fn from_html_err(err: html::Error, url: Url) -> Self {
        ErrorKind::BadHtml { source: err, url }.into()
    }

    /// Returns `true` if this error corresponds to an offline error.
    pub(crate) fn is_offline(&self) -> bool {
        matches!(&*self.kind, ErrorKind::Offline(_))
    }

    /// Returns `true` if this error corresponds to an I/O "not found" error.
    pub(crate) fn is_file_not_exists(&self) -> bool {
        let ErrorKind::Io(ref err) = &*self.kind else {
            return false;
        };
        matches!(err.kind(), std::io::ErrorKind::NotFound)
    }

    /// Returns `true` if the error is due to the server not supporting HTTP range requests.
    pub fn is_http_range_requests_unsupported(&self) -> bool {
        match &*self.kind {
            // The server doesn't support range requests (as reported by the `HEAD` check).
            ErrorKind::AsyncHttpRangeReader(
                AsyncHttpRangeReaderError::HttpRangeRequestUnsupported,
            ) => {
                return true;
            }

            // The server returned a "Method Not Allowed" error, indicating it doesn't support
            // HEAD requests, so we can't check for range requests.
            ErrorKind::ReqwestError(err) => {
                if let Some(status) = err.status() {
                    // If the server doesn't support HEAD requests, we can't check for range
                    // requests.
                    if status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
                        return true;
                    }

                    // In some cases, registries return a 404 for HEAD requests when they're not
                    // supported. In the worst case, we'll now just proceed to attempt to stream the
                    // entire file, so it's fine to be somewhat lenient here.
                    if status == reqwest::StatusCode::NOT_FOUND {
                        return true;
                    }
                }
            }

            // The server doesn't support range requests, but we only discovered this while
            // unzipping due to erroneous server behavior.
            ErrorKind::Zip(_, ZipError::UpstreamReadError(err)) => {
                if let Some(inner) = err.get_ref() {
                    if let Some(inner) = inner.downcast_ref::<AsyncHttpRangeReaderError>() {
                        if matches!(
                            inner,
                            AsyncHttpRangeReaderError::HttpRangeRequestUnsupported
                        ) {
                            return true;
                        }
                    }
                }
            }

            _ => {}
        }

        false
    }

    /// Returns `true` if the error is due to the server not supporting HTTP streaming. Most
    /// commonly, this is due to serving ZIP files with features that are incompatible with
    /// streaming, like data descriptors.
    pub fn is_http_streaming_unsupported(&self) -> bool {
        matches!(
            &*self.kind,
            ErrorKind::Zip(_, ZipError::FeatureNotSupported(_))
        )
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
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

    /// The metadata file was not found in the wheel.
    #[error("Metadata file `{0}` was not found in {1}")]
    MetadataNotFound(WheelFilename, String),

    /// The metadata file was not found in the registry.
    #[error("File `{0}` was not found in the registry at {1}.")]
    FileNotFound(String, #[source] BetterReqwestError),

    /// A generic request error happened while making a request. Refer to the
    /// error message for more details.
    #[error(transparent)]
    ReqwestError(#[from] BetterReqwestError),

    /// A generic request middleware error happened while making a request.
    /// Refer to the error message for more details.
    #[error(transparent)]
    ReqwestMiddlewareError(#[from] anyhow::Error),

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

    #[error("Failed to unzip wheel: {0}")]
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

impl From<reqwest::Error> for ErrorKind {
    fn from(error: reqwest::Error) -> Self {
        Self::ReqwestError(BetterReqwestError::from(error))
    }
}
impl From<reqwest_middleware::Error> for ErrorKind {
    fn from(error: reqwest_middleware::Error) -> Self {
        if let reqwest_middleware::Error::Middleware(ref underlying) = error {
            if let Some(err) = underlying.downcast_ref::<OfflineError>() {
                return Self::Offline(err.url().to_string());
            }
        }

        match error {
            reqwest_middleware::Error::Middleware(err) => Self::ReqwestMiddlewareError(err),
            reqwest_middleware::Error::Reqwest(err) => Self::from(err),
        }
    }
}

/// Handle the case with no internet by explicitly telling the user instead of showing an obscure
/// DNS error.
#[derive(Debug)]
pub struct BetterReqwestError(reqwest::Error);

impl BetterReqwestError {
    fn is_likely_offline(&self) -> bool {
        if !self.0.is_connect() {
            return false;
        }
        // Self is "error sending request for url", the first source is "error trying to connect",
        // the second source is "dns error". We have to check for the string because hyper errors
        // are opaque.
        std::error::Error::source(&self.0)
            .and_then(|err| err.source())
            .is_some_and(|err| err.to_string().starts_with("dns error: "))
    }
}

impl From<reqwest::Error> for BetterReqwestError {
    fn from(error: reqwest::Error) -> Self {
        Self(error)
    }
}

impl Deref for BetterReqwestError {
    type Target = reqwest::Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for BetterReqwestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_likely_offline() {
            f.write_str("Could not connect, are you offline?")
        } else {
            Display::fmt(&self.0, f)
        }
    }
}

impl std::error::Error for BetterReqwestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if self.is_likely_offline() {
            Some(&self.0)
        } else {
            self.0.source()
        }
    }
}
