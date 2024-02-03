use std::future::Future;

use futures::FutureExt;
use reqwest::{Request, Response};
use reqwest_middleware::ClientWithMiddleware;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{debug, info_span, instrument, trace, warn, Instrument};

use puffin_cache::{CacheEntry, Freshness};
use puffin_fs::write_atomic;

use crate::{
    httpcache::{AfterResponse, BeforeRequest, CachePolicy, CachePolicyBuilder},
    Error, ErrorKind,
};

/// Either a cached client error or a (user specified) error from the callback
#[derive(Debug)]
pub enum CachedClientError<CallbackError> {
    Client(Error),
    Callback(CallbackError),
}

impl<CallbackError> From<Error> for CachedClientError<CallbackError> {
    fn from(error: Error) -> Self {
        CachedClientError::Client(error)
    }
}

impl<CallbackError> From<ErrorKind> for CachedClientError<CallbackError> {
    fn from(error: ErrorKind) -> Self {
        CachedClientError::Client(error.into())
    }
}

impl<E: Into<Error>> From<CachedClientError<E>> for Error {
    fn from(error: CachedClientError<E>) -> Error {
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
}

impl From<Freshness> for CacheControl {
    fn from(value: Freshness) -> Self {
        match value {
            Freshness::Fresh => CacheControl::None,
            Freshness::Stale => CacheControl::MustRevalidate,
            Freshness::Missing => CacheControl::None,
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
/// Unlike `http-cache`, all outputs must be serde-able. Currently everything is json, but we can
/// transparently switch to a faster/smaller format.
///
/// Again unlike `http-cache`, the caller gets full control over the cache key with the assumption
/// that it's a file.
#[derive(Debug, Clone)]
pub struct CachedClient(ClientWithMiddleware);

impl CachedClient {
    pub fn new(client: ClientWithMiddleware) -> Self {
        Self(client)
    }

    /// The middleware is the retry strategy
    pub fn uncached(&self) -> ClientWithMiddleware {
        self.0.clone()
    }

    /// Make a cached request with a custom response transformation
    ///
    /// If a new response was received (no prior cached response or modified on the remote), the
    /// response is passed through `response_callback` and only the result is cached and returned.
    /// The `response_callback` is allowed to make subsequent requests, e.g. through the uncached
    /// client.
    #[instrument(skip_all)]
    pub async fn get_cached_with_callback<
        Payload: Serialize + DeserializeOwned + Send + 'static,
        CallBackError,
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
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>> + Send,
    {
        let cached_response = match Self::read_cache(cache_entry).await {
            Some(cached) => self.send_cached(req, cache_control, cached).boxed().await?,
            None => {
                debug!("No cache entry for: {}", req.url());
                self.fresh_request(req).await?
            }
        };
        match cached_response {
            CachedResponse::FreshCache(data) => Ok(data),
            CachedResponse::NotModified(data_with_cache_policy) => {
                let refresh_cache =
                    info_span!("refresh_cache", file = %cache_entry.path().display());
                async {
                    let data =
                        rmp_serde::to_vec(&data_with_cache_policy).map_err(ErrorKind::Encode)?;
                    write_atomic(cache_entry.path(), data)
                        .await
                        .map_err(ErrorKind::CacheWrite)?;
                    Ok(data_with_cache_policy.data)
                }
                .instrument(refresh_cache)
                .await
            }
            CachedResponse::ModifiedOrNew(res, cache_policy) => {
                let new_cache = info_span!("new_cache", file = %cache_entry.path().display());
                let data = response_callback(res)
                    .boxed()
                    .await
                    .map_err(|err| CachedClientError::Callback(err))?;
                let Some(cache_policy) = cache_policy else {
                    return Ok(data);
                };
                let data_with_cache_policy = DataWithCachePolicy { data, cache_policy };
                async {
                    fs_err::tokio::create_dir_all(cache_entry.dir())
                        .await
                        .map_err(ErrorKind::CacheWrite)?;
                    let data =
                        rmp_serde::to_vec(&data_with_cache_policy).map_err(ErrorKind::Encode)?;
                    write_atomic(cache_entry.path(), data)
                        .await
                        .map_err(ErrorKind::CacheWrite)?;
                    Ok(data_with_cache_policy.data)
                }
                .instrument(new_cache)
                .await
            }
        }
    }

    async fn read_cache<Payload: Serialize + DeserializeOwned + Send + 'static>(
        cache_entry: &CacheEntry,
    ) -> Option<DataWithCachePolicy<Payload>> {
        let read_span = info_span!("read_cache", file = %cache_entry.path().display());
        let cached = fs_err::tokio::read(cache_entry.path())
            .instrument(read_span)
            .await
            .ok()?;

        let parse_span = info_span!(
            "parse_cache",
            path = %cache_entry.path().display()
        );
        let parse_result = tokio::task::spawn_blocking(move || {
            parse_span.in_scope(|| rmp_serde::from_slice::<DataWithCachePolicy<Payload>>(&cached))
        })
        .await
        .expect("Tokio executor failed, was there a panic?");
        match parse_result {
            Ok(data) => Some(data),
            Err(err) => {
                warn!(
                    "Broken cache entry at {}, removing: {err}",
                    cache_entry.path().display()
                );
                let _ = fs_err::tokio::remove_file(&cache_entry.path()).await;
                None
            }
        }
    }

    /// Send a request given that we have a (possibly) stale cached response.
    ///
    /// If the cached response is valid but stale, then this will attempt a
    /// revalidation request.
    async fn send_cached<T: Serialize + DeserializeOwned>(
        &self,
        mut req: Request,
        cache_control: CacheControl,
        cached: DataWithCachePolicy<T>,
    ) -> Result<CachedResponse<T>, Error> {
        // Apply the cache control header, if necessary.
        match cache_control {
            CacheControl::None => {}
            CacheControl::MustRevalidate => {
                req.headers_mut().insert(
                    http::header::CACHE_CONTROL,
                    http::HeaderValue::from_static("no-cache"),
                );
            }
        }
        Ok(
            match cached.cache_policy.to_archived().before_request(&mut req) {
                BeforeRequest::Fresh => {
                    debug!("Found fresh response for: {}", req.url());
                    CachedResponse::FreshCache(cached.data)
                }
                BeforeRequest::Stale(new_cache_policy_builder) => {
                    debug!("Found stale response for: {}", req.url());
                    self.send_cached_handle_stale(req, cached, new_cache_policy_builder)
                        .await?
                }
                BeforeRequest::NoMatch => {
                    // This shouldn't happen; if it does, we'll override the cache.
                    warn!(
                        "Cached request doesn't match current request for: {}",
                        req.url()
                    );
                    self.fresh_request(req).await?
                }
            },
        )
    }

    async fn send_cached_handle_stale<T: Serialize + DeserializeOwned>(
        &self,
        req: Request,
        cached: DataWithCachePolicy<T>,
        new_cache_policy_builder: CachePolicyBuilder,
    ) -> Result<CachedResponse<T>, Error> {
        let url = req.url().clone();
        debug!("Sending revalidation request for: {url}");
        let res = self
            .0
            .execute(req)
            .instrument(info_span!("revalidation_request", url = url.as_str()))
            .await
            .map_err(ErrorKind::RequestMiddlewareError)?
            .error_for_status()
            .map_err(ErrorKind::RequestError)?;
        match cached
            .cache_policy
            .to_archived()
            .after_response(new_cache_policy_builder, &res)
        {
            AfterResponse::NotModified(new_policy) => {
                debug!("Found not-modified response for: {url}");
                Ok(CachedResponse::NotModified(DataWithCachePolicy {
                    data: cached.data,
                    cache_policy: Box::new(new_policy),
                }))
            }
            AfterResponse::Modified(new_policy) => {
                debug!("Found modified response for: {url}");
                Ok(CachedResponse::ModifiedOrNew(
                    res,
                    new_policy
                        .to_archived()
                        .is_storable()
                        .then(|| Box::new(new_policy)),
                ))
            }
        }
    }

    #[instrument(skip_all, fields(url = req.url().as_str()))]
    async fn fresh_request<T: Serialize>(&self, req: Request) -> Result<CachedResponse<T>, Error> {
        trace!("Sending fresh {} request for {}", req.method(), req.url());
        let cache_policy_builder = CachePolicyBuilder::new(&req);
        let res = self
            .0
            .execute(req)
            .await
            .map_err(ErrorKind::RequestMiddlewareError)?
            .error_for_status()
            .map_err(ErrorKind::RequestError)?;
        let cache_policy = cache_policy_builder.build(&res);
        Ok(CachedResponse::ModifiedOrNew(
            res,
            cache_policy
                .to_archived()
                .is_storable()
                .then(|| Box::new(cache_policy)),
        ))
    }
}

#[derive(Debug)]
enum CachedResponse<Payload: Serialize> {
    /// The cached response is fresh without an HTTP request (e.g. age < max-age).
    FreshCache(Payload),
    /// The cached response is fresh after an HTTP request (e.g. 304 not modified)
    NotModified(DataWithCachePolicy<Payload>),
    /// There was no prior cached response or the cache was outdated
    ///
    /// The cache policy is `None` if it isn't storable
    ModifiedOrNew(Response, Option<Box<CachePolicy>>),
}

/// Serialize the actual payload together with its caching information.
#[derive(Debug, Deserialize, Serialize)]
pub struct DataWithCachePolicy<Payload: Serialize> {
    pub data: Payload,
    /// The [`CachePolicy`] is used to determine if the response is fresh or stale.
    /// The policy is large (448 bytes at time of writing), so we reduce the stack size by
    /// boxing it.
    cache_policy: Box<CachePolicy>,
}
