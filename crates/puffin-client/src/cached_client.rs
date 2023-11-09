use std::future::Future;
use std::path::Path;
use std::time::SystemTime;

use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use reqwest::{Client, Request, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tracing::{trace, warn};
use url::Url;

use crate::error::Error;

#[derive(Debug)]
enum CachedResponse<Payload: Serialize> {
    /// The cached response is fresh without an HTTP request (e.g. immutable)
    FreshCache(Payload),
    /// The cached response is fresh after an HTTP request (e.g. 304 not modified)
    NotModified(DataWithCachePolicy<Payload>),
    /// There was no prior cached response or the cache was outdated
    ///
    /// The cache policy is `None` if it isn't storable
    ModifiedOrNew(Response, Option<CachePolicy>),
}

/// Serialize the actual payload together with its caching information
#[derive(Debug, Deserialize, Serialize)]
struct DataWithCachePolicy<Payload: Serialize> {
    data: Payload,
    cache_policy: CachePolicy,
}

/// Custom caching layer over [`reqwest::Client`] using `http-cache-semantics`.
///
/// This effective middleware takes inspiration from the `http-cache` crate, but unlike this crate,
/// we allow running an async callback on the response before caching. We use this to e.g. store a
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
/// that it's a file. TODO(konstin): Centralize the cache bucket management.
#[derive(Debug, Clone)]
pub(crate) struct CachedClient(Client);

impl CachedClient {
    pub(crate) fn new(client: Client) -> Self {
        Self(client)
    }

    /// Makes a cached request with a custom response transformation
    ///
    /// If a new response was received (no prior cached response or modified on the remote), the
    /// response through `response_callback` and only the result is cached and returned
    pub(crate) async fn get_transformed_cached<
        Payload: Serialize + DeserializeOwned,
        Callback,
        CallbackReturn,
    >(
        &self,
        url: Url,
        cache_dir: &Path,
        filename: &str,
        response_callback: Callback,
    ) -> Result<Payload, Error>
    where
        Callback: FnOnce(Response) -> CallbackReturn,
        CallbackReturn: Future<Output = Result<Payload, Error>>,
    {
        let cache_file = cache_dir.join(filename);
        let cached = if let Ok(cached) = fs_err::tokio::read(&cache_file).await {
            match serde_json::from_slice::<DataWithCachePolicy<Payload>>(&cached) {
                Ok(data) => Some(data),
                Err(err) => {
                    warn!(
                        "Broken cache entry at {}, removing: {err}",
                        cache_file.display()
                    );
                    let _ = fs_err::tokio::remove_file(&cache_file).await;
                    None
                }
            }
        } else {
            None
        };

        let req = self.0.get(url.clone()).build()?;
        let cached_response = self.send_cached(req, cached).await?;

        match cached_response {
            CachedResponse::FreshCache(data) => Ok(data),
            CachedResponse::NotModified(data_with_cache_policy) => {
                let temp_file = NamedTempFile::new_in(cache_dir)?;
                fs_err::tokio::write(&temp_file, &serde_json::to_vec(&data_with_cache_policy)?)
                    .await?;
                temp_file.persist(cache_file)?;
                Ok(data_with_cache_policy.data)
            }
            CachedResponse::ModifiedOrNew(res, cache_policy) => {
                let data = response_callback(res).await?;
                if let Some(cache_policy) = cache_policy {
                    let data_with_cache_policy = DataWithCachePolicy { data, cache_policy };
                    fs_err::tokio::create_dir_all(cache_dir).await?;
                    let temp_file = NamedTempFile::new_in(cache_dir)?;
                    fs_err::tokio::write(&temp_file, &serde_json::to_vec(&data_with_cache_policy)?)
                        .await?;
                    temp_file.persist(cache_file)?;
                    Ok(data_with_cache_policy.data)
                } else {
                    Ok(data)
                }
            }
        }
    }

    /// `http-cache-semantics` to `reqwest` wrapper
    async fn send_cached<T: Serialize + DeserializeOwned>(
        &self,
        mut req: Request,
        cached: Option<DataWithCachePolicy<T>>,
    ) -> Result<CachedResponse<T>, Error> {
        // The converted types are from the specific `reqwest` types to the more generic `http`
        // types
        let mut converted_req = http::Request::try_from(
            req.try_clone()
                .expect("You can't use streaming request bodies with this function"),
        )?;
        let cached_response = if let Some(cached) = cached {
            match cached
                .cache_policy
                .before_request(&converted_req, SystemTime::now())
            {
                BeforeRequest::Fresh(_) => {
                    trace!("Fresh cache for {}", req.url());
                    CachedResponse::FreshCache(cached.data)
                }
                BeforeRequest::Stale { request, matches } => {
                    if !matches {
                        // This should not happen
                        warn!(
                            "Cached request doesn't match current request for {}",
                            req.url()
                        );
                        // This will override the bogus cache
                        return self.fresh_request(req, converted_req).await;
                    }
                    trace!("Revalidation request for {}", req.url());
                    for header in &request.headers {
                        req.headers_mut().insert(header.0.clone(), header.1.clone());
                        converted_req
                            .headers_mut()
                            .insert(header.0.clone(), header.1.clone());
                    }
                    let res = self.0.execute(req).await?.error_for_status()?;
                    let mut converted_res = http::Response::new(());
                    *converted_res.status_mut() = res.status();
                    for header in res.headers() {
                        converted_res.headers_mut().insert(
                            http::HeaderName::from(header.0),
                            http::HeaderValue::from(header.1),
                        );
                    }
                    let after_response = cached.cache_policy.after_response(
                        &converted_req,
                        &converted_res,
                        SystemTime::now(),
                    );
                    match after_response {
                        AfterResponse::NotModified(new_policy, _parts) => {
                            CachedResponse::NotModified(DataWithCachePolicy {
                                data: cached.data,
                                cache_policy: new_policy,
                            })
                        }
                        AfterResponse::Modified(new_policy, _parts) => {
                            CachedResponse::ModifiedOrNew(
                                res,
                                new_policy.is_storable().then_some(new_policy),
                            )
                        }
                    }
                }
            }
        } else {
            // No reusable cache
            self.fresh_request(req, converted_req).await?
        };
        Ok(cached_response)
    }

    async fn fresh_request<T: Serialize>(
        &self,
        req: Request,
        converted_req: http::Request<reqwest::Body>,
    ) -> Result<CachedResponse<T>, Error> {
        trace!("{} {}", req.method(), req.url());
        let res = self.0.execute(req).await?.error_for_status()?;
        let mut converted_res = http::Response::new(());
        *converted_res.status_mut() = res.status();
        for header in res.headers() {
            converted_res.headers_mut().insert(
                http::HeaderName::from(header.0),
                http::HeaderValue::from(header.1),
            );
        }
        let cache_policy =
            CachePolicy::new(&converted_req.into_parts().0, &converted_res.into_parts().0);
        Ok(CachedResponse::ModifiedOrNew(
            res,
            cache_policy.is_storable().then_some(cache_policy),
        ))
    }
}
