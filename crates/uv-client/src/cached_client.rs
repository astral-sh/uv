use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crc32fast::Hasher;
use futures::FutureExt;
use reqwest::{Request, Response};
use rkyv::util::AlignedVec;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{Instrument, Span, debug, info_span, instrument, trace, warn};
use zerocopy::byteorder::little_endian::{U32, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use uv_cache::{CacheEntry, Freshness};
use uv_fastid::Id;
use uv_fs::write_atomic;
use uv_redacted::DisplaySafeUrl;

use crate::base_client::CertificateSource;
use crate::httpcache::{AfterResponse, BeforeRequest, CachePolicy, CachePolicyBuilder};
use crate::{BaseClient, Error, ErrorKind, OwnedArchive, ProblemDetails, RetryState};

/// A trait the generalizes (de)serialization at a high level.
///
/// The main purpose of this trait is to make the `CachedClient` work for
/// either serde or other mechanisms of serialization such as `rkyv`.
///
/// If you're using Serde, then unless you want to control the format, callers
/// should just use [`CachedClient::get_serde_with_retry`]. This will use a default
/// implementation of `Cacheable` internally.
///
/// Alternatively, callers using `rkyv` should use
/// [`CachedClient::get_cacheable_with_retry`]. If your types fit into the
/// `rkyvutil::OwnedArchive` mold, then an implementation of `Cacheable` is
/// already provided for that type.
pub(crate) trait Cacheable: Sized {
    /// This associated type permits customizing what the "output" type of
    /// deserialization is. It can be identical to `Self`.
    ///
    /// Typical use of this is for wrapper types used to provide blanket trait
    /// impls without hitting overlapping impl problems.
    type Target: Send + 'static;

    /// Deserialize a value from bytes aligned to a 16-byte boundary.
    fn from_aligned_bytes(bytes: AlignedVec) -> Result<Self::Target, Error>;
    /// Serialize bytes to a possibly owned byte buffer.
    fn to_bytes(&self) -> Result<Cow<'_, [u8]>, Error>;
    /// Convert this type into its final form.
    fn into_target(self) -> Self::Target;
}

/// A wrapper type that makes anything with Serde support automatically
/// implement `Cacheable`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct SerdeCacheable<T> {
    inner: T,
}

impl<T: Serialize + DeserializeOwned + Send + 'static> Cacheable for SerdeCacheable<T> {
    type Target = T;

    fn from_aligned_bytes(bytes: AlignedVec) -> Result<T, Error> {
        Ok(rmp_serde::from_slice::<T>(&bytes).map_err(ErrorKind::Decode)?)
    }

    fn to_bytes(&self) -> Result<Cow<'_, [u8]>, Error> {
        Ok(Cow::from(
            rmp_serde::to_vec(&self.inner).map_err(ErrorKind::Encode)?,
        ))
    }

    fn into_target(self) -> Self::Target {
        self.inner
    }
}

/// All `OwnedArchive` values are cacheable.
impl<A> Cacheable for OwnedArchive<A>
where
    A: rkyv::Archive + for<'a> rkyv::Serialize<crate::rkyvutil::Serializer<'a>> + Send + 'static,
    A::Archived: rkyv::Portable
        + rkyv::Deserialize<A, crate::rkyvutil::Deserializer>
        + for<'a> rkyv::bytecheck::CheckBytes<crate::rkyvutil::Validator<'a>>,
{
    type Target = Self;

    fn from_aligned_bytes(bytes: AlignedVec) -> Result<Self, Error> {
        Self::new(bytes)
    }

    fn to_bytes(&self) -> Result<Cow<'_, [u8]>, Error> {
        Ok(Cow::from(Self::as_bytes(self)))
    }

    fn into_target(self) -> Self::Target {
        self
    }
}

/// Dispatch type: Either a cached client error or a (user specified) error from the callback.
#[derive(Debug)]
pub enum CachedClientError<CallbackError: std::error::Error + 'static> {
    /// The client tracks retries internally.
    Client(Error),
    /// The callback error, with retry context attached by the outer request loop.
    Callback {
        retries: u32,
        err: CallbackError,
        duration: Duration,
    },
}

impl<CallbackError: std::error::Error + 'static> CachedClientError<CallbackError> {
    /// Attach the combined number of retries to the error context, discarding the previous count.
    fn with_retries(self, retries: u32) -> Self {
        match self {
            Self::Client(err) => Self::Client(err.with_retries(retries)),
            Self::Callback {
                retries: _,
                err,
                duration,
            } => Self::Callback {
                retries,
                err,
                duration,
            },
        }
    }

    fn retries(&self) -> u32 {
        match self {
            Self::Client(err) => err.retries(),
            Self::Callback { retries, .. } => *retries,
        }
    }

    fn error(&self) -> &(dyn std::error::Error + 'static) {
        match self {
            Self::Client(err) => err,
            Self::Callback { err, .. } => err,
        }
    }
}

impl<CallbackError: std::error::Error + 'static> From<Error> for CachedClientError<CallbackError> {
    fn from(error: Error) -> Self {
        Self::Client(error)
    }
}

impl<CallbackError: std::error::Error + 'static> From<ErrorKind>
    for CachedClientError<CallbackError>
{
    fn from(error: ErrorKind) -> Self {
        Self::Client(error.into())
    }
}

impl<E: Into<Self> + std::error::Error + 'static> From<CachedClientError<E>> for Error {
    /// Attach retry error context, if there were retries.
    fn from(error: CachedClientError<E>) -> Self {
        match error {
            CachedClientError::Client(error) => error,
            CachedClientError::Callback {
                retries,
                err,
                duration,
            } => Self::new(err.into().into_kind(), retries, duration),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheControl {
    /// Respect the `cache-control` header from the response.
    None,
    /// Apply `max-age=0, must-revalidate` to the request.
    MustRevalidate,
    /// Allow the client to return stale responses.
    AllowStale,
    /// Override the cache control header with a custom value.
    Override(http::HeaderValue),
}

impl From<Freshness> for CacheControl {
    fn from(value: Freshness) -> Self {
        match value {
            Freshness::Fresh => Self::None,
            Freshness::Stale => Self::MustRevalidate,
            Freshness::Missing => Self::None,
        }
    }
}

/// Custom caching layer over [`reqwest::Client`].
///
/// The implementation takes inspiration from the `http-cache` crate, but adds support for running
/// an async callback on the response before caching. We use this to e.g. store a
/// parsed version of the wheel metadata and for our remote zip reader. In the latter case, we want
/// to read a single file from a remote zip using range requests (so we don't have to download the
/// entire file). We send a HEAD request in the caching layer to check if the remote file has
/// changed (and if range requests are supported), and in the callback we make the actual range
/// requests if required.
///
/// Unlike `http-cache`, all outputs must be serializable/deserializable in some way, by
/// implementing the `Cacheable` trait.
///
/// Again unlike `http-cache`, the caller gets full control over the cache key with the assumption
/// that it's a file.
#[derive(Debug, Clone)]
pub struct CachedClient(BaseClient);

impl CachedClient {
    pub fn new(client: BaseClient) -> Self {
        Self(client)
    }

    /// The underlying [`BaseClient`] without caching.
    pub fn uncached(&self) -> &BaseClient {
        &self.0
    }

    pub(crate) fn certificate_source(&self) -> CertificateSource {
        self.0.certificate_source()
    }

    /// Make a cached request with a custom response transformation while using
    /// the `Cacheable` trait to (de)serialize cached responses.
    ///
    /// The purpose of this routine is the use of `Cacheable`. Namely, it
    /// generalizes over (de)serialization such that mechanisms other than
    /// serde (such as rkyv) can be used to manage (de)serialization of cached
    /// data.
    ///
    /// If a new response was received (no prior cached response or modified
    /// on the remote), the response is passed through `response_callback` and
    /// only the result is cached and returned. The `response_callback` is
    /// allowed to make subsequent requests, e.g. through the uncached client.
    #[instrument(skip_all)]
    async fn get_cacheable<
        Payload: Cacheable + 'static,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFnOnce(Response) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>> {
        let start = Instant::now();

        if matches!(cache_control, CacheControl::AllowStale) {
            let (req, cached) = self
                .read_and_decode_stale_cache::<Payload>(req, cache_entry)
                .await;
            match cached {
                Ok(Some(payload)) => return Ok(payload),
                Ok(None) => warn!(
                    "Cached response doesn't match current request for: {}",
                    DisplaySafeUrl::from_url(req.url().clone())
                ),
                Err(err) if err.is_file_not_exists() => {
                    trace!(
                        "No cache entry exists for `{}`",
                        cache_entry.path().display()
                    );
                }
                Err(err) => {
                    warn!(
                        "Broken cache entry at `{}`, removing: {err}",
                        cache_entry.path().display()
                    );
                    let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
                }
            }

            let (response, cache_policy) = self.fresh_request(req, cache_control).await?;
            return self
                .run_response_callback(
                    cache_entry,
                    cache_policy,
                    start,
                    response,
                    response_callback,
                )
                .await;
        }

        let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
        let (req, cached) = self
            .read_cache::<Payload>(req, cache_entry, cache_control.clone())
            .await;
        let cached_response = match cached {
            Some(CachedEntry::Fresh(payload)) => {
                return match payload {
                    Ok(payload) => Ok(payload),
                    Err(err) => {
                        warn!(
                            "Broken fresh cache entry (for payload) at `{}`, removing: {err}",
                            cache_entry.path().display()
                        );
                        self.resend_and_heal_cache(
                            fresh_req,
                            cache_entry,
                            cache_control,
                            response_callback,
                        )
                        .await
                    }
                };
            }
            Some(CachedEntry::Stale {
                cached,
                new_cache_policy_builder,
            }) => {
                self.send_cached_handle_stale(
                    req,
                    cache_control.clone(),
                    cached,
                    *new_cache_policy_builder,
                )
                .boxed_local()
                .await?
            }
            None => {
                debug!(
                    "No cache entry for: {}",
                    DisplaySafeUrl::from_url(req.url().clone())
                );
                let (response, cache_policy) =
                    self.fresh_request(req, cache_control.clone()).await?;
                CachedResponse::ModifiedOrNew {
                    response,
                    cache_policy,
                }
            }
        };
        match cached_response {
            CachedResponse::NotModified { cached, new_policy } => {
                let refresh_cache =
                    info_span!("refresh_cache", file = %cache_entry.path().display());
                async {
                    let path = cache_entry.path().to_path_buf();
                    let span = Span::current();
                    let payload = tokio::task::spawn_blocking(move || {
                        span.in_scope(|| {
                            cached.refresh_policy(&path, &new_policy)?;
                            // Keep decoding errors separate so a corrupt payload can be refetched.
                            Ok::<_, Error>(Payload::from_aligned_bytes(cached.data))
                        })
                    })
                    .await
                    .expect("cache refresh task panicked")?;
                    match payload {
                        Ok(payload) => Ok(payload),
                        Err(err) => {
                            warn!(
                                "Broken fresh cache entry after revalidation \
                                 (for payload) at `{}`, removing: {err}",
                                cache_entry.path().display()
                            );
                            self.resend_and_heal_cache(
                                fresh_req,
                                cache_entry,
                                cache_control.clone(),
                                response_callback,
                            )
                            .await
                        }
                    }
                }
                .instrument(refresh_cache)
                .await
            }
            CachedResponse::ModifiedOrNew {
                response,
                cache_policy,
            } => {
                // If we got a modified response, but it's a 304, then a validator failed (e.g., the
                // ETag didn't match). We need to make a fresh request.
                if response.status() == http::StatusCode::NOT_MODIFIED {
                    warn!(
                        "Server returned unusable 304 for: {}",
                        DisplaySafeUrl::from_url(fresh_req.url().clone())
                    );
                    self.resend_and_heal_cache(
                        fresh_req,
                        cache_entry,
                        cache_control,
                        response_callback,
                    )
                    .await
                } else {
                    self.run_response_callback(
                        cache_entry,
                        cache_policy,
                        start,
                        response,
                        response_callback,
                    )
                    .await
                }
            }
        }
    }

    /// Make a request without checking whether the cache is fresh.
    async fn skip_cache<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFnOnce(Response) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>> {
        let start = Instant::now();
        let (response, cache_policy) = self.fresh_request(req, cache_control).await?;

        let payload = self
            .run_response_callback(cache_entry, cache_policy, start, response, async |resp| {
                let payload = response_callback(resp).await?;
                Ok(SerdeCacheable { inner: payload })
            })
            .await?;

        Ok(payload)
    }

    async fn resend_and_heal_cache<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFnOnce(Response) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>> {
        let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
        let start = Instant::now();
        let (response, cache_policy) = self.fresh_request(req, cache_control).await?;
        self.run_response_callback(
            cache_entry,
            cache_policy,
            start,
            response,
            response_callback,
        )
        .await
    }

    async fn run_response_callback<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFnOnce(Response) -> Result<Payload, CallBackError>,
    >(
        &self,
        cache_entry: &CacheEntry,
        cache_policy: Option<Box<CachePolicy>>,
        start: Instant,
        response: Response,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>> {
        let new_cache = info_span!("new_cache", file = %cache_entry.path().display());
        let data = response_callback(response)
            .boxed_local()
            .await
            .map_err(|err| CachedClientError::Callback {
                // These retries are already counted in RetryState, so don't count them again.
                // The outer loop fills in the total before returning the error to the caller.
                retries: 0,
                err,
                duration: start.elapsed(),
            })?;
        let Some(cache_policy) = cache_policy else {
            return Ok(data.into_target());
        };
        async {
            fs_err::tokio::create_dir_all(cache_entry.dir())
                .await
                .map_err(ErrorKind::CacheWrite)?;
            let data_with_cache_policy_bytes =
                DataWithCachePolicy::serialize(&cache_policy, &data.to_bytes()?)?;
            write_atomic(cache_entry.path(), data_with_cache_policy_bytes)
                .await
                .map_err(ErrorKind::CacheWrite)?;
            Ok(data.into_target())
        }
        .instrument(new_cache)
        .await
    }

    /// Reads the cache policy and decodes a fresh payload in one blocking task.
    ///
    /// Stale payloads remain encoded until revalidation confirms they can be reused.
    #[instrument(name = "read_and_parse_cache", skip_all, fields(file = %cache_entry.path().display()))]
    async fn read_cache<Payload: Cacheable + 'static>(
        &self,
        mut req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
    ) -> (Request, Option<CachedEntry<Payload::Target>>) {
        let path = cache_entry.path().to_path_buf();
        let span = Span::current();
        let (req, cached) = self
            .0
            .cache_read_runtime()
            .spawn_blocking(move || {
                span.in_scope(|| {
                    let cached = DataWithCachePolicy::from_path_sync(&path).map(|cached| {
                        // Apply the cache control header only when checking an existing entry.
                        if let CacheControl::MustRevalidate = cache_control {
                            req.headers_mut().insert(
                                http::header::CACHE_CONTROL,
                                http::HeaderValue::from_static("no-cache"),
                            );
                        }
                        let url = DisplaySafeUrl::from_url(req.url().clone());
                        match cached.cache_policy.before_request(&mut req) {
                            BeforeRequest::Fresh => {
                                debug!("Found fresh response for: {url}");
                                Some(CachedEntry::Fresh(Payload::from_aligned_bytes(cached.data)))
                            }
                            BeforeRequest::Stale(new_cache_policy_builder) => {
                                debug!("Found stale response for: {url}");
                                Some(CachedEntry::Stale {
                                    cached,
                                    new_cache_policy_builder: Box::new(new_cache_policy_builder),
                                })
                            }
                            BeforeRequest::NoMatch => {
                                warn!("Cached response doesn't match current request for: {url}");
                                None
                            }
                        }
                    });
                    (req, cached)
                })
            })
            .await
            .expect("cache read and payload decoding task panicked");
        let cached = match cached {
            Ok(cached) => cached,
            Err(err) => {
                // When we know the cache entry doesn't exist, then things are
                // normal and we shouldn't emit a WARN.
                if err.is_file_not_exists() {
                    trace!(
                        "No cache entry exists for `{}`",
                        cache_entry.path().display()
                    );
                } else {
                    warn!(
                        "Broken cache policy entry at `{}`, removing: {err}",
                        cache_entry.path().display()
                    );
                    let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
                }
                None
            }
        };
        (req, cached)
    }

    /// Reads and decodes an allowed-stale cache entry in one blocking task.
    ///
    /// The task returns the request it owns while checking the policy. `Ok(None)` means the entry
    /// belongs to a different request; errors indicate a broken entry for the caller to remove.
    #[instrument(name = "read_and_decode_stale_cache", skip_all, fields(file = %cache_entry.path().display()))]
    async fn read_and_decode_stale_cache<Payload: Cacheable + 'static>(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
    ) -> (Request, Result<Option<Payload::Target>, Error>) {
        let path = cache_entry.path().to_path_buf();
        self.0
            .cache_read_runtime()
            .spawn_blocking(move || {
                let cached = DataWithCachePolicy::from_path_sync(&path).and_then(|cached| {
                    if cached.cache_policy.matches_stale_request(&req) {
                        Payload::from_aligned_bytes(cached.data).map(Some)
                    } else {
                        Ok(None)
                    }
                });
                (req, cached)
            })
            .await
            .expect("cache read and payload decoding task panicked")
    }

    async fn send_cached_handle_stale(
        &self,
        req: Request,
        cache_control: CacheControl,
        cached: DataWithCachePolicy,
        new_cache_policy_builder: CachePolicyBuilder,
    ) -> Result<CachedResponse, Error> {
        let url = DisplaySafeUrl::from_url(req.url().clone());
        debug!("Sending revalidation request for: {url}");
        let start = Instant::now();
        let mut response = self
            .0
            .execute(req)
            .instrument(info_span!("revalidation_request", url = %url))
            .await
            .map_err(|err| {
                Error::from_reqwest_middleware(url.clone(), err, start, self.certificate_source())
            })?;
        trace!(
            "Received response for revalidation request with status {} for: {}",
            response.status(),
            url
        );

        // Check for HTTP error status and extract problem details if available
        let retry_count = response
            .extensions()
            .get::<reqwest_retry::RetryCount>()
            .map(|retries| retries.value());

        if let Err(status_error) = response.error_for_status_ref() {
            let problem_details = ProblemDetails::try_from_response(response).await;
            return Err(Error::new(
                ErrorKind::from_reqwest_with_problem_details(url, status_error, problem_details),
                retry_count.unwrap_or_default(),
                start.elapsed(),
            ));
        }

        // If the user set a custom `Cache-Control` header, override it.
        if let CacheControl::Override(header) = &cache_control {
            response
                .headers_mut()
                .insert(http::header::CACHE_CONTROL, header.clone());
        }

        match cached
            .cache_policy
            .after_response(new_cache_policy_builder, &response)
        {
            AfterResponse::NotModified(new_policy) => {
                debug!("Found not-modified response for: {url}");
                Ok(CachedResponse::NotModified {
                    cached,
                    new_policy: Box::new(new_policy),
                })
            }
            AfterResponse::Modified(new_policy) => {
                debug!("Found modified response for: {url}");
                Ok(CachedResponse::ModifiedOrNew {
                    response,
                    cache_policy: new_policy
                        .to_archived()
                        .is_storable()
                        .then(|| Box::new(new_policy)),
                })
            }
        }
    }

    #[instrument(skip_all, fields(url = %DisplaySafeUrl::from_url(req.url().clone())))]
    async fn fresh_request(
        &self,
        req: Request,
        cache_control: CacheControl,
    ) -> Result<(Response, Option<Box<CachePolicy>>), Error> {
        let url = DisplaySafeUrl::from_url(req.url().clone());
        debug!("Sending fresh {} request for: {}", req.method(), url);
        let cache_policy_builder = CachePolicyBuilder::new(&req);
        let start = Instant::now();
        let mut response = self.0.execute(req).await.map_err(|err| {
            Error::from_reqwest_middleware(url.clone(), err, start, self.certificate_source())
        })?;
        trace!(
            "Received response for fresh request with status {} for: {}",
            response.status(),
            url
        );

        // If the user set a custom `Cache-Control` header, override it.
        if let CacheControl::Override(header) = &cache_control {
            response
                .headers_mut()
                .insert(http::header::CACHE_CONTROL, header.clone());
        }

        let retry_count = response
            .extensions()
            .get::<reqwest_retry::RetryCount>()
            .map(|retries| retries.value());

        if let Err(status_error) = response.error_for_status_ref() {
            let problem_details = ProblemDetails::try_from_response(response).await;
            return Err(Error::new(
                ErrorKind::from_reqwest_with_problem_details(url, status_error, problem_details),
                retry_count.unwrap_or_default(),
                start.elapsed(),
            ));
        }

        let cache_policy = cache_policy_builder.build(&response);
        let cache_policy = if cache_policy.to_archived().is_storable() {
            Some(Box::new(cache_policy))
        } else {
            None
        };
        Ok((response, cache_policy))
    }

    /// Perform a [`CachedClient::get_serde`] request with a default retry strategy.
    ///
    /// The callback shares the request's [`RetryState`]. It must use that state for any retries
    /// it performs and send subsequent requests through [`RetryState::send`].
    #[instrument(skip_all)]
    pub async fn get_serde_with_retry<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFn(Response, &mut RetryState) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>> {
        let payload = self
            .get_cacheable_with_retry(
                req,
                cache_entry,
                cache_control,
                async |resp, retry_state| {
                    let payload = response_callback(resp, retry_state).await?;
                    Ok(SerdeCacheable { inner: payload })
                },
            )
            .await?;
        Ok(payload)
    }

    /// Perform a [`CachedClient::get_cacheable`] request with a default retry strategy.
    ///
    /// See: <https://github.com/TrueLayer/reqwest-middleware/blob/8a494c165734e24c62823714843e1c9347027e8a/reqwest-retry/src/middleware.rs#L137>
    #[instrument(skip_all)]
    pub(crate) async fn get_cacheable_with_retry<
        Payload: Cacheable + 'static,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFn(Response, &mut RetryState) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>> {
        let mut retry_state = RetryState::start(self.uncached().retry_policy(), req.url().clone());
        loop {
            let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
            let result = self
                .get_cacheable(
                    fresh_req,
                    cache_entry,
                    cache_control.clone(),
                    async |response| {
                        retry_state
                            .handle_response(response, &response_callback)
                            .await
                    },
                )
                .await;

            match result {
                Ok(ok) => return Ok(ok),
                Err(err)
                    if let Some(backoff) = retry_state.should_retry(err.error(), err.retries()) =>
                {
                    retry_state.sleep_backoff(backoff).await;
                }
                Err(err) => return Err(err.with_retries(retry_state.total_retries())),
            }
        }
    }

    /// Perform a [`CachedClient::skip_cache`] request with a default retry strategy.
    ///
    /// The callback shares the request's [`RetryState`], as in [`Self::get_serde_with_retry`].
    ///
    /// See: <https://github.com/TrueLayer/reqwest-middleware/blob/8a494c165734e24c62823714843e1c9347027e8a/reqwest-retry/src/middleware.rs#L137>
    pub async fn skip_cache_with_retry<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback: AsyncFn(Response, &mut RetryState) -> Result<Payload, CallBackError>,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>> {
        let mut retry_state = RetryState::start(self.uncached().retry_policy(), req.url().clone());
        loop {
            let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
            let result = self
                .skip_cache(
                    fresh_req,
                    cache_entry,
                    cache_control.clone(),
                    async |response| {
                        retry_state
                            .handle_response(response, &response_callback)
                            .await
                    },
                )
                .await;

            match result {
                Ok(ok) => return Ok(ok),
                Err(err)
                    if let Some(backoff) = retry_state.should_retry(err.error(), err.retries()) =>
                {
                    retry_state.sleep_backoff(backoff).await;
                }
                Err(err) => return Err(err.with_retries(retry_state.total_retries())),
            }
        }
    }
}

/// A cache entry checked against the current request.
#[derive(Debug)]
enum CachedEntry<Payload> {
    /// The payload was decoded without an HTTP request (e.g. age < max-age).
    Fresh(Result<Payload, Error>),
    /// The payload may be reused if the server confirms it is unmodified.
    Stale {
        cached: DataWithCachePolicy,
        new_cache_policy_builder: Box<CachePolicyBuilder>,
    },
}

#[derive(Debug)]
enum CachedResponse {
    /// The cached response is fresh after an HTTP request (e.g. 304 not modified)
    NotModified {
        /// The cached response (with its old cache policy).
        cached: DataWithCachePolicy,
        /// The new [`CachePolicy`] is used to determine if the response
        /// is fresh or stale when making subsequent requests for the same
        /// resource. This policy should overwrite the old policy associated
        /// with the cached response. In particular, this new policy is derived
        /// from data received in a revalidation response, which might change
        /// the parameters of cache behavior.
        ///
        /// Box the policy to reduce the enum's stack size.
        new_policy: Box<CachePolicy>,
    },
    /// There was no prior cached response or the cache was outdated
    ///
    /// The cache policy is `None` if it isn't storable
    ModifiedOrNew {
        /// The response received from the server.
        response: Response,
        /// The [`CachePolicy`] is used to determine if the response is fresh or
        /// stale when making subsequent requests for the same resource.
        ///
        /// Box the policy to reduce the enum's stack size.
        cache_policy: Option<Box<CachePolicy>>,
    },
}

/// Cached data with an HTTP policy that can be refreshed in place.
///
/// # Format
///
/// Each file contains the payload, a random 16-byte entry ID, the archived
/// HTTP cache policy, a 4-byte CRC32 checksum, and the payload length as a little-endian
/// `u64`. The checksum covers the entry ID, policy, and encoded payload length.
/// The payload remains first so an [`AlignedVec`] can be truncated without moving it.
///
/// New responses atomically replace the entire file with a new entry ID. HTTP 304
/// responses overwrite only the policy, checksum, and length. A writer opens the file
/// and verifies its entry ID before updating it, so a delayed 304 cannot mutate a
/// newer response. If a replacement occurs after opening, the writer updates only the
/// old file through its open handle.
///
/// Readers validate the checksum before using the policy to detect interrupted or
/// overlapping policy writes. An invalid entry requires a full fetch, so it cannot be
/// used offline. The checksum detects accidental corruption, not malicious changes.
/// No additional file or reader lock is needed.
#[derive(Debug)]
pub struct DataWithCachePolicy {
    pub data: AlignedVec,
    cache_policy: OwnedArchive<CachePolicy>,
    entry_id: [u8; 16],
}

/// The fixed-size footer following the archived HTTP cache policy.
///
/// Byte-aligned, little-endian fields allow the footer to follow a policy of any length.
#[derive(Debug, FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned)]
#[repr(C)]
struct CachePolicyFooter {
    checksum: U32,
    data_len: U64,
}

impl DataWithCachePolicy {
    /// Loads cached data and its associated HTTP cache policy from the given
    /// file path in a synchronous fashion.
    ///
    /// # Errors
    ///
    /// If the given byte buffer is not in a valid format or if reading the
    /// file given fails, then this returns an error.
    #[instrument]
    fn from_path_sync(path: &Path) -> Result<Self, Error> {
        let mut file = fs_err::File::open(path).map_err(ErrorKind::Io)?;
        let file_size = file.metadata().map_err(ErrorKind::Io)?.len();
        let file_size = usize::try_from(file_size)
            .ok()
            .filter(|&file_size| file_size <= AlignedVec::<16>::MAX_CAPACITY)
            .ok_or_else(|| {
                ErrorKind::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "cache entry file size of {file_size} bytes exceeds the maximum supported \
                         size of {} bytes",
                        AlignedVec::<16>::MAX_CAPACITY,
                    ),
                ))
            })?;

        let mut aligned_bytes = AlignedVec::with_capacity(file_size);
        aligned_bytes.resize(file_size, 0);
        file.read_exact(&mut aligned_bytes).map_err(ErrorKind::Io)?;
        Self::from_aligned_bytes(aligned_bytes)
    }

    /// Loads cached data and its associated HTTP cache policy from the given
    /// reader.
    ///
    /// # Errors
    ///
    /// If the given byte buffer is not in a valid format or if the reader
    /// fails, then this returns an error.
    pub fn from_reader(mut rdr: impl std::io::Read) -> Result<Self, Error> {
        let mut aligned_bytes = AlignedVec::new();
        aligned_bytes
            .extend_from_reader(&mut rdr)
            .map_err(ErrorKind::Io)?;
        Self::from_aligned_bytes(aligned_bytes)
    }

    /// Validate the policy and separate it from the aligned payload.
    fn from_aligned_bytes(mut bytes: AlignedVec) -> Result<Self, Error> {
        let (contents, footer) = CachePolicyFooter::ref_from_suffix(&bytes).map_err(|_| {
            ErrorKind::ArchiveRead("HTTP cache entry is shorter than its trailer".to_owned())
        })?;
        let data_len = usize::try_from(footer.data_len.get())
            .map_err(|_| ErrorKind::ArchiveRead("invalid HTTP cache payload length".to_owned()))?;
        let (_, tail) = contents.split_at_checked(data_len).ok_or_else(|| {
            ErrorKind::ArchiveRead("invalid HTTP cache payload length".to_owned())
        })?;
        let (entry_id, policy_bytes) = <[u8; 16]>::ref_from_prefix(tail)
            .map_err(|_| ErrorKind::ArchiveRead("invalid HTTP cache payload length".to_owned()))?;

        let mut hasher = Hasher::new();
        hasher.update(tail);
        hasher.update(footer.data_len.as_bytes());
        if hasher.finalize() != footer.checksum.get() {
            return Err(
                ErrorKind::ArchiveRead("HTTP cache policy checksum mismatch".to_owned()).into(),
            );
        }

        let entry_id = *entry_id;
        let mut policy = AlignedVec::with_capacity(policy_bytes.len());
        policy.extend_from_slice(policy_bytes);
        let cache_policy = OwnedArchive::new(policy)?;

        bytes.resize(data_len, 0);
        Ok(Self {
            data: bytes,
            cache_policy,
            entry_id,
        })
    }

    /// Serialize a new response with a unique entry ID for atomic publication.
    fn serialize(policy: &CachePolicy, data: &[u8]) -> Result<Vec<u8>, Error> {
        let entry_id = *Id::secure()
            .as_bytes()
            .as_array::<16>()
            .expect("IDs have 16 bytes");
        let tail = Self::serialize_policy(policy, &entry_id, data.len())?;
        let mut bytes = Vec::with_capacity(data.len() + entry_id.len() + tail.len());
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&entry_id);
        bytes.extend_from_slice(&tail);
        Ok(bytes)
    }

    /// Serialize the mutable tail, binding the policy to its entry ID and payload boundary.
    fn serialize_policy(
        policy: &CachePolicy,
        entry_id: &[u8; 16],
        data_len: usize,
    ) -> Result<Vec<u8>, Error> {
        let policy = OwnedArchive::from_unarchived(policy)?;
        let policy = OwnedArchive::as_bytes(&policy);
        let data_len = U64::new(
            u64::try_from(data_len).map_err(|err| ErrorKind::ArchiveWrite(err.to_string()))?,
        );

        let mut hasher = Hasher::new();
        hasher.update(entry_id);
        hasher.update(policy);
        hasher.update(data_len.as_bytes());
        let footer = CachePolicyFooter {
            checksum: U32::new(hasher.finalize()),
            data_len,
        };

        let mut bytes = Vec::with_capacity(policy.len() + size_of::<CachePolicyFooter>());
        bytes.extend_from_slice(policy);
        bytes.extend_from_slice(footer.as_bytes());
        Ok(bytes)
    }

    /// Refresh only the policy tail of the file whose payload was revalidated.
    fn refresh_policy(&self, path: &Path, policy: &CachePolicy) -> Result<(), Error> {
        let mut file = match fs_err::OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err)
                if err.kind() == std::io::ErrorKind::PermissionDenied
                    || err.kind() == std::io::ErrorKind::ReadOnlyFilesystem =>
            {
                // The server has validated the payload even if its new policy cannot be saved.
                debug!("Skipping cache policy update at {}: {err}", path.display());
                return Ok(());
            }
            Err(err) => return Err(ErrorKind::CacheWrite(err).into()),
        };
        file.seek(SeekFrom::Start(self.data.len() as u64))
            .map_err(ErrorKind::CacheWrite)?;
        let mut entry_id = [0; 16];
        match file.read_exact(&mut entry_id) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(ErrorKind::CacheWrite(err).into()),
        }
        if entry_id != self.entry_id {
            // A newer response replaced the entry while this request was in flight.
            return Ok(());
        }
        let tail = Self::serialize_policy(policy, &entry_id, self.data.len())?;
        file.write_all(&tail).map_err(ErrorKind::CacheWrite)?;
        file.set_len((self.data.len() + entry_id.len() + tail.len()) as u64)
            .map_err(ErrorKind::CacheWrite)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{CachePolicy, CachePolicyBuilder, DataWithCachePolicy, OwnedArchive};

    /// Build policies with different serialized sizes without depending on wall-clock changes.
    fn policy(vary: &str) -> Result<CachePolicy> {
        let request = reqwest::Request::new(http::Method::GET, "https://example.com/".parse()?);
        let response = reqwest::Response::from(
            http::Response::builder()
                .header("cache-control", "public, max-age=3600")
                .header("vary", vary)
                .body(Vec::new())?,
        );
        Ok(CachePolicyBuilder::new(&request).build(&response))
    }

    #[test]
    fn policy_can_grow_and_shrink_in_place() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("response");
        let small = policy("x-small")?;
        let large = policy(&format!("x-{}", "large".repeat(1000)))?;
        let payload = vec![42; 1024 * 1024];
        let original = DataWithCachePolicy::serialize(&small, &payload)?;
        fs_err::write(&path, &original)?;
        let cached = DataWithCachePolicy::from_path_sync(&path)?;

        cached.refresh_policy(&path, &large)?;
        assert!(fs_err::metadata(&path)?.len() > original.len() as u64);
        let grown = DataWithCachePolicy::from_path_sync(&path)?;
        assert_eq!(grown.data.as_slice(), payload);
        assert_eq!(grown.entry_id, cached.entry_id);
        assert_eq!(
            OwnedArchive::as_bytes(&grown.cache_policy),
            OwnedArchive::as_bytes(&large.to_archived()),
        );

        grown.refresh_policy(&path, &small)?;
        assert_eq!(fs_err::read(&path)?, original);
        Ok(())
    }

    #[test]
    fn interrupted_policy_writes_never_accept_a_mixed_policy() -> Result<()> {
        let small = policy("x-small")?;
        let large = policy(&format!("x-{}", "large".repeat(20)))?;
        for (before, after) in [(&small, &large), (&large, &small)] {
            let original = DataWithCachePolicy::serialize(before, b"payload")?;
            let cached = DataWithCachePolicy::from_reader(original.as_slice())?;
            let tail =
                DataWithCachePolicy::serialize_policy(after, &cached.entry_id, cached.data.len())?;
            let offset = cached.data.len() + cached.entry_id.len();
            let before = before.to_archived();
            let after = after.to_archived();
            // Model every interruption point in write_all, before set_len truncates a shorter policy.
            for written in 0..=tail.len() {
                let mut interrupted = original.clone();
                interrupted.resize(interrupted.len().max(offset + written), 0);
                interrupted[offset..offset + written].copy_from_slice(&tail[..written]);
                if let Ok(read) = DataWithCachePolicy::from_reader(interrupted.as_slice()) {
                    assert_eq!(read.data.as_slice(), b"payload");
                    let policy = OwnedArchive::as_bytes(&read.cache_policy);
                    assert!(
                        policy == OwnedArchive::as_bytes(&before)
                            || policy == OwnedArchive::as_bytes(&after)
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn cache_trailer_corruption_is_rejected() -> Result<()> {
        let bytes = DataWithCachePolicy::serialize(&policy("x-small")?, b"payload")?;
        // The checksum covers the entry ID, policy, and length; corrupt each byte in turn.
        for index in b"payload".len()..bytes.len() {
            let mut corrupted = bytes.clone();
            corrupted[index] ^= 1;
            assert!(DataWithCachePolicy::from_reader(corrupted.as_slice()).is_err());
        }
        Ok(())
    }
}
