//! Resumable HTTP reader with transparent retry on failure.
//!
//! This module provides an [`AsyncRead`] implementation that transparently handles
//! transient network failures by resuming downloads via HTTP Range requests.
//!
//! # Usage
//!
//! ```rust,ignore
//! use uv_client::resumable_reader::ResponseExt;
//!
//! let response = client.get(url).send().await?;
//! let reader = response.resumable_stream(client.clone())?;
//!
//! // Use reader with any AsyncRead consumer - failures are handled transparently
//! let decoder = GzipDecoder::new(BufReader::new(reader));
//! ```

use std::error::Error as StdError;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use futures::TryStreamExt;
use reqwest::Response;
use tokio::io::{AsyncRead, ReadBuf};
use tracing::{debug, trace, warn};
use url::Url;

use uv_redacted::DisplaySafeUrl;

use crate::BaseClient;

// ============================================================================
// Extension Trait for Response
// ============================================================================

/// Extension trait to convert a `reqwest::Response` into a resumable stream.
///
/// This provides a convenient way to wrap response bodies with automatic
/// retry-on-failure behavior using HTTP Range requests.
pub trait ResponseExt {
    /// Convert this response into a resumable async reader.
    ///
    /// The returned reader will automatically handle transient network failures
    /// by making new HTTP Range requests to resume from the last successful byte.
    ///
    /// # Requirements
    ///
    /// - The server must support HTTP Range requests (`Accept-Ranges: bytes`)
    /// - The resource should not change during the download
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use uv_client::resumable_reader::ResponseExt;
    ///
    /// let response = client.get(url).send().await?.error_for_status()?;
    /// let reader = response.resumable_stream(client.clone())?;
    ///
    /// // Now use with GzipDecoder, tar reader, etc.
    /// uv_extract::stream::untar_gz(reader, target).await?;
    /// ```
    fn resumable_stream(
        self,
        client: BaseClient,
    ) -> Result<ResumableReader, ResumableError>;

    /// Convert this response into a resumable async reader with custom configuration.
    fn resumable_stream_with_config(
        self,
        client: BaseClient,
        config: ResumableConfig,
    ) -> Result<ResumableReader, ResumableError>;

    /// Check if this response supports resumable downloads.
    ///
    /// Returns `true` if the server sent `Accept-Ranges: bytes` header.
    fn supports_range_requests(&self) -> bool;
}

impl ResponseExt for Response {
    fn resumable_stream(
        self,
        client: BaseClient,
    ) -> Result<ResumableReader, ResumableError> {
        self.resumable_stream_with_config(client, ResumableConfig::default())
    }

    fn resumable_stream_with_config(
        self,
        client: BaseClient,
        config: ResumableConfig,
    ) -> Result<ResumableReader, ResumableError> {
        let url = self.url().clone();
        ResumableReader::new(client, url, self, config)
    }

    fn supports_range_requests(&self) -> bool {
        // Log all headers for debugging
        debug!(
            "Response headers for {}: {:?}",
            self.url(),
            self.headers()
                .iter()
                .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or("<binary>")))
                .collect::<Vec<_>>()
        );

        let accept_ranges = self
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok());
        let supports = accept_ranges == Some("bytes");
        debug!(
            "Accept-Ranges header: {:?}, supports_range_requests: {}",
            accept_ranges,
            supports
        );
        supports
    }
}

/// Configuration for resumable downloads.
#[derive(Debug, Clone)]
pub struct ResumableConfig {
    /// Maximum number of reconnection attempts per failure.
    pub max_reconnects: u32,
    /// Delay between reconnection attempts.
    pub reconnect_delay: Duration,
    /// Whether to verify the server supports Range requests.
    pub require_range_support: bool,
}

impl Default for ResumableConfig {
    fn default() -> Self {
        Self {
            max_reconnects: 10,
            reconnect_delay: Duration::from_millis(500),
            require_range_support: true,
        }
    }
}

/// Error returned when resumable download fails permanently.
#[derive(Debug, thiserror::Error)]
pub enum ResumableError {
    #[error("Server does not support Range requests")]
    RangeNotSupported,
    #[error("Max reconnection attempts ({0}) exceeded")]
    MaxReconnectsExceeded(u32),
    #[error("Content-Length changed during download (expected {expected}, got {actual})")]
    ContentLengthMismatch { expected: u64, actual: u64 },
    #[error("Request failed: {0}")]
    Request(#[from] reqwest_middleware::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Internal state machine for managing reconnection.
enum ReaderState {
    /// Actively reading from the current stream.
    Reading {
        stream: Pin<Box<dyn AsyncRead + Send>>,
    },
    /// Reconnecting after a failure.
    Reconnecting {
        future: Pin<Box<dyn Future<Output = Result<Response, reqwest_middleware::Error>> + Send>>,
        attempt: u32,
    },
    /// Waiting before retry.
    Backoff {
        sleep: Pin<Box<tokio::time::Sleep>>,
        attempt: u32,
    },
    /// Terminal error state.
    Failed(Option<ResumableError>),
    /// Successfully completed.
    Done,
}

/// An `AsyncRead` implementation that transparently resumes on transient failures.
///
/// When a network error occurs during reading, this reader will:
/// 1. Track the byte position that was successfully delivered
/// 2. Make a new HTTP request with `Range: bytes=N-` header
/// 3. Continue delivering bytes as if nothing happened
///
/// The downstream consumer (e.g., GzipDecoder) sees a seamless byte stream.
pub struct ResumableReader {
    /// HTTP client for making requests.
    client: BaseClient,
    /// URL being downloaded.
    url: Url,
    /// Expected content length (from initial response).
    content_length: Option<u64>,
    /// Bytes successfully delivered to downstream consumer.
    bytes_delivered: Arc<AtomicU64>,
    /// Configuration.
    config: ResumableConfig,
    /// Current state.
    state: ReaderState,
}

impl ResumableReader {
    /// Create a new resumable reader from an initial HTTP response.
    ///
    /// The response should be from a successful GET request. This reader will
    /// use Range requests to resume if the connection fails.
    pub fn new(
        client: BaseClient,
        url: Url,
        initial_response: Response,
        config: ResumableConfig,
    ) -> Result<Self, ResumableError> {
        // Check if server supports Range requests
        if config.require_range_support {
            let accept_ranges = initial_response
                .headers()
                .get(reqwest::header::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok());

            if accept_ranges != Some("bytes") {
                debug!(
                    "Server does not support Range requests (Accept-Ranges: {:?}), falling back to non-resumable",
                    accept_ranges
                );
                return Err(ResumableError::RangeNotSupported);
            }
        }

        let content_length = initial_response.content_length();
        debug!(
            "Created ResumableReader for {} (content_length: {:?}, max_reconnects: {})",
            DisplaySafeUrl::from(url.clone()),
            content_length,
            config.max_reconnects
        );
        let stream = response_to_async_read(initial_response);

        Ok(Self {
            client,
            url,
            content_length,
            bytes_delivered: Arc::new(AtomicU64::new(0)),
            config,
            state: ReaderState::Reading { stream },
        })
    }

    /// Get the number of bytes successfully delivered so far.
    pub fn bytes_delivered(&self) -> u64 {
        self.bytes_delivered.load(Ordering::SeqCst)
    }

    /// Check if an error is transient and should trigger a reconnection.
    fn is_transient_error(err: &io::Error) -> bool {
        matches!(
            err.kind(),
            io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::UnexpectedEof
                | io::ErrorKind::TimedOut
        ) || err.to_string().contains("unexpected BufError")
    }

    /// Start a reconnection attempt.
    fn start_reconnect(&mut self, attempt: u32) {
        let position = self.bytes_delivered.load(Ordering::SeqCst);
        let client = self.client.clone();
        let url = self.url.clone();
        let range_header = format!("bytes={}-", position);

        debug!(
            "Attempting to resume download from byte {} for {} (attempt {}/{})",
            position,
            DisplaySafeUrl::from(url.clone()),
            attempt,
            self.config.max_reconnects
        );
        trace!(
            "Sending Range request: {} for {}",
            range_header,
            DisplaySafeUrl::from(url.clone())
        );

        let future = Box::pin(async move {
            let response = client
                .for_host(&DisplaySafeUrl::from(url.clone()))
                .get(url)
                .header(reqwest::header::RANGE, range_header)
                .send()
                .await?;
            // Convert middleware response to reqwest response
            Ok(response)
        });

        self.state = ReaderState::Reconnecting { future, attempt };
    }

    /// Handle a successful reconnection response.
    fn handle_reconnect_response(
        &mut self,
        response: Response,
    ) -> Result<(), ResumableError> {
        trace!(
            "Reconnect response: status={}, content-range={:?}",
            response.status(),
            response.headers().get(reqwest::header::CONTENT_RANGE)
        );

        // Verify we got a 206 Partial Content response
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            // Server might not support Range, or the resource changed
            trace!("Expected 206 Partial Content, got {}", response.status());
            return Err(ResumableError::RangeNotSupported);
        }

        // Verify Content-Length is consistent (if available)
        if let (Some(expected), Some(actual_range)) = (
            self.content_length,
            response.headers().get(reqwest::header::CONTENT_RANGE),
        ) {
            // Content-Range format: "bytes start-end/total"
            if let Some(total) = actual_range
                .to_str()
                .ok()
                .and_then(|s| s.split('/').last())
                .and_then(|s| s.parse::<u64>().ok())
            {
                if total != expected {
                    return Err(ResumableError::ContentLengthMismatch {
                        expected,
                        actual: total,
                    });
                }
            }
        }

        let stream = response_to_async_read(response);
        self.state = ReaderState::Reading { stream };
        Ok(())
    }
}

impl AsyncRead for ResumableReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            match &mut self.state {
                ReaderState::Reading { stream } => {
                    let before = buf.filled().len();
                    
                    match stream.as_mut().poll_read(cx, buf) {
                        Poll::Ready(Ok(())) => {
                            let bytes_read = buf.filled().len() - before;
                            if bytes_read > 0 {
                                self.bytes_delivered
                                    .fetch_add(bytes_read as u64, Ordering::SeqCst);
                            } else {
                                // EOF - check if we got all expected bytes
                                if let Some(expected) = self.content_length {
                                    let delivered = self.bytes_delivered.load(Ordering::SeqCst);
                                    if delivered < expected {
                                        // Premature EOF - treat as transient
                                        if self.config.max_reconnects > 0 {
                                            self.state = ReaderState::Backoff {
                                                sleep: Box::pin(tokio::time::sleep(
                                                    self.config.reconnect_delay,
                                                )),
                                                attempt: 1,
                                            };
                                            continue;
                                        }
                                    }
                                }
                                self.state = ReaderState::Done;
                            }
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Ready(Err(err)) if Self::is_transient_error(&err) => {
                            // Transient error - try to reconnect
                            let position = self.bytes_delivered.load(Ordering::SeqCst);
                            trace!(
                                "Transient error at byte {}: {} (kind: {:?})",
                                position,
                                err,
                                err.kind()
                            );
                            if self.config.max_reconnects > 0 {
                                self.state = ReaderState::Backoff {
                                    sleep: Box::pin(tokio::time::sleep(
                                        self.config.reconnect_delay,
                                    )),
                                    attempt: 1,
                                };
                                continue;
                            }
                            return Poll::Ready(Err(err));
                        }
                        Poll::Ready(Err(err)) => {
                            // Non-transient error - propagate
                            trace!(
                                "Non-transient error, not retrying: {} (kind: {:?})",
                                err,
                                err.kind()
                            );
                            self.state = ReaderState::Failed(None);
                            return Poll::Ready(Err(err));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }

                ReaderState::Backoff { sleep, attempt } => {
                    match sleep.as_mut().poll(cx) {
                        Poll::Ready(()) => {
                            let attempt = *attempt;
                            self.start_reconnect(attempt);
                            continue;
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }

                ReaderState::Reconnecting { future, attempt } => {
                    match future.as_mut().poll(cx) {
                        Poll::Ready(Ok(response)) => {
                            match self.handle_reconnect_response(response) {
                                Ok(()) => {
                                    debug!("Successfully resumed download");
                                    continue;
                                }
                                Err(err) => {
                                    warn!("Reconnection failed: {}", err);
                                    self.state = ReaderState::Failed(Some(err));
                                    return Poll::Ready(Err(io::Error::new(
                                        io::ErrorKind::Other,
                                        "Reconnection failed",
                                    )));
                                }
                            }
                        }
                        Poll::Ready(Err(err)) => {
                            let attempt = *attempt;
                            trace!("Reconnection attempt {} failed: {}", attempt, err);
                            if attempt < self.config.max_reconnects {
                                // Retry with exponential backoff
                                let delay = self.config.reconnect_delay * (1 << attempt.min(5));
                                debug!(
                                    "Reconnection failed, retrying in {:?} (attempt {}/{})",
                                    delay,
                                    attempt + 1,
                                    self.config.max_reconnects
                                );
                                self.state = ReaderState::Backoff {
                                    sleep: Box::pin(tokio::time::sleep(delay)),
                                    attempt: attempt + 1,
                                };
                                continue;
                            }
                            warn!(
                                "Max reconnection attempts ({}) exceeded",
                                self.config.max_reconnects
                            );
                            self.state = ReaderState::Failed(Some(
                                ResumableError::MaxReconnectsExceeded(self.config.max_reconnects),
                            ));
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::Other,
                                "Max reconnects exceeded",
                            )));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }

                ReaderState::Failed(err) => {
                    let msg = err
                        .take()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Download failed".to_string());
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, msg)));
                }

                ReaderState::Done => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

/// Convert a reqwest Response into an AsyncRead.
fn response_to_async_read(response: Response) -> Pin<Box<dyn AsyncRead + Send>> {
    let stream = response
        .bytes_stream()
        .map_err(reqwest_error_to_io_error);

    Box::pin(tokio_util::io::StreamReader::new(stream))
}

/// Convert a reqwest error to an io::Error, preserving error kind for transient errors.
fn reqwest_error_to_io_error(err: reqwest::Error) -> io::Error {
    // Check for timeout
    if err.is_timeout() {
        return io::Error::new(io::ErrorKind::TimedOut, err);
    }

    // Check for connection errors by inspecting the source chain
    let mut source: Option<&(dyn StdError + 'static)> = err.source();
    while let Some(s) = source {
        if let Some(io_err) = s.downcast_ref::<io::Error>() {
            // Preserve the original io::Error kind
            return io::Error::new(io_err.kind(), err);
        }
        // Check error message for hyper incomplete message indicators
        let msg = s.to_string();
        if msg.contains("connection closed")
            || msg.contains("incomplete message")
            || msg.contains("connection reset")
        {
            return io::Error::new(io::ErrorKind::ConnectionReset, err);
        }
        source = s.source();
    }

    // Check if it's a connect error (usually network issues)
    if err.is_connect() {
        return io::Error::new(io::ErrorKind::ConnectionRefused, err);
    }

    // Default to Other
    io::Error::new(io::ErrorKind::Other, err)
}

// ============================================================================
// Integration Examples
// ============================================================================

/// # Integration with uv-python downloads
///
/// ```rust,ignore
/// // In downloads.rs - replace read_url() implementation:
///
/// use uv_client::resumable_reader::{ResponseExt, ResumableConfig};
///
/// async fn read_url(
///     url: &DisplaySafeUrl,
///     client: &BaseClient,
/// ) -> Result<(impl AsyncRead + Unpin, Option<u64>), Error> {
///     if url.scheme() == "file" {
///         // ... file handling unchanged ...
///     } else {
///         let response = client
///             .for_host(url)
///             .get(Url::from(url.clone()))
///             .send()
///             .await
///             .map_err(|err| Error::from_reqwest_middleware(url.clone(), err))?
///             .error_for_status()
///             .map_err(|err| Error::from_reqwest(url.clone(), err, None))?;
///
///         let size = response.content_length();
///
///         // Use resumable stream if server supports it, otherwise fall back
///         let reader: Box<dyn AsyncRead + Unpin + Send> = 
///             if response.supports_range_requests() {
///                 Box::new(response.resumable_stream(client.clone())?)
///             } else {
///                 // Fall back to non-resumable stream
///                 let stream = response
///                     .bytes_stream()
///                     .map_err(io::Error::other)
///                     .into_async_read();
///                 Box::new(stream.compat())
///             };
///
///         Ok((reader, size))
///     }
/// }
/// ```
///
/// # Integration with distribution downloads
///
/// ```rust,ignore
/// // In source/mod.rs or distribution_database.rs:
///
/// let response = client.get(url).send().await?;
///
/// // For large archives, use resumable streaming
/// let reader = if response.content_length().unwrap_or(0) > 10_000_000 {
///     // >10MB: use resumable reader
///     response.resumable_stream(client.clone())?
/// } else {
///     // Small files: use simple streaming
///     response.bytes_stream().map_err(io::Error::other).into_async_read()
/// };
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transient_error_connection_reset() {
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::ConnectionReset,
            "connection reset"
        )));
    }

    #[test]
    fn test_is_transient_error_connection_aborted() {
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "connection aborted"
        )));
    }

    #[test]
    fn test_is_transient_error_broken_pipe() {
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::BrokenPipe,
            "broken pipe"
        )));
    }

    #[test]
    fn test_is_transient_error_unexpected_eof() {
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "unexpected eof"
        )));
    }

    #[test]
    fn test_is_transient_error_timed_out() {
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::TimedOut,
            "timed out"
        )));
    }

    #[test]
    fn test_is_transient_error_buf_error() {
        // This is the specific error from flate2/async_compression that we want to catch
        assert!(ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::Other,
            "unexpected BufError"
        )));
    }

    #[test]
    fn test_is_not_transient_error_not_found() {
        assert!(!ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::NotFound,
            "not found"
        )));
    }

    #[test]
    fn test_is_not_transient_error_permission_denied() {
        assert!(!ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::PermissionDenied,
            "permission denied"
        )));
    }

    #[test]
    fn test_is_not_transient_error_other() {
        // A generic "Other" error without the BufError message should not be transient
        assert!(!ResumableReader::is_transient_error(&io::Error::new(
            io::ErrorKind::Other,
            "some other error"
        )));
    }

    #[test]
    fn test_default_config() {
        let config = ResumableConfig::default();
        assert_eq!(config.max_reconnects, 10);
        assert_eq!(config.reconnect_delay, Duration::from_millis(500));
        assert!(config.require_range_support);
    }

    #[test]
    fn test_custom_config() {
        let config = ResumableConfig {
            max_reconnects: 5,
            reconnect_delay: Duration::from_secs(1),
            require_range_support: false,
        };
        assert_eq!(config.max_reconnects, 5);
        assert_eq!(config.reconnect_delay, Duration::from_secs(1));
        assert!(!config.require_range_support);
    }
}
