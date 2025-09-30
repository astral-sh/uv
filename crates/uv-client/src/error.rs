use std::fmt::{Display, Formatter};
use std::ops::Deref;

use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;

use uv_distribution_filename::{WheelFilename, WheelFilenameError};
use uv_normalize::PackageName;
use uv_redacted::DisplaySafeUrl;

use crate::middleware::OfflineError;
use crate::{FlatIndexError, html};

#[derive(Debug)]
pub struct Error {
    kind: Box<ErrorKind>,
    retries: u32,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.retries > 0 {
            write!(
                f,
                "Request failed after {retries} retries",
                retries = self.retries
            )
        } else {
            Display::fmt(&self.kind, f)
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if self.retries > 0 {
            Some(&self.kind)
        } else {
            self.kind.source()
        }
    }
}

impl Error {
    /// Create a new [`Error`] with the given [`ErrorKind`] and number of retries.
    pub fn new(kind: ErrorKind, retries: u32) -> Self {
        Self {
            kind: Box::new(kind),
            retries,
        }
    }

    /// Return the number of retries that were attempted before this error was returned.
    pub fn retries(&self) -> u32 {
        self.retries
    }

    /// Convert this error into an [`ErrorKind`].
    pub fn into_kind(self) -> ErrorKind {
        *self.kind
    }

    /// Return the [`ErrorKind`] of this error.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Create a new error from a JSON parsing error.
    pub(crate) fn from_json_err(err: serde_json::Error, url: DisplaySafeUrl) -> Self {
        ErrorKind::BadJson { source: err, url }.into()
    }

    /// Create a new error from an HTML parsing error.
    pub(crate) fn from_html_err(err: html::Error, url: DisplaySafeUrl) -> Self {
        ErrorKind::BadHtml { source: err, url }.into()
    }

    /// Create a new error from a `MessagePack` parsing error.
    pub(crate) fn from_msgpack_err(err: rmp_serde::decode::Error, url: DisplaySafeUrl) -> Self {
        ErrorKind::BadMessagePack { source: err, url }.into()
    }

    /// Returns `true` if this error corresponds to an offline error.
    pub(crate) fn is_offline(&self) -> bool {
        matches!(&*self.kind, ErrorKind::Offline(_))
    }

    /// Returns `true` if this error corresponds to an I/O "not found" error.
    pub(crate) fn is_file_not_exists(&self) -> bool {
        let ErrorKind::Io(err) = &*self.kind else {
            return false;
        };
        matches!(err.kind(), std::io::ErrorKind::NotFound)
    }

    /// Returns `true` if the error is due to an SSL error.
    pub fn is_ssl(&self) -> bool {
        matches!(&*self.kind, ErrorKind::WrappedReqwestError(.., err) if err.is_ssl())
    }

    /// Returns `true` if the error is due to the server not supporting HTTP range requests.
    pub fn is_http_range_requests_unsupported(&self) -> bool {
        match &*self.kind {
            // The server doesn't support range requests (as reported by the `HEAD` check).
            ErrorKind::AsyncHttpRangeReader(
                _,
                AsyncHttpRangeReaderError::HttpRangeRequestUnsupported,
            ) => {
                return true;
            }

            // The server doesn't support range requests (it doesn't return the necessary headers).
            ErrorKind::AsyncHttpRangeReader(
                _,
                AsyncHttpRangeReaderError::ContentLengthMissing
                | AsyncHttpRangeReaderError::ContentRangeMissing,
            ) => {
                return true;
            }

            // The server returned a "Method Not Allowed" error, indicating it doesn't support
            // HEAD requests, so we can't check for range requests.
            ErrorKind::WrappedReqwestError(_, err) => {
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

                    // In some cases, registries (like PyPICloud) return a 403 for HEAD requests
                    // when they're not supported. Again, it's better to be lenient here.
                    if status == reqwest::StatusCode::FORBIDDEN {
                        return true;
                    }

                    // In some cases, registries (like Alibaba Cloud) return a 400 for HEAD requests
                    // when they're not supported. Again, it's better to be lenient here.
                    if status == reqwest::StatusCode::BAD_REQUEST {
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
            retries: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(transparent)]
    InvalidUrl(#[from] uv_distribution_types::ToUrlError),

    #[error(transparent)]
    Flat(#[from] FlatIndexError),

    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(DisplaySafeUrl),

    #[error("Expected an index URL, but received non-base URL: {0}")]
    CannotBeABase(DisplaySafeUrl),

    #[error("Failed to read metadata: `{0}`")]
    Metadata(String, #[source] uv_metadata::Error),

    #[error("{0} isn't available locally, but making network requests to registries was banned")]
    NoIndex(String),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{0}` was not found in the registry")]
    PackageNotFound(String),

    /// The package was not found in the local (file-based) index.
    #[error("Package `{0}` was not found in the local index")]
    FileNotFound(String),

    /// The metadata file could not be parsed.
    #[error("Couldn't parse metadata of {0} from {1}")]
    MetadataParseError(
        WheelFilename,
        String,
        #[source] Box<uv_pypi_types::MetadataError>,
    ),

    /// The metadata file was not found in the wheel.
    #[error("Metadata file `{0}` was not found in {1}")]
    MetadataNotFound(WheelFilename, String),

    /// An error that happened while making a request or in a reqwest middleware.
    #[error("Failed to fetch: `{0}`")]
    WrappedReqwestError(DisplaySafeUrl, #[source] WrappedReqwestError),

    /// Add the number of failed retries to the error.
    #[error("Request failed after {retries} retries")]
    RequestWithRetries {
        source: Box<ErrorKind>,
        retries: u32,
    },

    #[error("Received some unexpected JSON from {}", url)]
    BadJson {
        source: serde_json::Error,
        url: DisplaySafeUrl,
    },

    #[error("Received some unexpected HTML from {}", url)]
    BadHtml {
        source: html::Error,
        url: DisplaySafeUrl,
    },

    #[error("Received some unexpected MessagePack from {}", url)]
    BadMessagePack {
        source: rmp_serde::decode::Error,
        url: DisplaySafeUrl,
    },

    #[error("Failed to read zip with range requests: `{0}`")]
    AsyncHttpRangeReader(DisplaySafeUrl, #[source] AsyncHttpRangeReaderError),

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
    Io(std::io::Error),

    #[error("Cache deserialization failed")]
    Decode(#[source] rmp_serde::decode::Error),

    #[error("Cache serialization failed")]
    Encode(#[source] rmp_serde::encode::Error),

    #[error("Missing `Content-Type` header for {0}")]
    MissingContentType(DisplaySafeUrl),

    #[error("Invalid `Content-Type` header for {0}")]
    InvalidContentTypeHeader(DisplaySafeUrl, #[source] http::header::ToStrError),

    #[error("Unsupported `Content-Type` \"{1}\" for {0}. Expected JSON or HTML.")]
    UnsupportedMediaType(DisplaySafeUrl, String),

    #[error("Reading from cache archive failed: {0}")]
    ArchiveRead(String),

    #[error("Writing to cache archive failed: {0}")]
    ArchiveWrite(String),

    #[error(
        "Network connectivity is disabled, but the requested data wasn't found in the cache for: `{0}`"
    )]
    Offline(String),

    #[error("Invalid cache control header: `{0}`")]
    InvalidCacheControl(String),

    #[error("Invalid variants.json format: {0}")]
    VariantsJsonFormat(DisplaySafeUrl, #[source] serde_json::Error),
}

impl ErrorKind {
    /// Create an [`ErrorKind`] from a [`reqwest::Error`].
    pub(crate) fn from_reqwest(url: DisplaySafeUrl, error: reqwest::Error) -> Self {
        Self::WrappedReqwestError(url, WrappedReqwestError::from(error))
    }

    /// Create an [`ErrorKind`] from a [`reqwest_middleware::Error`].
    pub(crate) fn from_reqwest_middleware(
        url: DisplaySafeUrl,
        err: reqwest_middleware::Error,
    ) -> Self {
        if let reqwest_middleware::Error::Middleware(ref underlying) = err {
            if let Some(err) = underlying.downcast_ref::<OfflineError>() {
                return Self::Offline(err.url().to_string());
            }
        }

        Self::WrappedReqwestError(url, WrappedReqwestError(err))
    }
}

/// Handle the case with no internet by explicitly telling the user instead of showing an obscure
/// DNS error.
///
/// Wraps a [`reqwest_middleware::Error`] instead of an [`reqwest::Error`] since the actual reqwest
/// error may be below some context in the [`anyhow::Error`].
#[derive(Debug)]
pub struct WrappedReqwestError(reqwest_middleware::Error);

impl WrappedReqwestError {
    /// Return the inner [`reqwest::Error`] from the error chain, if it exists.
    fn inner(&self) -> Option<&reqwest::Error> {
        match &self.0 {
            reqwest_middleware::Error::Reqwest(err) => Some(err),
            reqwest_middleware::Error::Middleware(err) => err.chain().find_map(|err| {
                if let Some(err) = err.downcast_ref::<reqwest::Error>() {
                    Some(err)
                } else if let Some(reqwest_middleware::Error::Reqwest(err)) =
                    err.downcast_ref::<reqwest_middleware::Error>()
                {
                    Some(err)
                } else {
                    None
                }
            }),
        }
    }

    /// Check if the error chain contains a `reqwest` error that looks like this:
    /// * error sending request for url (...)
    /// * client error (Connect)
    /// * dns error: failed to lookup address information: Name or service not known
    /// * failed to lookup address information: Name or service not known
    fn is_likely_offline(&self) -> bool {
        if let Some(reqwest_err) = self.inner() {
            if !reqwest_err.is_connect() {
                return false;
            }
            // Self is "error sending request for url", the first source is "error trying to connect",
            // the second source is "dns error". We have to check for the string because hyper errors
            // are opaque.
            if std::error::Error::source(&reqwest_err)
                .and_then(|err| err.source())
                .is_some_and(|err| err.to_string().starts_with("dns error: "))
            {
                return true;
            }
        }
        false
    }

    /// Check if the error chain contains a `reqwest` error that looks like this:
    /// * invalid peer certificate: `UnknownIssuer`
    fn is_ssl(&self) -> bool {
        if let Some(reqwest_err) = self.inner() {
            if !reqwest_err.is_connect() {
                return false;
            }
            // Self is "error sending request for url", the first source is "error trying to connect",
            // the second source is "dns error". We have to check for the string because hyper errors
            // are opaque.
            if std::error::Error::source(&reqwest_err)
                .and_then(|err| err.source())
                .is_some_and(|err| err.to_string().starts_with("invalid peer certificate: "))
            {
                return true;
            }
        }
        false
    }
}

impl From<reqwest::Error> for WrappedReqwestError {
    fn from(error: reqwest::Error) -> Self {
        Self(error.into())
    }
}

impl From<reqwest_middleware::Error> for WrappedReqwestError {
    fn from(error: reqwest_middleware::Error) -> Self {
        Self(error)
    }
}

impl Deref for WrappedReqwestError {
    type Target = reqwest_middleware::Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for WrappedReqwestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_likely_offline() {
            // Insert an extra hint, we'll show the wrapped error through `source`
            f.write_str("Could not connect, are you offline?")
        } else {
            // Show the wrapped error
            Display::fmt(&self.0, f)
        }
    }
}

impl std::error::Error for WrappedReqwestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if self.is_likely_offline() {
            // `Display` is inserting an extra message, so we need to show the wrapped error
            Some(&self.0)
        } else {
            // `Display` is showing the wrapped error, continue with its source
            self.0.source()
        }
    }
}
