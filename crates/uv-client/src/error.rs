use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::time::Duration;

use async_http_range_reader::AsyncHttpRangeReaderError;
use async_zip::error::ZipError;
use tracing::warn;

use crate::{FlatIndexError, html};
use uv_distribution_filename::{WheelFilename, WheelFilenameError};
use uv_distribution_types::IndexUrl;
use uv_errors::{Hinted, Hints};
use uv_git::GitError;
use uv_http::{CachedClientError, Error as HttpError, ErrorKind as HttpErrorKind};
use uv_normalize::PackageName;
use uv_pypi_types::HashDigest;
use uv_redacted::DisplaySafeUrl;

#[derive(Debug)]
pub struct Error {
    kind: Box<ErrorKind>,
    retries: u32,
    duration: Duration,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.retries > 0 {
            write!(
                f,
                "Request failed after {retries} {subject} in {duration:.1}s",
                retries = self.retries,
                subject = if self.retries > 1 { "retries" } else { "retry" },
                duration = self.duration.as_secs_f32(),
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
    /// Return whether this is an expected user-facing failure.
    pub fn is_user_failure(&self) -> bool {
        match self.kind() {
            ErrorKind::InvalidUrl(_)
            | ErrorKind::MissingWheelGitLfsArtifacts(..)
            | ErrorKind::NonFileUrl(_)
            | ErrorKind::CannotBeABase(_)
            | ErrorKind::Metadata(..)
            | ErrorKind::NoIndex(_)
            | ErrorKind::RemotePackageNotFound(_)
            | ErrorKind::LocalPackageNotFound(_)
            | ErrorKind::LocalIndexNotFound(_)
            | ErrorKind::MetadataHashMismatch { .. }
            | ErrorKind::MetadataParseError(..)
            | ErrorKind::BadJson { .. }
            | ErrorKind::BadHtml { .. }
            | ErrorKind::MetadataRangeRequestsRequired(..)
            | ErrorKind::WheelFilename(_)
            | ErrorKind::NameMismatch { .. }
            | ErrorKind::Zip(..)
            | ErrorKind::MissingContentType(_)
            | ErrorKind::InvalidContentTypeHeader(..)
            | ErrorKind::UnsupportedMediaType(..) => true,
            ErrorKind::Git(error) => error.is_user_failure(),
            ErrorKind::Flat(error) => error.is_user_failure(),
            ErrorKind::Http(error) => match error {
                HttpErrorKind::WrappedReqwestError(_, error) => error.is_user_failure(),
                HttpErrorKind::Offline(_) => true,
                HttpErrorKind::CacheWrite(_)
                | HttpErrorKind::CacheLock(_)
                | HttpErrorKind::Io(_)
                | HttpErrorKind::Decode(_)
                | HttpErrorKind::Encode(_)
                | HttpErrorKind::ArchiveRead(_)
                | HttpErrorKind::ArchiveWrite(_) => false,
            },
            ErrorKind::AsyncHttpRangeReader(..) => false,
        }
    }

    /// Create a new [`Error`] with the given [`ErrorKind`] and number of retries.
    pub fn new(kind: ErrorKind, retries: u32, duration: Duration) -> Self {
        Self {
            kind: Box::new(kind),
            retries,
            duration,
        }
    }

    /// Return the number of retries that were attempted before this error was returned.
    pub fn retries(&self) -> u32 {
        self.retries
    }

    /// Return the time taken for network requests, including retries, backoff and jitter,
    /// before this error was returned.
    pub fn duration(&self) -> Duration {
        self.duration
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

    /// Returns `true` if the error is due to the server not supporting HTTP range requests.
    pub(crate) fn is_http_range_requests_unsupported(
        &self,
        url: &DisplaySafeUrl,
        index: Option<&IndexUrl>,
    ) -> bool {
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

            // The server advertises range request support, but doesn't implement it correctly.
            ErrorKind::AsyncHttpRangeReader(
                _,
                AsyncHttpRangeReaderError::RangeMismatch { .. }
                | AsyncHttpRangeReaderError::ResponseTooShort { .. }
                | AsyncHttpRangeReaderError::ResponseTooLong { .. },
            ) => {
                let url = if let Some(index) = index {
                    index.url()
                } else {
                    url
                };
                warn!(
                    "Invalid range request response from server that declares HTTP range request \
                    support, falling back to streaming: {url}"
                );
                return true;
            }

            // The server returned a "Method Not Allowed" error, indicating it doesn't support
            // HEAD requests, so we can't check for range requests.
            ErrorKind::Http(HttpErrorKind::WrappedReqwestError(_, err))
                if let Some(status) = err.status() =>
            {
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

            // The server doesn't support range requests, but we only discovered this while
            // unzipping due to erroneous server behavior.
            ErrorKind::Zip(_, ZipError::UpstreamReadError(err))
                if let Some(inner) = err.get_ref()
                    && let Some(range_reader_error) =
                        inner.downcast_ref::<AsyncHttpRangeReaderError>() =>
            {
                match range_reader_error {
                    AsyncHttpRangeReaderError::HttpRangeRequestUnsupported
                    | AsyncHttpRangeReaderError::ContentLengthMissing
                    | AsyncHttpRangeReaderError::ContentRangeMissing => {
                        return true;
                    }
                    AsyncHttpRangeReaderError::RangeMismatch { .. }
                    | AsyncHttpRangeReaderError::ResponseTooShort { .. }
                    | AsyncHttpRangeReaderError::ResponseTooLong { .. } => {
                        let url = if let Some(index) = index {
                            index.url()
                        } else {
                            url
                        };
                        warn!(
                            "Invalid range request response from server that declares HTTP \
                            range request support, falling back to streaming: {url}"
                        );
                        return true;
                    }
                    _ => {}
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

impl Hinted for Error {
    fn hints(&self) -> Hints<'_> {
        if let ErrorKind::Http(error) = self.kind() {
            error.hints()
        } else {
            Hints::none()
        }
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind: Box::new(kind),
            retries: 0,
            duration: Duration::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error(transparent)]
    InvalidUrl(#[from] uv_distribution_types::ToUrlError),

    #[error(transparent)]
    Flat(#[from] FlatIndexError),

    #[error(transparent)]
    Git(#[from] uv_git::GitResolverError),

    #[error("The wheel `{0}` is missing Git LFS artifacts.")]
    MissingWheelGitLfsArtifacts(DisplaySafeUrl, #[source] GitError),

    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(DisplaySafeUrl),

    #[error("Expected an index URL, but received non-base URL: {0}")]
    CannotBeABase(DisplaySafeUrl),

    #[error("Failed to read metadata: {0}")]
    Metadata(String, #[source] uv_metadata::Error),

    #[error("{0} isn't available locally, but making network requests to registries was banned")]
    NoIndex(String),

    /// The package was not found in the registry.
    ///
    /// Make sure the package name is spelled correctly and that you've
    /// configured the right registry to fetch it from.
    #[error("Package `{0}` was not found in the registry")]
    RemotePackageNotFound(PackageName),

    /// The package was not found in the local (file-based) index.
    #[error("Package `{0}` was not found in the local index")]
    LocalPackageNotFound(PackageName),

    /// The root was not found in the local (file-based) index.
    #[error("Local index not found at: {}", _0.display())]
    LocalIndexNotFound(PathBuf),

    /// The metadata file does not match a hash provided by its package index.
    #[error(
        "Hash mismatch for package metadata at `{url}`\n\nExpected:\n  {expected}\n\nComputed:\n  {actual}"
    )]
    MetadataHashMismatch {
        url: DisplaySafeUrl,
        expected: HashDigest,
        actual: HashDigest,
    },

    /// The metadata file could not be parsed.
    #[error("Couldn't parse metadata of `{0}` from `{1}`")]
    MetadataParseError(
        WheelFilename,
        String,
        #[source] Box<uv_pypi_types::MetadataError>,
    ),

    #[error(transparent)]
    Http(#[from] HttpErrorKind),

    #[error("Received some unexpected JSON from `{}`", url)]
    BadJson {
        source: serde_json::Error,
        url: DisplaySafeUrl,
    },

    #[error("Received some unexpected HTML from `{}`", url)]
    BadHtml {
        source: html::Error,
        url: DisplaySafeUrl,
    },

    #[error("Failed to read zip with range requests: {0}")]
    AsyncHttpRangeReader(DisplaySafeUrl, #[source] AsyncHttpRangeReaderError),

    #[error("Wheel metadata range requests are required, but not supported for: {0}")]
    MetadataRangeRequestsRequired(DisplaySafeUrl, #[source] Box<Error>),

    #[error("{0} is not a valid wheel filename")]
    WheelFilename(#[source] WheelFilenameError),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("Failed to unzip wheel: {0}")]
    Zip(WheelFilename, #[source] ZipError),

    #[error("Missing `Content-Type` header for `{0}`")]
    MissingContentType(DisplaySafeUrl),

    #[error("Invalid `Content-Type` header for `{0}`")]
    InvalidContentTypeHeader(DisplaySafeUrl, #[source] http::header::ToStrError),

    #[error("Unsupported `Content-Type` \"{1}\" for `{0}`. Expected JSON or HTML.")]
    UnsupportedMediaType(DisplaySafeUrl, String),
}

impl From<HttpErrorKind> for Error {
    fn from(kind: HttpErrorKind) -> Self {
        Self::from(ErrorKind::Http(kind))
    }
}

impl From<HttpError> for Error {
    fn from(error: HttpError) -> Self {
        let retries = error.retries();
        let duration = error.duration();
        Self::new(ErrorKind::from(error.into_kind()), retries, duration)
    }
}

impl<E: Into<Self> + std::error::Error + 'static> From<CachedClientError<E>> for Error {
    /// Attach retry error context, if there were retries.
    fn from(error: CachedClientError<E>) -> Self {
        match error {
            CachedClientError::Client(error) => Self::from(error),
            CachedClientError::Callback {
                retries,
                err,
                duration,
            } => Self::new(err.into().into_kind(), retries, duration),
        }
    }
}
