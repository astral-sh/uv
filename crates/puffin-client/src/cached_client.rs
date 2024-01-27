#![allow(warnings)]

use std::fmt::Debug;
use std::future::Future;
use std::time::SystemTime;

use futures::FutureExt;
use http::request::Parts;
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use reqwest::{Request, Response};
use reqwest_middleware::ClientWithMiddleware;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::{debug, info_span, instrument, trace, warn, Instrument};
use url::Url;

use puffin_cache::{CacheEntry, Freshness};
use puffin_fs::write_atomic;

use crate::{cache_headers::CacheHeaders, rkyvutil::OwnedArchive, Error, ErrorKind};

pub trait Cacheable: Sized + Send {
    type Target;

    fn from_bytes(bytes: Vec<u8>) -> Result<Self::Target, crate::Error>;
    fn to_bytes(&self) -> Result<Vec<u8>, crate::Error>;
    fn into_target(self) -> Self::Target;
}

/// A wrapper type that makes anything with Serde support automatically
/// implement Cacheable.
#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct SerdeCacheable<T> {
    inner: T,
}

impl<T: Send + Serialize + DeserializeOwned> Cacheable for SerdeCacheable<T> {
    type Target = T;

    fn from_bytes(bytes: Vec<u8>) -> Result<T, Error> {
        Ok(rmp_serde::from_slice::<T>(&bytes).map_err(ErrorKind::Decode)?)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(rmp_serde::to_vec(&self.inner).map_err(ErrorKind::Encode)?)
    }

    fn into_target(self) -> Self::Target {
        self.inner
    }
}

impl<A> Cacheable for OwnedArchive<A>
where
    A: rkyv::Archive + rkyv::Serialize<crate::rkyvutil::Serializer<4096>> + Send,
    A::Archived: for<'a> rkyv::CheckBytes<rkyv::validation::validators::DefaultValidator<'a>>
        + rkyv::Deserialize<A, rkyv::de::deserializers::SharedDeserializeMap>,
{
    type Target = OwnedArchive<A>;

    fn from_bytes(bytes: Vec<u8>) -> Result<OwnedArchive<A>, Error> {
        let mut aligned = rkyv::util::AlignedVec::new();
        aligned.extend_from_slice(&bytes);
        OwnedArchive::new(aligned)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(OwnedArchive::as_bytes(self).to_vec())
    }

    fn into_target(self) -> Self::Target {
        self
    }
}

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

#[derive(Debug)]
enum CachedResponse {
    /// The cached response is fresh without an HTTP request (e.g. immutable)
    FreshCache(Vec<u8>),
    /// The cached response is fresh after an HTTP request (e.g. 304 not modified)
    NotModified(DataWithCachePolicy),
    /// There was no prior cached response or the cache was outdated
    ///
    /// The cache policy is `None` if it isn't storable
    // ModifiedOrNew(Response, Option<Box<CachePolicy>>),
    ModifiedOrNew(Response, Option<Box<CachePolicyStub>>),
}

/// Serialize the actual payload together with its caching information.
#[derive(Debug, Deserialize, Serialize)]
pub struct DataWithCachePolicy {
    pub data: Vec<u8>,
    /// Whether the response should be considered immutable.
    immutable: bool,
    /// The [`CachePolicy`] is used to determine if the response is fresh or stale.
    /// The policy is large (448 bytes at time of writing), so we reduce the stack size by
    /// boxing it.
    cache_policy: Box<CachePolicyStub>,
}

#[derive(Debug)]
struct CachePolicyStub(Option<CachePolicy>);

impl CachePolicyStub {
    fn is_stale(&self, time: SystemTime) -> bool {
        self.0.as_ref().map_or(false, |p| p.is_stale(time))
    }

    fn is_storable(&self) -> bool {
        self.0.as_ref().map_or(false, |p| p.is_storable())
    }

    fn before_request<Req: http_cache_semantics::RequestLike>(
        &self,
        req: &Req,
        now: SystemTime,
    ) -> BeforeRequestStub {
        match self.0.as_ref() {
            None => {
                let dummy = http::Response::new(()).into_parts().0;
                BeforeRequestStub::Fresh(dummy)
            }
            Some(p) => p.before_request(req, now).into(),
        }
    }

    fn after_response<
        Req: http_cache_semantics::RequestLike,
        Resp: http_cache_semantics::ResponseLike,
    >(
        &self,
        req: &Req,
        resp: &Resp,
        time: SystemTime,
    ) -> AfterResponseStub {
        match self.0.as_ref() {
            None => unreachable!("oops"),
            Some(p) => p.after_response(req, resp, time).into(),
        }
    }
}

impl<'de> Deserialize<'de> for CachePolicyStub {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let p = CachePolicy::deserialize(deserializer)?;
        Ok(CachePolicyStub(Some(p)))
    }
}

impl Serialize for CachePolicyStub {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.0.as_ref().unwrap().serialize(serializer)
    }
}

enum BeforeRequestStub {
    Fresh(http::response::Parts),
    Stale {
        request: http::request::Parts,
        matches: bool,
    },
}

impl From<BeforeRequest> for BeforeRequestStub {
    fn from(br: BeforeRequest) -> BeforeRequestStub {
        match br {
            BeforeRequest::Fresh(parts) => BeforeRequestStub::Fresh(parts),
            BeforeRequest::Stale { request, matches } => {
                BeforeRequestStub::Stale { request, matches }
            }
        }
    }
}

enum AfterResponseStub {
    NotModified(CachePolicyStub, http::response::Parts),
    Modified(CachePolicyStub, http::response::Parts),
}

impl From<AfterResponse> for AfterResponseStub {
    fn from(ar: AfterResponse) -> AfterResponseStub {
        match ar {
            AfterResponse::NotModified(p, parts) => {
                AfterResponseStub::NotModified(CachePolicyStub(Some(p)), parts)
            }
            AfterResponse::Modified(p, parts) => {
                AfterResponseStub::Modified(CachePolicyStub(Some(p)), parts)
            }
        }
    }
}

/// Custom caching layer over [`reqwest::Client`] using `http-cache-semantics`.
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
        Callback: FnOnce(Response) -> CallbackReturn + Send,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>> + Send,
    {
        let payload = self
            .get_cached_with_callback2(req, cache_entry, cache_control, move |resp| async {
                let payload = response_callback(resp).await?;
                Ok(SerdeCacheable { inner: payload })
            })
            .await?;
        Ok(payload)
    }

    #[instrument(skip_all)]
    pub async fn get_cached_with_callback2<
        Payload: Cacheable,
        CallBackError,
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
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, CallBackError>> + Send,
    {
        let cached = Self::read_cache(&req, cache_entry).await;

        let cached_response = self.send_cached(req, cache_control, cached).boxed().await?;

        let write_cache = info_span!("write_cache", file = %cache_entry.path().display());
        match cached_response {
            CachedResponse::FreshCache(data) => Ok(Payload::from_bytes(data)?),
            CachedResponse::NotModified(data_with_cache_policy) => {
                async {
                    if std::env::var("PUFFIN_STUB_CACHE_POLICY").map_or(false, |v| v == "1") {
                        write_atomic(cache_entry.path(), &data_with_cache_policy.data)
                            .await
                            .map_err(ErrorKind::CacheWrite)?;
                    } else {
                        let data = rmp_serde::to_vec(&data_with_cache_policy)
                            .map_err(ErrorKind::Encode)?;
                        write_atomic(cache_entry.path(), &data)
                            .await
                            .map_err(ErrorKind::CacheWrite)?;
                    }
                    Ok(Payload::from_bytes(data_with_cache_policy.data)?)
                }
                .instrument(write_cache)
                .await
            }
            CachedResponse::ModifiedOrNew(res, cache_policy) => {
                let headers = CacheHeaders::from_response(res.headers().get_all("cache-control"));
                let immutable = headers.is_immutable();

                let data = response_callback(res)
                    .boxed()
                    .await
                    .map_err(|err| CachedClientError::Callback(err))?;
                if let Some(cache_policy) = cache_policy {
                    let data_with_cache_policy = DataWithCachePolicy {
                        data: data.to_bytes()?,
                        immutable,
                        cache_policy,
                    };
                    async {
                        fs_err::tokio::create_dir_all(cache_entry.dir())
                            .await
                            .map_err(ErrorKind::CacheWrite)?;
                        if std::env::var("PUFFIN_STUB_CACHE_POLICY").map_or(false, |v| v == "1") {
                            write_atomic(cache_entry.path(), &data_with_cache_policy.data)
                                .await
                                .map_err(ErrorKind::CacheWrite)?;
                        } else {
                            let envelope = rmp_serde::to_vec(&data_with_cache_policy)
                                .map_err(ErrorKind::Encode)?;
                            write_atomic(cache_entry.path(), envelope)
                                .await
                                .map_err(ErrorKind::CacheWrite)?;
                        }
                        Ok(data.into_target())
                    }
                    .instrument(write_cache)
                    .await
                } else {
                    Ok(data.into_target())
                }
            }
        }
    }

    async fn read_cache(req: &Request, cache_entry: &CacheEntry) -> Option<DataWithCachePolicy> {
        let read_span = info_span!("read_cache", file = %cache_entry.path().display());
        let read_result = fs_err::tokio::read(cache_entry.path())
            .instrument(read_span)
            .await;

        if let Ok(cached) = read_result {
            let parse_span = info_span!(
                "parse_cache",
                path = %cache_entry.path().display()
            );
            if std::env::var("PUFFIN_STUB_CACHE_POLICY").map_or(false, |v| v == "1") {
                Some(DataWithCachePolicy {
                    data: cached,
                    immutable: req.url().as_str().contains("pypi.org"),
                    cache_policy: Box::new(CachePolicyStub(None)),
                })
            } else {
                let parse_result = tokio::task::spawn_blocking(move || {
                    parse_span.in_scope(|| rmp_serde::from_slice::<DataWithCachePolicy>(&cached))
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
        } else {
            None
        }
    }

    /// `http-cache-semantics` to `reqwest` wrapper
    async fn send_cached(
        &self,
        mut req: Request,
        cache_control: CacheControl,
        cached: Option<DataWithCachePolicy>,
    ) -> Result<CachedResponse, Error> {
        if std::env::var("PUFFIN_STUB_CACHE_POLICY").map_or(false, |v| v == "1") && cached.is_some()
        {
            return Ok(CachedResponse::FreshCache(cached.expect("wat").data));
        }

        let url = req.url().clone();
        let cached_response = if let Some(cached) = cached {
            // Avoid sending revalidation requests for immutable responses.
            if cached.immutable && !cached.cache_policy.is_stale(SystemTime::now()) {
                debug!("Found immutable response for: {url}");
                return Ok(CachedResponse::FreshCache(cached.data));
            }

            // Apply the cache control header, if necessary.
            match cache_control {
                CacheControl::None => {}
                CacheControl::MustRevalidate => {
                    req.headers_mut().insert(
                        http::header::CACHE_CONTROL,
                        http::HeaderValue::from_static("max-age=0, must-revalidate"),
                    );
                }
            }

            match cached
                .cache_policy
                .before_request(&RequestLikeReqwest(&req), SystemTime::now())
            {
                BeforeRequestStub::Fresh(_) => {
                    debug!("Found fresh response for: {url}");
                    CachedResponse::FreshCache(cached.data)
                }
                BeforeRequestStub::Stale { request, matches } => {
                    self.send_cached_handle_stale(req, url, cached, &request, matches)
                        .await?
                }
            }
        } else {
            debug!("No cache entry for: {url}");
            self.fresh_request(req).await?
        };
        Ok(cached_response)
    }

    async fn send_cached_handle_stale(
        &self,
        mut req: Request,
        url: Url,
        cached: DataWithCachePolicy,
        request: &Parts,
        matches: bool,
    ) -> Result<CachedResponse, Error> {
        if !matches {
            // This shouldn't happen; if it does, we'll override the cache.
            warn!("Cached request doesn't match current request for: {url}");
            return self.fresh_request(req).await;
        }

        debug!("Sending revalidation request for: {url}");
        for header in &request.headers {
            req.headers_mut().insert(header.0.clone(), header.1.clone());
        }
        let res = self
            .0
            .execute(req.try_clone().expect("streaming requests not supported"))
            .instrument(info_span!("revalidation_request", url = url.as_str()))
            .await
            .map_err(ErrorKind::RequestMiddlewareError)?
            .error_for_status()
            .map_err(ErrorKind::RequestError)?;
        let after_response = cached.cache_policy.after_response(
            &RequestLikeReqwest(&req),
            &ResponseLikeReqwest(&res),
            SystemTime::now(),
        );
        match after_response {
            AfterResponseStub::NotModified(new_policy, _parts) => {
                debug!("Found not-modified response for: {url}");
                let headers = CacheHeaders::from_response(res.headers().get_all("cache-control"));
                let immutable = headers.is_immutable();
                Ok(CachedResponse::NotModified(DataWithCachePolicy {
                    data: cached.data,
                    immutable,
                    cache_policy: Box::new(new_policy),
                }))
            }
            AfterResponseStub::Modified(new_policy, _parts) => {
                debug!("Found modified response for: {url}");
                Ok(CachedResponse::ModifiedOrNew(
                    res,
                    new_policy.is_storable().then(|| Box::new(new_policy)),
                ))
            }
        }
    }

    #[instrument(skip_all, fields(url = req.url().as_str()))]
    async fn fresh_request(&self, req: Request) -> Result<CachedResponse, Error> {
        trace!("{} {}", req.method(), req.url());
        let res = self
            .0
            .execute(req.try_clone().expect("streaming requests not supported"))
            .await
            .map_err(ErrorKind::RequestMiddlewareError)?
            .error_for_status()
            .map_err(ErrorKind::RequestError)?;
        let cache_policy = CachePolicyStub(Some(CachePolicy::new(
            &RequestLikeReqwest(&req),
            &ResponseLikeReqwest(&res),
        )));
        Ok(CachedResponse::ModifiedOrNew(
            res,
            cache_policy.is_storable().then(|| Box::new(cache_policy)),
        ))
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

#[derive(Debug)]
struct RequestLikeReqwest<'a>(&'a Request);

impl<'a> http_cache_semantics::RequestLike for RequestLikeReqwest<'a> {
    fn uri(&self) -> http::uri::Uri {
        // This converts from a url::Url (as returned by reqwest::Request::url)
        // to a http::uri::Uri. The conversion requires parsing, but this is
        // only called ~once per HTTP request. We can afford it.
        self.0
            .url()
            .as_str()
            .parse()
            .expect("reqwest::Request::url always returns a valid URL")
    }
    fn is_same_uri(&self, other: &http::uri::Uri) -> bool {
        // At time of writing, I saw no way to cheaply compare a http::uri::Uri
        // with a url::Url. We can at least avoid parsing anything, and
        // Url::as_str() is free. In practice though, this routine is called
        // ~once per HTTP request. We can afford it. (And it looks like
        // http::uri::Uri's PartialEq<str> implementation has been tuned.)
        self.0.url().as_str() == *other
    }
    fn method(&self) -> &http::method::Method {
        self.0.method()
    }
    fn headers(&self) -> &http::header::HeaderMap {
        self.0.headers()
    }
}

#[derive(Debug)]
struct ResponseLikeReqwest<'a>(&'a Response);

impl<'a> http_cache_semantics::ResponseLike for ResponseLikeReqwest<'a> {
    fn status(&self) -> http::status::StatusCode {
        self.0.status()
    }
    fn headers(&self) -> &http::header::HeaderMap {
        self.0.headers()
    }
}
