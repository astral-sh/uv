use std::error::Error;
use std::time::{Duration, SystemTime, SystemTimeError};
use std::{io, iter};

use http::status::StatusCode;
use http::{Extensions, Method};
use itertools::Itertools;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    RetryPolicy, RetryTransientMiddleware, Retryable, RetryableStrategy, default_on_request_error,
    default_on_request_success,
};
use rustls::{AlertDescription, Error as RustlsError};
use tracing::{debug, trace};

use uv_redacted::SafeUrl;

use crate::{RequestBuilder, WrappedReqwestError};

/// Passes the request method to the retry strategy.
pub(crate) struct UvRetryMiddleware(pub(crate) ExponentialBackoff);

#[async_trait::async_trait]
impl Middleware for UvRetryMiddleware {
    async fn handle(
        &self,
        request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        // RetryableStrategy::handle receives only the result, so carry this request's method
        // into the strategy that classifies its errors.
        let strategy = UvRetryableStrategy(request.method().clone());
        RetryTransientMiddleware::new_with_policy_and_strategy(self.0, strategy)
            .handle(request, extensions, next)
            .await
    }
}

/// An extension over [`DefaultRetryableStrategy`] that logs transient request failures and
/// adds additional retry cases.
struct UvRetryableStrategy(Method);

impl RetryableStrategy for UvRetryableStrategy {
    fn handle(&self, res: &Result<Response, reqwest_middleware::Error>) -> Option<Retryable> {
        let retryable = match res {
            Ok(success) => default_on_request_success(success),
            Err(err) => retryable_on_request_failure_inner(err, Some(&self.0)),
        };

        // Log on transient errors
        if retryable == Some(Retryable::Transient) {
            match res {
                Ok(response) => {
                    debug!(
                        "Transient request failure for: {}",
                        SafeUrl::ref_cast(response.url())
                    );
                }
                Err(err) => {
                    let url = request_error_url(err);
                    let redact = |message: &str| {
                        url.map_or_else(|| message.to_owned(), |url| url.redact_in(message))
                    };
                    let context = iter::successors(err.source(), |&err| err.source())
                        .map(|err| format!("  Caused by: {}", redact(&err.to_string())))
                        .join("\n");
                    let error = redact(&err.to_string());
                    debug!(
                        "Transient request failure for `{}`, retrying: {error}\n{context}",
                        url.map_or_else(|| "unknown URL".to_owned(), ToString::to_string)
                    );
                }
            }
        }
        retryable
    }
}

/// Per-request retry state and policy.
pub struct RetryState {
    retry_policy: ExponentialBackoff,
    start_time: SystemTime,
    total_retries: u32,
    url: SafeUrl,
}

impl RetryState {
    /// Initialize the [`RetryState`] and record the start time for the retry policy.
    pub fn start(retry_policy: ExponentialBackoff, url: impl Into<SafeUrl>) -> Self {
        Self {
            retry_policy,
            start_time: SystemTime::now(),
            total_retries: 0,
            url: url.into(),
        }
    }

    /// The number of retries across all requests.
    ///
    /// Includes retries reported by the HTTP middleware.
    pub(crate) fn total_retries(&self) -> u32 {
        self.total_retries
    }

    /// The total duration from the first request to the (failure) of the last request.
    pub(crate) fn duration(&self) -> Result<Duration, SystemTimeError> {
        self.start_time.elapsed()
    }

    /// Send a request and count any retries performed by the middleware.
    pub async fn send(
        &mut self,
        request: RequestBuilder<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let result = request.send().await;
        self.record_request_retries(result.as_ref());
        result
    }

    /// Count middleware retries before invoking the callback with the updated [`RetryState`].
    pub(crate) async fn handle_response<Payload, CallbackError, Callback>(
        &mut self,
        response: Response,
        callback: Callback,
    ) -> Result<Payload, CallbackError>
    where
        Callback: AsyncFnOnce(Response, &mut Self) -> Result<Payload, CallbackError>,
    {
        self.record_request_retries(Ok(&response));
        callback(response, self).await
    }

    /// Account for retries performed by the middleware before handling a response or error.
    ///
    /// Call once per request, including successful requests whose bodies may later fail.
    fn record_request_retries(&mut self, result: Result<&Response, &reqwest_middleware::Error>) {
        let retries = match result {
            Ok(response) => response
                .extensions()
                .get::<reqwest_retry::RetryCount>()
                .map_or(0, |retries| retries.value()),
            Err(reqwest_middleware::Error::Middleware(err)) => {
                match err.downcast_ref::<reqwest_retry::RetryError>() {
                    Some(reqwest_retry::RetryError::WithRetries { retries, .. }) => *retries,
                    Some(reqwest_retry::RetryError::Error(_)) | None => 0,
                }
            }
            Err(reqwest_middleware::Error::Reqwest(_)) => 0,
        };
        self.total_retries += retries;
    }

    /// Determines whether request should be retried.
    ///
    /// Takes the number of retries from nested layers associated with the specific `err` type as
    /// `error_retries`.
    ///
    /// Returns the backoff duration if the request should be retried.
    #[must_use]
    pub fn should_retry(
        &mut self,
        err: &(dyn Error + 'static),
        error_retries: u32,
    ) -> Option<Duration> {
        // If the middleware performed any retries, consider them in our budget.
        self.total_retries += error_retries;
        match retryable_on_request_failure(err) {
            Some(Retryable::Transient) => {
                // Capture `now` before calling the policy so that `execute_after`
                // (computed from a `SystemTime::now()` inside the library) is always
                // >= `now`, making `duration_since` reliable.
                let now = SystemTime::now();
                let retry_decision = self
                    .retry_policy
                    .should_retry(self.start_time, self.total_retries);
                if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                    let duration = execute_after
                        .duration_since(now)
                        .unwrap_or_else(|_| Duration::default());

                    self.total_retries += 1;
                    return Some(duration);
                }

                None
            }
            Some(Retryable::Fatal) | None => None,
        }
    }

    /// Wait before retrying the request.
    pub async fn sleep_backoff(&self, duration: Duration) {
        debug!(
            "Transient failure while handling response from `{}`; retrying after {:.1}s...",
            self.url,
            duration.as_secs_f32(),
        );
        // TODO(konsti): Should we show a spinner plus a message in the CLI while
        // waiting?
        tokio::time::sleep(duration).await;
    }
}

/// Whether the error looks like a network error that should be retried.
///
/// This is an extension over [`reqwest_middleware::default_on_request_failure`], which is missing
/// a number of cases:
/// * Inside the reqwest or reqwest-middleware error is an `io::Error` such as a broken pipe
/// * When streaming a response, a reqwest error may be hidden several layers behind errors
///   of different crates processing the stream, including `io::Error` layers
/// * Any `h2` error
pub fn retryable_on_request_failure(err: &(dyn Error + 'static)) -> Option<Retryable> {
    retryable_on_request_failure_inner(err, None)
}

fn retryable_on_request_failure_inner(
    err: &(dyn Error + 'static),
    method: Option<&Method>,
) -> Option<Retryable> {
    // First, try to show a nice trace log
    if let Some((Some(status), Some(url))) = find_source::<WrappedReqwestError>(&err)
        .map(|request_err| (request_err.status(), request_err.url()))
    {
        trace!(
            "Considering retry of response HTTP {status} for `{url}`",
            url = SafeUrl::from_url(url.clone())
        );
    } else if let Some(url) = request_error_url(err) {
        trace!(
            "Considering retry of error: {}",
            url.redact_in(&format!("{err:?}"))
        );
    } else {
        trace!("Considering retry of error: {err:?}");
    }

    let mut has_known_error = false;
    // IO Errors or reqwest errors may be nested through custom IO errors or stream processing
    // crates
    let mut current_source = Some(err);
    while let Some(source) = current_source {
        // Handle different kinds of reqwest error nesting not accessible by downcast.
        let reqwest_err = if let Some(reqwest_err) = source.downcast_ref::<reqwest::Error>() {
            Some(reqwest_err)
        } else if let Some(reqwest_err) = source
            .downcast_ref::<WrappedReqwestError>()
            .and_then(|err| err.inner())
        {
            Some(reqwest_err)
        } else if let Some(reqwest_middleware::Error::Reqwest(reqwest_err)) =
            source.downcast_ref::<reqwest_middleware::Error>()
        {
            Some(reqwest_err)
        } else {
            None
        };

        if let Some(reqwest_err) = reqwest_err {
            has_known_error = true;
            if is_tls_certificate_error(reqwest_err) {
                trace!("Fatal nested reqwest TLS certificate error");
                return Some(Retryable::Fatal);
            }
            // Ignore the default retry strategy returning fatal.
            if default_on_request_error(reqwest_err) == Some(Retryable::Transient) {
                trace!("Transient nested reqwest error");
                return Some(Retryable::Transient);
            }
            if is_retryable_status_error(reqwest_err) {
                trace!("Transient nested reqwest status code error");
                return Some(Retryable::Transient);
            }

            // Hyper's dropped dispatch task is a request error with no public discriminator.
            // It may have sent the request, so only replay it if the HTTP method is safe.
            if reqwest_err.is_request() && method.is_some_and(Method::is_safe) {
                trace!("Transient unclassified reqwest request error for safe method");
                return Some(Retryable::Transient);
            }

            trace!("Fatal nested reqwest error");
        } else if source.downcast_ref::<h2::Error>().is_some() {
            // All h2 errors look like errors that should be retried
            // https://github.com/astral-sh/uv/issues/15916
            trace!("Transient nested h2 error");
            return Some(Retryable::Transient);
        } else if let Some(io_err) = source.downcast_ref::<io::Error>() {
            has_known_error = true;
            let retryable_io_err_kinds = [
                // https://github.com/astral-sh/uv/issues/12054
                io::ErrorKind::BrokenPipe,
                // From reqwest-middleware
                io::ErrorKind::ConnectionAborted,
                // https://github.com/astral-sh/uv/issues/3514
                io::ErrorKind::ConnectionReset,
                // https://github.com/astral-sh/uv/issues/14699
                io::ErrorKind::InvalidData,
                // https://github.com/astral-sh/uv/issues/17697#issuecomment-3817060484
                io::ErrorKind::TimedOut,
                // https://github.com/astral-sh/uv/issues/9246
                io::ErrorKind::UnexpectedEof,
            ];
            if retryable_io_err_kinds.contains(&io_err.kind()) {
                trace!("Transient IO error: `{}`", io_err.kind());
                return Some(Retryable::Transient);
            }

            trace!(
                "Fatal IO error `{}`, not a transient IO error kind",
                io_err.kind()
            );
        }

        current_source = source.source();
    }

    if !has_known_error {
        trace!("Cannot retry error: neither an IO error nor a reqwest error");
    }

    None
}

/// An error type that supports URL-fallback and exponential-backoff retry logic.
///
/// Used by [`fetch_with_url_fallback`] to drive the retry loop without knowing the concrete error
/// type.
pub trait RetriableError: std::error::Error + Sized + 'static {
    /// Returns `true` if an alternative URL should be tried immediately (without backoff).
    fn should_try_next_url(&self) -> bool;

    /// Returns the number of inner retries already recorded in this error.
    fn retries(&self) -> u32;

    /// Wrap the error to indicate that the operation was retried `retries` times before failing.
    #[must_use]
    fn into_retried(self, retries: u32, duration: Duration) -> Self;
}

/// Whether the error is a status code error that is retryable.
///
/// Port of `reqwest_retry::default_on_request_success`.
fn is_retryable_status_error(reqwest_err: &reqwest::Error) -> bool {
    let Some(status) = reqwest_err.status() else {
        return false;
    };
    status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
}

fn is_tls_certificate_error(reqwest_err: &reqwest::Error) -> bool {
    let Some(rustls_error) = find_source::<RustlsError>(reqwest_err) else {
        return false;
    };

    // TODO(konsti): https://github.com/seanmonstar/reqwest/issues/2819#issuecomment-5032072023
    match rustls_error {
        RustlsError::InvalidCertificate(_) | RustlsError::NoCertificatesPresented => true,
        RustlsError::AlertReceived(alert) => matches!(
            alert,
            AlertDescription::AccessDenied
                | AlertDescription::BadCertificate
                | AlertDescription::BadCertificateHashValue
                | AlertDescription::BadCertificateStatusResponse
                | AlertDescription::CertificateExpired
                | AlertDescription::CertificateRequired
                | AlertDescription::CertificateRevoked
                | AlertDescription::CertificateUnknown
                | AlertDescription::CertificateUnobtainable
                | AlertDescription::DecryptError
                | AlertDescription::NoCertificate
                | AlertDescription::UnknownCA
                | AlertDescription::UnsupportedCertificate
        ),
        _ => false,
    }
}

/// Finds the request URL for diagnostics, including transparent middleware and retry wrappers.
fn request_error_url<'a>(err: &'a (dyn Error + 'static)) -> Option<&'a SafeUrl> {
    iter::successors(Some(err), |&err| {
        if let Some(io_error) = err.downcast_ref::<io::Error>()
            && let Some(inner) = io_error.get_ref()
        {
            Some(inner as &(dyn Error + 'static))
        } else {
            err.source()
        }
    })
    .find_map(|err| {
        err.downcast_ref::<reqwest::Error>()
            .and_then(reqwest::Error::url)
            .or_else(|| {
                err.downcast_ref::<reqwest_middleware::Error>()
                    .and_then(reqwest_middleware::Error::url)
            })
            .or_else(|| {
                err.downcast_ref::<WrappedReqwestError>()
                    .and_then(|err| err.url())
            })
    })
    .map(SafeUrl::ref_cast)
}

/// Find the first source error of a specific type, including errors wrapped by [`io::Error`].
///
/// Inspired by <https://github.com/seanmonstar/reqwest/issues/1602#issuecomment-1220996681>
/// See <https://github.com/hyperium/h2/issues/862>
fn find_source<E: Error + 'static>(orig: &dyn Error) -> Option<&E> {
    let mut cause = orig.source();
    while let Some(err) = cause {
        if let Some(concrete_err) = err.downcast_ref() {
            return Some(concrete_err);
        }
        if let Some(io_err) = err.downcast_ref::<io::Error>()
            && let Some(inner_err) = io_err.get_ref()
        {
            if let Some(concrete_err) = inner_err.downcast_ref() {
                return Some(concrete_err);
            }
            cause = Some(inner_err);
            continue;
        }
        cause = err.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use insta::assert_debug_snapshot;
    use reqwest::Client;
    use reqwest_middleware::ClientWithMiddleware;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tracing_test::traced_test;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::retryable_on_request_failure;

    #[tokio::test]
    async fn unclassified_request_errors_respect_method() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url: SafeUrl = format!("http://{}", listener.local_addr()?).parse()?;
        let server = async {
            let (stream, _) = listener.accept().await?;
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            while stream.read_line(&mut request).await? != 0 {
                if request.ends_with("\r\n\r\n") {
                    break;
                }
            }
            stream
                .get_mut()
                .write_all(b"invalid HTTP response\r\n\r\n")
                .await
        };
        let client = Client::new();
        let (served, response) = tokio::join!(server, client.get(url.as_str()).send());
        served?;
        let error = response.expect_err("invalid response must fail");
        assert!(error.is_request());
        assert!(retryable_on_request_failure(&error).is_none());
        let result = Err(reqwest_middleware::Error::Reqwest(error));
        assert!(UvRetryableStrategy(Method::POST).handle(&result).is_none());
        assert!(UvRetryableStrategy(Method::HEAD).handle(&result) == Some(Retryable::Transient));
        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn retry_logs_redact_signed_urls() -> Result<()> {
        let server = MockServer::start().await;
        Mock::given(path("/wheel"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let response = Client::new()
            .get(format!(
                "{}/wheel?sig=azure-secret&X-Amz-Signature=aws-secret",
                server.uri()
            ))
            .send()
            .await?;
        assert!(matches!(
            UvRetryableStrategy(Method::GET).handle(&Ok(response)),
            Some(Retryable::Transient)
        ));

        let response = Client::new()
            .get(format!(
                "{}/wheel?sig=azure-secret&X-Amz-Signature=aws-secret",
                server.uri()
            ))
            .send()
            .await?;
        let err = response
            .error_for_status()
            .expect_err("expected a 503 response");
        assert!(matches!(
            UvRetryableStrategy(Method::GET).handle(&Err(err.into())),
            Some(Retryable::Transient)
        ));

        let response = Client::new()
            .get(format!(
                "{}/wheel?sig=azure-secret&X-Amz-Signature=aws-secret",
                server.uri()
            ))
            .send()
            .await?;
        let err = response
            .error_for_status()
            .expect_err("expected a 503 response");
        let err = reqwest_middleware::Error::middleware(reqwest_retry::RetryError::WithRetries {
            retries: 1,
            err: err.into(),
        });
        assert!(matches!(
            UvRetryableStrategy(Method::GET).handle(&Err(err)),
            Some(Retryable::Transient)
        ));

        logs_assert(|lines| {
            let logs = lines.join("\n");
            assert!(logs.contains("Transient request failure"));
            assert!(logs.contains("Considering retry"));
            assert!(logs.contains("sig=****&X-Amz-Signature=****"));
            assert!(!logs.contains("azure-secret"));
            assert!(!logs.contains("aws-secret"));
            Ok(())
        });
        Ok(())
    }

    /// Enumerate which status codes we are retrying.
    #[tokio::test]
    async fn retried_status_codes() -> Result<()> {
        let server = MockServer::start().await;
        let client = Client::default();
        let middleware_client = ClientWithMiddleware::default();
        let mut retried = Vec::new();
        for status in 100..599 {
            // Test all standard status codes and an example for a non-RFC code used in the wild.
            if StatusCode::from_u16(status)?.canonical_reason().is_none() && status != 420 {
                continue;
            }

            Mock::given(path(format!("/{status}")))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let response = middleware_client
                .get(format!("{}/{}", server.uri(), status))
                .send()
                .await;

            let middleware_retry =
                UvRetryableStrategy(Method::GET).handle(&response) == Some(Retryable::Transient);

            let response = client
                .get(format!("{}/{}", server.uri(), status))
                .send()
                .await?;

            let uv_retry = match response.error_for_status() {
                Ok(_) => false,
                Err(err) => retryable_on_request_failure(&err) == Some(Retryable::Transient),
            };

            // Ensure we're retrying the same status code as the reqwest_retry crate. We may choose
            // to deviate from this later.
            assert_eq!(middleware_retry, uv_retry);
            if uv_retry {
                retried.push(status);
            }
        }

        assert_debug_snapshot!(retried, @"
        [
            100,
            102,
            103,
            408,
            429,
            500,
            501,
            502,
            503,
            504,
            505,
            506,
            507,
            508,
            510,
            511,
        ]
        ");

        Ok(())
    }
}
