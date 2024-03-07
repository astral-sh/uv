use std::fmt::Debug;

use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, RequestBuilder, RequestInitialiser};
use task_local_extensions::Extensions;
use url::Url;
use uv_keyring::get_keyring_auth;

/// A custom error type for the offline middleware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfflineError {
    url: Url,
}

impl OfflineError {
    /// Returns the URL that caused the error.
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl std::fmt::Display for OfflineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Network connectivity is disabled, but the requested data wasn't found in the cache for: `{}`", self.url)
    }
}

impl std::error::Error for OfflineError {}

/// A middleware that always returns an error indicating that the client is offline.
pub(crate) struct OfflineMiddleware;

#[async_trait::async_trait]
impl Middleware for OfflineMiddleware {
    async fn handle(
        &self,
        req: Request,
        _extensions: &mut Extensions,
        _next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        Err(reqwest_middleware::Error::Middleware(
            OfflineError {
                url: req.url().clone(),
            }
            .into(),
        ))
    }
}

pub struct KeyringMiddleware;

impl RequestInitialiser for KeyringMiddleware {
    fn init(&self, req: RequestBuilder) -> RequestBuilder {
        match req.try_clone() {
            Some(nr) => req
                .try_clone()
                .unwrap()
                .build()
                .ok()
                .map(|r| {
                    let auth = get_keyring_auth(r.url()).unwrap_or_else(|e| panic!("{}", e));
                    nr.basic_auth(auth.username, Some(auth.password))
                })
                .unwrap_or(req),
            None => req,
        }
    }
}
