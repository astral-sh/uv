use http::Extensions;
use std::fmt::Debug;

use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use url::Url;

/// A custom error type for the offline middleware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfflineError {
    url: Url,
}

impl OfflineError {
    /// Returns the URL that caused the error.
    pub(crate) fn url(&self) -> &Url {
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
