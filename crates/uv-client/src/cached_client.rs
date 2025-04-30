use std::fmt::{Debug, Display, Formatter};
use std::time::{Duration, SystemTime};
use std::{borrow::Cow, future::Future, path::Path};

use futures::FutureExt;
use reqwest::{Request, Response};
use reqwest_retry::RetryPolicy;
use rkyv::util::AlignedVec;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{debug, info_span, instrument, trace, warn, Instrument};

use uv_cache::{CacheEntry, Freshness};
use uv_fs::write_atomic;

use crate::base_client::is_extended_transient_error;
use crate::BaseClient;
use crate::{
    httpcache::{AfterResponse, BeforeRequest, CachePolicy, CachePolicyBuilder},
    rkyvutil::OwnedArchive,
    Error, ErrorKind,
};

/// A trait the generalizes (de)serialization at a high level.
///
/// The main purpose of this trait is to make the `CachedClient` work for
/// either serde or other mechanisms of serialization such as `rkyv`.
///
/// If you're using Serde, then unless you want to control the format, callers
/// should just use `CachedClient::get_serde`. This will use a default
/// implementation of `Cacheable` internally.
///
/// Alternatively, callers using `rkyv` should use
/// `CachedClient::get_cacheable`. If your types fit into the
/// `rkyvutil::OwnedArchive` mold, then an implementation of `Cacheable` is
/// already provided for that type.
pub trait Cacheable: Sized {
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
pub(crate) struct SerdeCacheable<T> {
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

/// Either a cached client error or a (user specified) error from the callback
pub enum CachedClientError<CallbackError: std::error::Error + 'static> {
    Client(Error),
    Callback(CallbackError),
}

impl<CallbackError: std::error::Error + 'static> Display for CachedClientError<CallbackError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CachedClientError::Client(err) => write!(f, "{err}"),
            CachedClientError::Callback(err) => write!(f, "{err}"),
        }
    }
}

impl<CallbackError: std::error::Error + 'static> Debug for CachedClientError<CallbackError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CachedClientError::Client(err) => write!(f, "{err:?}"),
            CachedClientError::Callback(err) => write!(f, "{err:?}"),
        }
    }
}

impl<CallbackError: std::error::Error + 'static> std::error::Error
    for CachedClientError<CallbackError>
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CachedClientError::Client(err) => Some(err),
            CachedClientError::Callback(err) => Some(err),
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
    fn from(error: CachedClientError<E>) -> Self {
        match error {
            CachedClientError::Client(error) => error,
            CachedClientError::Callback(error) => error.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CacheControl {
    /// Respect the `cache-control` header from the response.
    None,
    /// Apply `max-age=0, must-revalidate` to the request.
    MustRevalidate,
    /// Allow the client to return stale responses.
    AllowStale,
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

    /// Make a cached request with a custom response transformation
    /// while using serde to (de)serialize cached responses.
    ///
    /// If a new response was received (no prior cached response or modified
    /// on the remote), the response is passed through `response_callback` and
    /// only the result is cached and returned. The `response_callback` is
    /// allowed to make subsequent requests, e.g. through the uncached client.
    #[instrument(skip_all)]
    pub async fn get_serde<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>>
    where
        Callback: Fn(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let payload = self
            .get_cacheable(req, cache_entry, cache_control, |resp| async {
                let payload = response_callback(resp).await?;
                Ok(SerdeCacheable { inner: payload })
            })
            .await?;
        Ok(payload)
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
    pub async fn get_cacheable<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>>
    where
        Callback: Fn(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
        let cached_response = if let Some(cached) = Self::read_cache(cache_entry).await {
            self.send_cached(req, cache_control, cached)
                .boxed_local()
                .await?
        } else {
            debug!("No cache entry for: {}", req.url());
            let (response, cache_policy) = self.fresh_request(req).await?;
            CachedResponse::ModifiedOrNew {
                response,
                cache_policy,
            }
        };
        match cached_response {
            CachedResponse::FreshCache(cached) => match Payload::from_aligned_bytes(cached.data) {
                Ok(payload) => Ok(payload),
                Err(err) => {
                    warn!(
                        "Broken fresh cache entry (for payload) at {}, removing: {err}",
                        cache_entry.path().display()
                    );
                    self.resend_and_heal_cache(fresh_req, cache_entry, response_callback)
                        .await
                }
            },
            CachedResponse::NotModified { cached, new_policy } => {
                let refresh_cache =
                    info_span!("refresh_cache", file = %cache_entry.path().display());
                async {
                    let data_with_cache_policy_bytes =
                        DataWithCachePolicy::serialize(&new_policy, &cached.data)?;
                    write_atomic(cache_entry.path(), data_with_cache_policy_bytes)
                        .await
                        .map_err(ErrorKind::CacheWrite)?;
                    match Payload::from_aligned_bytes(cached.data) {
                        Ok(payload) => Ok(payload),
                        Err(err) => {
                            warn!(
                                "Broken fresh cache entry after revalidation \
                                 (for payload) at {}, removing: {err}",
                                cache_entry.path().display()
                            );
                            self.resend_and_heal_cache(fresh_req, cache_entry, response_callback)
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
                    warn!("Server returned unusable 304 for: {}", fresh_req.url());
                    self.resend_and_heal_cache(fresh_req, cache_entry, response_callback)
                        .await
                } else {
                    self.run_response_callback(
                        cache_entry,
                        cache_policy,
                        response,
                        response_callback,
                    )
                    .await
                }
            }
        }
    }

    /// Make a request without checking whether the cache is fresh.
    pub async fn skip_cache<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>>
    where
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let (response, cache_policy) = self.fresh_request(req).await?;

        let payload = self
            .run_response_callback(cache_entry, cache_policy, response, move |resp| async {
                let payload = response_callback(resp).await?;
                Ok(SerdeCacheable { inner: payload })
            })
            .await?;

        Ok(payload)
    }

    async fn resend_and_heal_cache<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>>
    where
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
        let (response, cache_policy) = self.fresh_request(req).await?;
        self.run_response_callback(cache_entry, cache_policy, response, response_callback)
            .await
    }

    async fn run_response_callback<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        cache_entry: &CacheEntry,
        cache_policy: Option<Box<CachePolicy>>,
        response: Response,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>>
    where
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let new_cache = info_span!("new_cache", file = %cache_entry.path().display());
        let data = response_callback(response)
            .boxed_local()
            .await
            .map_err(|err| CachedClientError::Callback(err))?;
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

    #[instrument(name="read_and_parse_cache", skip_all, fields(file = %cache_entry.path().display()))]
    async fn read_cache(cache_entry: &CacheEntry) -> Option<DataWithCachePolicy> {
        match DataWithCachePolicy::from_path_async(cache_entry.path()).await {
            Ok(data) => Some(data),
            Err(err) => {
                // When we know the cache entry doesn't exist, then things are
                // normal and we shouldn't emit a WARN.
                if err.is_file_not_exists() {
                    trace!("No cache entry exists for {}", cache_entry.path().display());
                } else {
                    warn!(
                        "Broken cache policy entry at {}, removing: {err}",
                        cache_entry.path().display()
                    );
                    let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
                }
                None
            }
        }
    }

    /// Send a request given that we have a (possibly) stale cached response.
    ///
    /// If the cached response is valid but stale, then this will attempt a
    /// revalidation request.
    async fn send_cached(
        &self,
        mut req: Request,
        cache_control: CacheControl,
        cached: DataWithCachePolicy,
    ) -> Result<CachedResponse, Error> {
        // Apply the cache control header, if necessary.
        match cache_control {
            CacheControl::None | CacheControl::AllowStale => {}
            CacheControl::MustRevalidate => {
                req.headers_mut().insert(
                    http::header::CACHE_CONTROL,
                    http::HeaderValue::from_static("no-cache"),
                );
            }
        }
        Ok(match cached.cache_policy.before_request(&mut req) {
            BeforeRequest::Fresh => {
                debug!("Found fresh response for: {}", req.url());
                CachedResponse::FreshCache(cached)
            }
            BeforeRequest::Stale(new_cache_policy_builder) => match cache_control {
                CacheControl::None | CacheControl::MustRevalidate => {
                    debug!("Found stale response for: {}", req.url());
                    self.send_cached_handle_stale(req, cached, new_cache_policy_builder)
                        .await?
                }
                CacheControl::AllowStale => {
                    debug!("Found stale (but allowed) response for: {}", req.url());
                    CachedResponse::FreshCache(cached)
                }
            },
            BeforeRequest::NoMatch => {
                // This shouldn't happen; if it does, we'll override the cache.
                warn!(
                    "Cached request doesn't match current request for: {}",
                    req.url()
                );
                let (response, cache_policy) = self.fresh_request(req).await?;
                CachedResponse::ModifiedOrNew {
                    response,
                    cache_policy,
                }
            }
        })
    }

    async fn send_cached_handle_stale(
        &self,
        req: Request,
        cached: DataWithCachePolicy,
        new_cache_policy_builder: CachePolicyBuilder,
    ) -> Result<CachedResponse, Error> {
        let url = req.url().clone();
        debug!("Sending revalidation request for: {url}");
        let response = self
            .0
            .for_host(req.url())
            .execute(req)
            .instrument(info_span!("revalidation_request", url = url.as_str()))
            .await
            .map_err(|err| ErrorKind::from_reqwest_middleware(url.clone(), err))?
            .error_for_status()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
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

    #[instrument(skip_all, fields(url = req.url().as_str()))]
    async fn fresh_request(
        &self,
        req: Request,
    ) -> Result<(Response, Option<Box<CachePolicy>>), Error> {
        let url = req.url().clone();
        trace!("Sending fresh {} request for {}", req.method(), url);
        let cache_policy_builder = CachePolicyBuilder::new(&req);
        let response = self
            .0
            .for_host(&url)
            .execute(req)
            .await
            .map_err(|err| ErrorKind::from_reqwest_middleware(url.clone(), err))?
            .error_for_status()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
        let cache_policy = cache_policy_builder.build(&response);
        let cache_policy = if cache_policy.to_archived().is_storable() {
            Some(Box::new(cache_policy))
        } else {
            None
        };
        Ok((response, cache_policy))
    }

    /// Perform a [`CachedClient::get_serde`] request with a default retry strategy.
    #[instrument(skip_all)]
    pub async fn get_serde_with_retry<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>>
    where
        Callback: Fn(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let payload = self
            .get_cacheable_with_retry(req, cache_entry, cache_control, |resp| async {
                let payload = response_callback(resp).await?;
                Ok(SerdeCacheable { inner: payload })
            })
            .await?;
        Ok(payload)
    }

    /// Perform a [`CachedClient::get_cacheable`] request with a default retry strategy.
    ///
    /// See: <https://github.com/TrueLayer/reqwest-middleware/blob/8a494c165734e24c62823714843e1c9347027e8a/reqwest-retry/src/middleware.rs#L137>
    #[instrument(skip_all)]
    pub async fn get_cacheable_with_retry<
        Payload: Cacheable,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
        response_callback: Callback,
    ) -> Result<Payload::Target, CachedClientError<CallBackError>>
    where
        Callback: Fn(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let mut n_past_retries = 0;
        let start_time = SystemTime::now();
        let retry_policy = self.uncached().retry_policy();
        loop {
            let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
            let result = self
                .get_cacheable(fresh_req, cache_entry, cache_control, &response_callback)
                .await;
            if result
                .as_ref()
                .err()
                .is_some_and(|err| is_extended_transient_error(err))
            {
                let retry_decision = retry_policy.should_retry(start_time, n_past_retries);
                if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                    debug!(
                        "Transient failure while handling response from {}; retrying...",
                        req.url(),
                    );
                    let duration = execute_after
                        .duration_since(SystemTime::now())
                        .unwrap_or_else(|_| Duration::default());
                    tokio::time::sleep(duration).await;
                    n_past_retries += 1;
                    continue;
                }
            }
            return result;
        }
    }

    /// Perform a [`CachedClient::skip_cache`] request with a default retry strategy.
    ///
    /// See: <https://github.com/TrueLayer/reqwest-middleware/blob/8a494c165734e24c62823714843e1c9347027e8a/reqwest-retry/src/middleware.rs#L137>
    pub async fn skip_cache_with_retry<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError: std::error::Error + 'static,
        Callback,
        CallbackReturn,
    >(
        &self,
        req: Request,
        cache_entry: &CacheEntry,
        response_callback: Callback,
    ) -> Result<Payload, CachedClientError<CallBackError>>
    where
        Callback: Fn(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>>,
    {
        let mut n_past_retries = 0;
        let start_time = SystemTime::now();
        let retry_policy = self.uncached().retry_policy();
        loop {
            let fresh_req = req.try_clone().expect("HTTP request must be cloneable");
            let result = self
                .skip_cache(fresh_req, cache_entry, &response_callback)
                .await;
            if result
                .as_ref()
                .err()
                .is_some_and(|err| is_extended_transient_error(err))
            {
                let retry_decision = retry_policy.should_retry(start_time, n_past_retries);
                if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                    debug!(
                        "Transient failure while handling response from {}; retrying...",
                        req.url(),
                    );
                    let duration = execute_after
                        .duration_since(SystemTime::now())
                        .unwrap_or_else(|_| Duration::default());
                    tokio::time::sleep(duration).await;
                    n_past_retries += 1;
                    continue;
                }
            }
            return result;
        }
    }
}

#[derive(Debug)]
enum CachedResponse {
    /// The cached response is fresh without an HTTP request (e.g. age < max-age).
    FreshCache(DataWithCachePolicy),
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
        /// The policy is large (352 bytes at time of writing), so we reduce
        /// the stack size by boxing it.
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
        /// The policy is large (352 bytes at time of writing), so we reduce
        /// the stack size by boxing it.
        cache_policy: Option<Box<CachePolicy>>,
    },
}

/// Represents an arbitrary data blob with an associated HTTP cache policy.
///
/// The cache policy is used to determine whether the data blob is stale or
/// not.
///
/// # Format
///
/// This type encapsulates the format for how blobs of data are stored on
/// disk. The format is very simple. First, the blob of data is written as-is.
/// Second, the archived representation of a `CachePolicy` is written. Thirdly,
/// the length, in bytes, of the archived `CachePolicy` is written as a 64-bit
/// little endian integer.
///
/// Reading the format is done via an `AlignedVec` so that `rkyv` can correctly
/// read the archived representation of the data blob. The cache policy is
/// split into its own `AlignedVec` allocation.
///
/// # Future ideas
///
/// This format was also chosen because it should in theory permit rewriting
/// the cache policy without needing to rewrite the data blob if the blob has
/// not changed. For example, this case occurs when a revalidation request
/// responds with HTTP 304 NOT MODIFIED. At time of writing, this is not yet
/// implemented because 1) the synchronization specifics of mutating a cache
/// file have not been worked out and 2) it's not clear if it's a win.
///
/// An alternative format would be to write the cache policy and the
/// blob in two distinct files. This would avoid needing to worry about
/// synchronization, but it means reading two files instead of one for every
/// cached response in the fast path. It's unclear whether it's worth it.
/// (Experiments have not yet been done.)
///
/// Another approach here would be to memory map the file and rejigger
/// `OwnedArchive` (or create a new type) that works with a memory map instead
/// of an `AlignedVec`. This will require care to ensure alignment is handled
/// correctly. This approach has not been litigated yet. I did not start with
/// it because experiments with ripgrep have tended to show that (on Linux)
/// memory mapping a bunch of small files ends up being quite a bit slower than
/// just reading them on to the heap.
#[derive(Debug)]
pub struct DataWithCachePolicy {
    pub data: AlignedVec,
    cache_policy: OwnedArchive<CachePolicy>,
}

impl DataWithCachePolicy {
    /// Loads cached data and its associated HTTP cache policy from the given
    /// file path in an asynchronous fashion (via `spawn_blocking`).
    ///
    /// # Errors
    ///
    /// If the given byte buffer is not in a valid format or if reading the
    /// file given fails, then this returns an error.
    async fn from_path_async(path: &Path) -> Result<Self, Error> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || Self::from_path_sync(&path))
            .await
            // This just forwards panics from the closure.
            .unwrap()
    }

    /// Loads cached data and its associated HTTP cache policy from the given
    /// file path in a synchronous fashion.
    ///
    /// # Errors
    ///
    /// If the given byte buffer is not in a valid format or if reading the
    /// file given fails, then this returns an error.
    #[instrument]
    fn from_path_sync(path: &Path) -> Result<Self, Error> {
        let file = fs_err::File::open(path).map_err(ErrorKind::Io)?;
        // Note that we don't wrap our file in a buffer because it will just
        // get passed to AlignedVec::extend_from_reader, which doesn't benefit
        // from an intermediary buffer. In effect, the AlignedVec acts as the
        // buffer.
        Self::from_reader(file)
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

    /// Loads cached data and its associated HTTP cache policy form an in
    /// memory byte buffer.
    ///
    /// # Errors
    ///
    /// If the given byte buffer is not in a valid format, then this
    /// returns an error.
    fn from_aligned_bytes(mut bytes: AlignedVec) -> Result<Self, Error> {
        let cache_policy = Self::deserialize_cache_policy(&mut bytes)?;
        Ok(Self {
            data: bytes,
            cache_policy,
        })
    }

    /// Serializes the given cache policy and arbitrary data blob to an in
    /// memory byte buffer.
    ///
    /// # Errors
    ///
    /// If there was a problem converting the given cache policy to its
    /// serialized representation, then this routine will return an error.
    fn serialize(cache_policy: &CachePolicy, data: &[u8]) -> Result<Vec<u8>, Error> {
        let mut buf = vec![];
        Self::serialize_to_writer(cache_policy, data, &mut buf)?;
        Ok(buf)
    }

    /// Serializes the given cache policy and arbitrary data blob to the given
    /// writer.
    ///
    /// # Errors
    ///
    /// If there was a problem converting the given cache policy to its
    /// serialized representation or if the writer returns an error, then
    /// this routine will return an error.
    fn serialize_to_writer(
        cache_policy: &CachePolicy,
        data: &[u8],
        mut wtr: impl std::io::Write,
    ) -> Result<(), Error> {
        let cache_policy_archived = OwnedArchive::from_unarchived(cache_policy)?;
        let cache_policy_bytes = OwnedArchive::as_bytes(&cache_policy_archived);
        wtr.write_all(data).map_err(ErrorKind::Io)?;
        wtr.write_all(cache_policy_bytes).map_err(ErrorKind::Io)?;
        let len = u64::try_from(cache_policy_bytes.len()).map_err(|_| {
            let msg = format!(
                "failed to represent {} (length of cache policy) in a u64",
                cache_policy_bytes.len()
            );
            ErrorKind::Io(std::io::Error::other(msg))
        })?;
        wtr.write_all(&len.to_le_bytes()).map_err(ErrorKind::Io)?;
        Ok(())
    }

    /// Deserializes a `OwnedArchive<CachePolicy>` off the end of the given
    /// aligned bytes. Upon success, the given bytes will only contain the
    /// data itself. The bytes representing the cached policy will have been
    /// removed.
    ///
    /// # Errors
    ///
    /// This returns an error if the cache policy could not be deserialized
    /// from the end of the given bytes.
    fn deserialize_cache_policy(
        bytes: &mut AlignedVec,
    ) -> Result<OwnedArchive<CachePolicy>, Error> {
        let len = Self::deserialize_cache_policy_len(bytes)?;
        let cache_policy_bytes_start = bytes.len() - (len + 8);
        let cache_policy_bytes = &bytes[cache_policy_bytes_start..][..len];
        let mut cache_policy_bytes_aligned = AlignedVec::with_capacity(len);
        cache_policy_bytes_aligned.extend_from_slice(cache_policy_bytes);
        assert!(
            cache_policy_bytes_start <= bytes.len(),
            "slicing cache policy should result in a truncation"
        );
        // Technically this will keep the extra capacity used to store the
        // cache policy around. But it should be pretty small, and it saves a
        // realloc. (It's unclear whether that matters more or less than the
        // extra memory usage.)
        bytes.resize(cache_policy_bytes_start, 0);
        OwnedArchive::new(cache_policy_bytes_aligned)
    }

    /// Deserializes the length, in bytes, of the cache policy given a complete
    /// serialized byte buffer of a `DataWithCachePolicy`.
    ///
    /// Upon success, callers are guaranteed that
    /// `&bytes[bytes.len() - (len + 8)..][..len]` will not panic.
    ///
    /// # Errors
    ///
    /// This returns an error if the length could not be read as a `usize` or is
    /// otherwise known to be invalid. (For example, it is a length that is bigger
    /// than `bytes.len()`.)
    fn deserialize_cache_policy_len(bytes: &[u8]) -> Result<usize, Error> {
        let Some(cache_policy_len_start) = bytes.len().checked_sub(8) else {
            let msg = format!(
                "data-with-cache-policy buffer should be at least 8 bytes \
                 in length, but is {} bytes",
                bytes.len(),
            );
            return Err(ErrorKind::ArchiveRead(msg).into());
        };
        let cache_policy_len_bytes = <[u8; 8]>::try_from(&bytes[cache_policy_len_start..])
            .expect("cache policy length is 8 bytes");
        let len_u64 = u64::from_le_bytes(cache_policy_len_bytes);
        let Ok(len_usize) = usize::try_from(len_u64) else {
            let msg = format!(
                "data-with-cache-policy has cache policy length of {len_u64}, \
                 but overflows usize",
            );
            return Err(ErrorKind::ArchiveRead(msg).into());
        };
        if bytes.len() < len_usize + 8 {
            let msg = format!(
                "invalid cache entry: data-with-cache-policy has cache policy length of {}, \
                 but total buffer size is {}",
                len_usize,
                bytes.len(),
            );
            return Err(ErrorKind::ArchiveRead(msg).into());
        }
        Ok(len_usize)
    }
}
