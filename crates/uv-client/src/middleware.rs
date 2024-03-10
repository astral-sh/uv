use std::fmt::Debug;

use http::HeaderValue;
use netrc::{Netrc, Result};
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use task_local_extensions::Extensions;
use url::Url;

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

/// A middleware with support for netrc files.
///
/// Based on: <https://github.com/gribouille/netrc>.
pub(crate) struct NetrcMiddleware {
    nrc: Netrc,
}

impl NetrcMiddleware {
    pub(crate) fn new() -> Result<Self> {
        Netrc::new().map(|nrc| NetrcMiddleware { nrc })
    }
}

#[async_trait::async_trait]
impl Middleware for NetrcMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        _extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        // If the request already has an authorization header, we don't need to do anything.
        // This gives in-URL credentials precedence over the netrc file.
        if req.headers().contains_key(reqwest::header::AUTHORIZATION) {
            return next.run(req, _extensions).await;
        }

        if let Some(auth) = req.url().host_str().and_then(|host| {
            self.nrc
                .hosts
                .get(host)
                .or_else(|| self.nrc.hosts.get("default"))
        }) {
            req.headers_mut().insert(
                reqwest::header::AUTHORIZATION,
                basic_auth(
                    &auth.login,
                    if auth.password.is_empty() {
                        None
                    } else {
                        Some(&auth.password)
                    },
                ),
            );
        }
        next.run(req, _extensions).await
    }
}

/// Create a `HeaderValue` for basic authentication.
///
/// Source: <https://github.com/seanmonstar/reqwest/blob/2c11ef000b151c2eebeed2c18a7b81042220c6b0/src/util.rs#L3>
fn basic_auth<U, P>(username: U, password: Option<P>) -> HeaderValue
where
    U: std::fmt::Display,
    P: std::fmt::Display,
{
    use base64::prelude::BASE64_STANDARD;
    use base64::write::EncoderWriter;
    use std::io::Write;

    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        let _ = write!(encoder, "{}:", username);
        if let Some(password) = password {
            let _ = write!(encoder, "{}", password);
        }
    }
    let mut header = HeaderValue::from_bytes(&buf).expect("base64 is always valid HeaderValue");
    header.set_sensitive(true);
    header
}
