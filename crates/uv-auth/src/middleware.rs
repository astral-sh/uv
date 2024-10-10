use std::sync::Arc;

use http::{Extensions, StatusCode};
use url::Url;

use crate::{
    credentials::{Credentials, Username},
    realm::Realm,
    CredentialsCache, KeyringProvider, CREDENTIALS_CACHE,
};
use anyhow::{anyhow, format_err};
use netrc::Netrc;
use reqwest::{Request, Response};
use reqwest_middleware::{Error, Middleware, Next};
use tracing::{debug, trace};

/// A middleware that adds basic authentication to requests.
///
/// Uses a cache to propagate credentials from previously seen requests and
/// fetches credentials from a netrc file and the keyring.
pub struct AuthMiddleware {
    netrc: Option<Netrc>,
    keyring: Option<KeyringProvider>,
    cache: Option<CredentialsCache>,
    /// We know that the endpoint needs authentication, so we don't try to send an unauthenticated
    /// request, avoiding cloning an uncloneable request.
    only_authenticated: bool,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            netrc: Netrc::new().ok(),
            keyring: None,
            cache: None,
            only_authenticated: false,
        }
    }

    /// Configure the [`Netrc`] credential file to use.
    ///
    /// `None` disables authentication via netrc.
    #[must_use]
    pub fn with_netrc(mut self, netrc: Option<Netrc>) -> Self {
        self.netrc = netrc;
        self
    }

    /// Configure the [`KeyringProvider`] to use.
    #[must_use]
    pub fn with_keyring(mut self, keyring: Option<KeyringProvider>) -> Self {
        self.keyring = keyring;
        self
    }

    /// Configure the [`CredentialsCache`] to use.
    #[must_use]
    pub fn with_cache(mut self, cache: CredentialsCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// We know that the endpoint needs authentication, so we don't try to send an unauthenticated
    /// request, avoiding cloning an uncloneable request.
    #[must_use]
    pub fn with_only_authenticated(mut self, only_authenticated: bool) -> Self {
        self.only_authenticated = only_authenticated;
        self
    }

    /// Get the configured authentication store.
    ///
    /// If not set, the global store is used.
    fn cache(&self) -> &CredentialsCache {
        self.cache.as_ref().unwrap_or(&CREDENTIALS_CACHE)
    }
}

impl Default for AuthMiddleware {
    fn default() -> Self {
        AuthMiddleware::new()
    }
}

#[async_trait::async_trait]
impl Middleware for AuthMiddleware {
    /// Handle authentication for a request.
    ///
    /// ## If the request has a username and password
    ///
    /// We already have a fully authenticated request and we don't need to perform a look-up.
    ///
    /// - Perform the request
    /// - Add the username and password to the cache if successful
    ///
    /// ## If the request only has a username
    ///
    /// We probably need additional authentication, because a username is provided.
    /// We'll avoid making a request we expect to fail and look for a password.
    /// The discovered credentials must have the requested username to be used.
    ///
    /// - Check the cache (realm key) for a password
    /// - Check the netrc for a password
    /// - Check the keyring for a password
    /// - Perform the request
    /// - Add the username and password to the cache if successful
    ///
    /// ## If the request has no authentication
    ///
    /// We may or may not need authentication. We'll check for cached credentials for the URL,
    /// which is relatively specific and can save us an expensive failed request. Otherwise,
    /// we'll make the request and look for less-specific credentials on failure i.e. if the
    /// server tells us authorization is needed. This pattern avoids attaching credentials to
    /// requests that do not need them, which can cause some servers to deny the request.
    ///
    /// - Check the cache (url key)
    /// - Perform the request
    /// - On 401, 403, or 404 check for authentication if there was a cache miss
    ///     - Check the cache (realm key) for the username and password
    ///     - Check the netrc for a username and password
    ///     - Perform the request again if found
    ///     - Add the username and password to the cache if successful
    async fn handle(
        &self,
        mut request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        // Check for credentials attached to the request already
        let credentials = Credentials::from_request(&request);

        // In the middleware, existing credentials are already moved from the URL
        // to the headers so for display purposes we restore some information
        let url = if tracing::enabled!(tracing::Level::DEBUG) {
            let mut url = request.url().clone();
            if let Some(username) = credentials
                .as_ref()
                .and_then(|credentials| credentials.username())
            {
                let _ = url.set_username(username);
            };
            if credentials
                .as_ref()
                .and_then(|credentials| credentials.password())
                .is_some()
            {
                let _ = url.set_password(Some("****"));
            };
            url.to_string()
        } else {
            request.url().to_string()
        };
        trace!("Handling request for {url}");

        if let Some(credentials) = credentials {
            let credentials = Arc::new(credentials);

            // If there's a password, send the request and cache
            if credentials.password().is_some() {
                trace!("Request for {url} is already fully authenticated");
                return self
                    .complete_request(Some(credentials), request, extensions, next)
                    .await;
            }

            trace!("Request for {url} is missing a password, looking for credentials");
            // There's just a username, try to find a password
            let credentials = if let Some(credentials) = self
                .cache()
                .get_realm(Realm::from(request.url()), credentials.to_username())
            {
                request = credentials.authenticate(request);
                // Do not insert already-cached credentials
                None
            } else if let Some(credentials) = self
                .cache()
                .get_url(request.url(), &credentials.to_username())
            {
                request = credentials.authenticate(request);
                // Do not insert already-cached credentials
                None
            } else if let Some(credentials) = self
                .fetch_credentials(Some(&credentials), request.url())
                .await
            {
                request = credentials.authenticate(request);
                Some(credentials)
            } else {
                // If we don't find a password, we'll still attempt the request with the existing credentials
                Some(credentials)
            };

            return self
                .complete_request(credentials, request, extensions, next)
                .await;
        }

        // We have no credentials
        trace!("Request for {url} is unauthenticated, checking cache");

        // Check the cache for a URL match
        let credentials = self.cache().get_url(request.url(), &Username::none());
        if let Some(credentials) = credentials.as_ref() {
            request = credentials.authenticate(request);
            if credentials.password().is_some() {
                return self.complete_request(None, request, extensions, next).await;
            }
        }
        let attempt_has_username = credentials
            .as_ref()
            .is_some_and(|credentials| credentials.username().is_some());

        let (mut retry_request, response) = if self.only_authenticated {
            // For endpoints where we require the user to provide credentials, we don't try the
            // unauthenticated request first.
            trace!("Checking for credentials for {url}");
            (request, None)
        } else {
            // Otherwise, attempt an anonymous request
            trace!("Attempting unauthenticated request for {url}");

            // <https://github.com/TrueLayer/reqwest-middleware/blob/abdf1844c37092d323683c2396b7eefda1418d3c/reqwest-retry/src/middleware.rs#L141-L149>
            // Clone the request so we can retry it on authentication failure
            let retry_request = request.try_clone().ok_or_else(|| {
                Error::Middleware(anyhow!(
                    "Request object is not cloneable. Are you passing a streaming body?"
                        .to_string()
                ))
            })?;

            let response = next.clone().run(request, extensions).await?;

            // If we don't fail with authorization related codes, return the response
            if !matches!(
                response.status(),
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED
            ) {
                return Ok(response);
            }

            // Otherwise, search for credentials
            trace!(
                "Request for {url} failed with {}, checking for credentials",
                response.status()
            );

            (retry_request, Some(response))
        };

        // Check in the cache first
        let credentials = self.cache().get_realm(
            Realm::from(retry_request.url()),
            credentials
                .map(|credentials| credentials.to_username())
                .unwrap_or(Username::none()),
        );
        if let Some(credentials) = credentials.as_ref() {
            if credentials.password().is_some() {
                trace!("Retrying request for {url} with credentials from cache {credentials:?}");
                retry_request = credentials.authenticate(retry_request);
                return self
                    .complete_request(None, retry_request, extensions, next)
                    .await;
            }
        }

        // Then, fetch from external services.
        // Here we use the username from the cache if present.
        if let Some(credentials) = self
            .fetch_credentials(credentials.as_deref(), retry_request.url())
            .await
        {
            retry_request = credentials.authenticate(retry_request);
            trace!("Retrying request for {url} with {credentials:?}");
            return self
                .complete_request(Some(credentials), retry_request, extensions, next)
                .await;
        }

        if let Some(credentials) = credentials.as_ref() {
            if !attempt_has_username {
                trace!("Retrying request for {url} with username from cache {credentials:?}");
                retry_request = credentials.authenticate(retry_request);
                return self
                    .complete_request(None, retry_request, extensions, next)
                    .await;
            }
        }

        if let Some(response) = response {
            Ok(response)
        } else {
            Err(Error::Middleware(format_err!(
                "Missing credentials for {url}"
            )))
        }
    }
}

impl AuthMiddleware {
    /// Run a request to completion.
    ///
    /// If credentials are present, insert them into the cache on success.
    async fn complete_request(
        &self,
        credentials: Option<Arc<Credentials>>,
        request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let Some(credentials) = credentials else {
            // Nothing to insert into the cache if we don't have credentials
            return next.run(request, extensions).await;
        };

        let url = request.url().clone();
        let result = next.run(request, extensions).await;

        // Update the cache with new credentials on a successful request
        if result
            .as_ref()
            .is_ok_and(|response| response.error_for_status_ref().is_ok())
        {
            trace!("Updating cached credentials for {url} to {credentials:?}");
            self.cache().insert(&url, credentials);
        };

        result
    }

    /// Fetch credentials for a URL.
    ///
    /// Supports netrc file and keyring lookups.
    async fn fetch_credentials(
        &self,
        credentials: Option<&Credentials>,
        url: &Url,
    ) -> Option<Arc<Credentials>> {
        // Fetches can be expensive, so we will only run them _once_ per realm and username combination
        // All other requests for the same realm will wait until the first one completes
        let key = (
            Realm::from(url),
            Username::from(
                credentials
                    .map(|credentials| credentials.username().unwrap_or_default().to_string()),
            ),
        );

        if !self.cache().fetches.register(key.clone()) {
            let credentials = self
                .cache()
                .fetches
                .wait(&key)
                .await
                .expect("The key must exist after register is called");

            if credentials.is_some() {
                trace!("Using credentials from previous fetch for {url}");
            } else {
                trace!("Skipping fetch of credentials for {url}, previous attempt failed");
            };

            return credentials;
        }

        // Netrc support based on: <https://github.com/gribouille/netrc>.
        let credentials = if let Some(credentials) = self.netrc.as_ref().and_then(|netrc| {
            debug!("Checking netrc for credentials for {url}");
            Credentials::from_netrc(
                netrc,
                url,
                credentials
                    .as_ref()
                    .and_then(|credentials| credentials.username()),
            )
        }) {
            debug!("Found credentials in netrc file for {url}");
            Some(credentials)
        // N.B. The keyring provider performs lookups for the exact URL then
        //      falls back to the host, but we cache the result per realm so if a keyring
        //      implementation returns different credentials for different URLs in the
        //      same realm we will use the wrong credentials.
        } else if let Some(credentials) = match self.keyring {
            Some(ref keyring) => {
                if let Some(username) = credentials.and_then(|credentials| credentials.username()) {
                    debug!("Checking keyring for credentials for {username}@{url}");
                    keyring.fetch(url, username).await
                } else {
                    debug!("Skipping keyring lookup for {url} with no username");
                    None
                }
            }
            None => None,
        } {
            debug!("Found credentials in keyring for {url}");
            Some(credentials)
        } else {
            None
        }
        .map(Arc::new);

        // Register the fetch for this key
        self.cache().fetches.done(key.clone(), credentials.clone());

        credentials
    }
}

#[cfg(test)]
mod tests;
