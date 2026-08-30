use http::Extensions;
use std::fmt::Debug;
use uv_auth::AzureEndpointProvider;
use uv_preview::Preview;
use uv_redacted::DisplaySafeUrl;

use reqwest::header::HeaderValue;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};

const AZURE_STORAGE_VERSION: &str = "2023-11-03";

pub(crate) struct AzureStorageMiddleware {
    pub(crate) preview: Preview,
}

#[async_trait::async_trait]
impl Middleware for AzureStorageMiddleware {
    async fn handle(
        &self,
        mut request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        if AzureEndpointProvider::is_azure_endpoint(request.url(), self.preview)? {
            request
                .headers_mut()
                .entry("x-ms-version")
                .or_insert(HeaderValue::from_static(AZURE_STORAGE_VERSION));
        }

        next.run(request, extensions).await
    }
}

/// A custom error type for the offline middleware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OfflineError {
    url: DisplaySafeUrl,
}

impl OfflineError {
    /// Returns the URL that caused the error.
    pub(crate) fn url(&self) -> &DisplaySafeUrl {
        &self.url
    }
}

impl std::fmt::Display for OfflineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Network connectivity is disabled, but the requested data wasn't found in the cache for: `{}`",
            self.url
        )
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
                url: DisplaySafeUrl::from_url(req.url().clone()),
            }
            .into(),
        ))
    }
}
