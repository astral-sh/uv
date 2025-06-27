use std::error::Error;
use std::fmt::Debug;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::{env, io, iter};

use anyhow::anyhow;
use http::{
    HeaderMap, HeaderName, HeaderValue, Method, StatusCode,
    header::{
        AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION,
        PROXY_AUTHORIZATION, REFERER, TRANSFER_ENCODING, WWW_AUTHENTICATE,
    },
};
use itertools::Itertools;
use reqwest::{Client, ClientBuilder, IntoUrl, Proxy, Request, Response, multipart};
use reqwest_middleware::{ClientWithMiddleware, Middleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    DefaultRetryableStrategy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use tracing::{debug, trace};
use url::ParseError;
use url::Url;

use uv_auth::Credentials;
use uv_auth::{AuthMiddleware, Indexes};
use uv_configuration::{KeyringProviderType, TrustedHost};
use uv_fs::Simplified;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::Connectivity;
use crate::linehaul::LineHaul;
use crate::middleware::OfflineMiddleware;
use crate::tls::read_identity;

pub const DEFAULT_RETRIES: u32 = 3;
/// Maximum number of redirects to follow before giving up.
///
/// This is the default used by [`reqwest`].
const DEFAULT_MAX_REDIRECTS: u32 = 10;

/// Selectively skip parts or the entire auth middleware.
#[derive(Debug, Clone, Copy, Default)]
pub enum AuthIntegration {
    /// Run the full auth middleware, including sending an unauthenticated request first.
    #[default]
    Default,
    /// Send only an authenticated request without cloning and sending an unauthenticated request
    /// first. Errors if no credentials were found.
    OnlyAuthenticated,
    /// Skip the auth middleware entirely. The caller is responsible for managing authentication.
    NoAuthMiddleware,
}

/// A builder for an [`BaseClient`].
#[derive(Debug, Clone)]
pub struct BaseClientBuilder<'a> {
    keyring: KeyringProviderType,
    allow_insecure_host: Vec<TrustedHost>,
    native_tls: bool,
    retries: u32,
    pub connectivity: Connectivity,
    markers: Option<&'a MarkerEnvironment>,
    platform: Option<&'a Platform>,
    auth_integration: AuthIntegration,
    indexes: Indexes,
    default_timeout: Duration,
    extra_middleware: Option<ExtraMiddleware>,
    proxies: Vec<Proxy>,
    redirect_policy: RedirectPolicy,
    /// Whether credentials should be propagated during cross-origin redirects.
    ///
    /// A policy allowing propagation is insecure and should only be available for test code.
    cross_origin_credential_policy: CrossOriginCredentialsPolicy,
}

/// The policy for handling HTTP redirects.
#[derive(Debug, Default, Clone, Copy)]
pub enum RedirectPolicy {
    /// Use reqwest's built-in redirect handling. This bypasses our custom middleware
    /// on redirect.
    #[default]
    BypassMiddleware,
    /// Handle redirects manually, re-triggering our custom middleware for each request.
    RetriggerMiddleware,
}

impl RedirectPolicy {
    pub fn reqwest_policy(self) -> reqwest::redirect::Policy {
        match self {
            RedirectPolicy::BypassMiddleware => reqwest::redirect::Policy::default(),
            RedirectPolicy::RetriggerMiddleware => reqwest::redirect::Policy::none(),
        }
    }
}

/// A list of user-defined middlewares to be applied to the client.
#[derive(Clone)]
pub struct ExtraMiddleware(pub Vec<Arc<dyn Middleware>>);

impl Debug for ExtraMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtraMiddleware")
            .field("0", &format!("{} middlewares", self.0.len()))
            .finish()
    }
}

impl Default for BaseClientBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseClientBuilder<'_> {
    pub fn new() -> Self {
        Self {
            keyring: KeyringProviderType::default(),
            allow_insecure_host: vec![],
            native_tls: false,
            connectivity: Connectivity::Online,
            retries: DEFAULT_RETRIES,
            markers: None,
            platform: None,
            auth_integration: AuthIntegration::default(),
            indexes: Indexes::new(),
            default_timeout: Duration::from_secs(30),
            extra_middleware: None,
            proxies: vec![],
            redirect_policy: RedirectPolicy::default(),
            cross_origin_credential_policy: CrossOriginCredentialsPolicy::Secure,
        }
    }
}

impl<'a> BaseClientBuilder<'a> {
    #[must_use]
    pub fn keyring(mut self, keyring_type: KeyringProviderType) -> Self {
        self.keyring = keyring_type;
        self
    }

    #[must_use]
    pub fn allow_insecure_host(mut self, allow_insecure_host: Vec<TrustedHost>) -> Self {
        self.allow_insecure_host = allow_insecure_host;
        self
    }

    #[must_use]
    pub fn connectivity(mut self, connectivity: Connectivity) -> Self {
        self.connectivity = connectivity;
        self
    }

    #[must_use]
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    #[must_use]
    pub fn native_tls(mut self, native_tls: bool) -> Self {
        self.native_tls = native_tls;
        self
    }

    #[must_use]
    pub fn markers(mut self, markers: &'a MarkerEnvironment) -> Self {
        self.markers = Some(markers);
        self
    }

    #[must_use]
    pub fn platform(mut self, platform: &'a Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    #[must_use]
    pub fn auth_integration(mut self, auth_integration: AuthIntegration) -> Self {
        self.auth_integration = auth_integration;
        self
    }

    #[must_use]
    pub fn indexes(mut self, indexes: Indexes) -> Self {
        self.indexes = indexes;
        self
    }

    #[must_use]
    pub fn default_timeout(mut self, default_timeout: Duration) -> Self {
        self.default_timeout = default_timeout;
        self
    }

    #[must_use]
    pub fn extra_middleware(mut self, middleware: ExtraMiddleware) -> Self {
        self.extra_middleware = Some(middleware);
        self
    }

    #[must_use]
    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.proxies.push(proxy);
        self
    }

    #[must_use]
    pub fn redirect(mut self, policy: RedirectPolicy) -> Self {
        self.redirect_policy = policy;
        self
    }

    /// Allows credentials to be propagated on cross-origin redirects.
    ///
    /// WARNING: This should only be available for tests. In production code, propagating credentials
    /// during cross-origin redirects can lead to security vulnerabilities including credential
    /// leakage to untrusted domains.
    #[cfg(test)]
    #[must_use]
    pub fn allow_cross_origin_credentials(mut self) -> Self {
        self.cross_origin_credential_policy = CrossOriginCredentialsPolicy::Insecure;
        self
    }

    pub fn is_offline(&self) -> bool {
        matches!(self.connectivity, Connectivity::Offline)
    }

    /// Create a [`RetryPolicy`] for the client.
    fn retry_policy(&self) -> ExponentialBackoff {
        ExponentialBackoff::builder().build_with_max_retries(self.retries)
    }

    pub fn build(&self) -> BaseClient {
        // Create user agent.
        let mut user_agent_string = format!("uv/{}", version());

        // Add linehaul metadata.
        if let Some(markers) = self.markers {
            let linehaul = LineHaul::new(markers, self.platform);
            if let Ok(output) = serde_json::to_string(&linehaul) {
                let _ = write!(user_agent_string, " {output}");
            }
        }

        // Check for the presence of an `SSL_CERT_FILE`.
        let ssl_cert_file_exists = env::var_os(EnvVars::SSL_CERT_FILE).is_some_and(|path| {
            let path_exists = Path::new(&path).exists();
            if !path_exists {
                warn_user_once!(
                    "Ignoring invalid `SSL_CERT_FILE`. File does not exist: {}.",
                    path.simplified_display().cyan()
                );
            }
            path_exists
        });

        // Timeout options, matching https://doc.rust-lang.org/nightly/cargo/reference/config.html#httptimeout
        // `UV_REQUEST_TIMEOUT` is provided for backwards compatibility with v0.1.6
        let timeout = env::var(EnvVars::UV_HTTP_TIMEOUT)
            .or_else(|_| env::var(EnvVars::UV_REQUEST_TIMEOUT))
            .or_else(|_| env::var(EnvVars::HTTP_TIMEOUT))
            .and_then(|value| {
                value.parse::<u64>()
                    .map(Duration::from_secs)
                    .or_else(|_| {
                        // On parse error, warn and use the default timeout
                        warn_user_once!("Ignoring invalid value from environment for `UV_HTTP_TIMEOUT`. Expected an integer number of seconds, got \"{value}\".");
                        Ok(self.default_timeout)
                    })
            })
            .unwrap_or(self.default_timeout);
        debug!("Using request timeout of {}s", timeout.as_secs());

        // Create a secure client that validates certificates.
        let raw_client = self.create_client(
            &user_agent_string,
            timeout,
            ssl_cert_file_exists,
            Security::Secure,
            self.redirect_policy,
        );

        // Create an insecure client that accepts invalid certificates.
        let raw_dangerous_client = self.create_client(
            &user_agent_string,
            timeout,
            ssl_cert_file_exists,
            Security::Insecure,
            self.redirect_policy,
        );

        // Wrap in any relevant middleware and handle connectivity.
        let client = RedirectClientWithMiddleware {
            client: self.apply_middleware(raw_client.clone()),
            redirect_policy: self.redirect_policy,
            cross_origin_credentials_policy: self.cross_origin_credential_policy,
        };
        let dangerous_client = RedirectClientWithMiddleware {
            client: self.apply_middleware(raw_dangerous_client.clone()),
            redirect_policy: self.redirect_policy,
            cross_origin_credentials_policy: self.cross_origin_credential_policy,
        };

        BaseClient {
            connectivity: self.connectivity,
            allow_insecure_host: self.allow_insecure_host.clone(),
            retries: self.retries,
            client,
            raw_client,
            dangerous_client,
            raw_dangerous_client,
            timeout,
        }
    }

    /// Share the underlying client between two different middleware configurations.
    pub fn wrap_existing(&self, existing: &BaseClient) -> BaseClient {
        // Wrap in any relevant middleware and handle connectivity.
        let client = RedirectClientWithMiddleware {
            client: self.apply_middleware(existing.raw_client.clone()),
            redirect_policy: self.redirect_policy,
            cross_origin_credentials_policy: self.cross_origin_credential_policy,
        };
        let dangerous_client = RedirectClientWithMiddleware {
            client: self.apply_middleware(existing.raw_dangerous_client.clone()),
            redirect_policy: self.redirect_policy,
            cross_origin_credentials_policy: self.cross_origin_credential_policy,
        };

        BaseClient {
            connectivity: self.connectivity,
            allow_insecure_host: self.allow_insecure_host.clone(),
            retries: self.retries,
            client,
            dangerous_client,
            raw_client: existing.raw_client.clone(),
            raw_dangerous_client: existing.raw_dangerous_client.clone(),
            timeout: existing.timeout,
        }
    }

    fn create_client(
        &self,
        user_agent: &str,
        timeout: Duration,
        ssl_cert_file_exists: bool,
        security: Security,
        redirect_policy: RedirectPolicy,
    ) -> Client {
        // Configure the builder.
        let client_builder = ClientBuilder::new()
            .http1_title_case_headers()
            .user_agent(user_agent)
            .pool_max_idle_per_host(20)
            .read_timeout(timeout)
            .tls_built_in_root_certs(false)
            .redirect(redirect_policy.reqwest_policy());

        // If necessary, accept invalid certificates.
        let client_builder = match security {
            Security::Secure => client_builder,
            Security::Insecure => client_builder.danger_accept_invalid_certs(true),
        };

        let client_builder = if self.native_tls || ssl_cert_file_exists {
            client_builder.tls_built_in_native_certs(true)
        } else {
            client_builder.tls_built_in_webpki_certs(true)
        };

        // Configure mTLS.
        let client_builder = if let Some(ssl_client_cert) = env::var_os(EnvVars::SSL_CLIENT_CERT) {
            match read_identity(&ssl_client_cert) {
                Ok(identity) => client_builder.identity(identity),
                Err(err) => {
                    warn_user_once!("Ignoring invalid `SSL_CLIENT_CERT`: {err}");
                    client_builder
                }
            }
        } else {
            client_builder
        };

        // apply proxies
        let mut client_builder = client_builder;
        for p in &self.proxies {
            client_builder = client_builder.proxy(p.clone());
        }
        let client_builder = client_builder;

        client_builder
            .build()
            .expect("Failed to build HTTP client.")
    }

    fn apply_middleware(&self, client: Client) -> ClientWithMiddleware {
        match self.connectivity {
            Connectivity::Online => {
                let mut client = reqwest_middleware::ClientBuilder::new(client);

                // Avoid uncloneable errors with a streaming body during publish.
                if self.retries > 0 {
                    // Initialize the retry strategy.
                    let retry_strategy = RetryTransientMiddleware::new_with_policy_and_strategy(
                        self.retry_policy(),
                        UvRetryableStrategy,
                    );
                    client = client.with(retry_strategy);
                }

                // Initialize the authentication middleware to set headers.
                match self.auth_integration {
                    AuthIntegration::Default => {
                        let auth_middleware = AuthMiddleware::new()
                            .with_indexes(self.indexes.clone())
                            .with_keyring(self.keyring.to_provider());
                        client = client.with(auth_middleware);
                    }
                    AuthIntegration::OnlyAuthenticated => {
                        let auth_middleware = AuthMiddleware::new()
                            .with_indexes(self.indexes.clone())
                            .with_keyring(self.keyring.to_provider())
                            .with_only_authenticated(true);

                        client = client.with(auth_middleware);
                    }
                    AuthIntegration::NoAuthMiddleware => {
                        // The downstream code uses custom auth logic.
                    }
                }

                // When supplied add the extra middleware
                if let Some(extra_middleware) = &self.extra_middleware {
                    for middleware in &extra_middleware.0 {
                        client = client.with_arc(middleware.clone());
                    }
                }

                client.build()
            }
            Connectivity::Offline => reqwest_middleware::ClientBuilder::new(client)
                .with(OfflineMiddleware)
                .build(),
        }
    }
}

/// A base client for HTTP requests
#[derive(Debug, Clone)]
pub struct BaseClient {
    /// The underlying HTTP client that enforces valid certificates.
    client: RedirectClientWithMiddleware,
    /// The underlying HTTP client that accepts invalid certificates.
    dangerous_client: RedirectClientWithMiddleware,
    /// The HTTP client without middleware.
    raw_client: Client,
    /// The HTTP client that accepts invalid certificates without middleware.
    raw_dangerous_client: Client,
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: Duration,
    /// Hosts that are trusted to use the insecure client.
    allow_insecure_host: Vec<TrustedHost>,
    /// The number of retries to attempt on transient errors.
    retries: u32,
}

#[derive(Debug, Clone, Copy)]
enum Security {
    /// The client should use secure settings, i.e., valid certificates.
    Secure,
    /// The client should use insecure settings, i.e., skip certificate validation.
    Insecure,
}

impl BaseClient {
    /// Selects the appropriate client based on the host's trustworthiness.
    pub fn for_host(&self, url: &DisplaySafeUrl) -> &RedirectClientWithMiddleware {
        if self.disable_ssl(url) {
            &self.dangerous_client
        } else {
            &self.client
        }
    }

    /// Executes a request, applying redirect policy.
    pub async fn execute(&self, req: Request) -> reqwest_middleware::Result<Response> {
        let client = self.for_host(&DisplaySafeUrl::from(req.url().clone()));
        client.execute(req).await
    }

    /// Returns `true` if the host is trusted to use the insecure client.
    pub fn disable_ssl(&self, url: &DisplaySafeUrl) -> bool {
        self.allow_insecure_host
            .iter()
            .any(|allow_insecure_host| allow_insecure_host.matches(url))
    }

    /// The configured client timeout, in seconds.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The configured connectivity mode.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// The [`RetryPolicy`] for the client.
    pub fn retry_policy(&self) -> ExponentialBackoff {
        ExponentialBackoff::builder().build_with_max_retries(self.retries)
    }
}

/// Wrapper around [`ClientWithMiddleware`] that manages redirects.
#[derive(Debug, Clone)]
pub struct RedirectClientWithMiddleware {
    client: ClientWithMiddleware,
    redirect_policy: RedirectPolicy,
    /// Whether credentials should be preserved during cross-origin redirects.
    ///
    /// WARNING: This should only be available for tests. In production code, preserving credentials
    /// during cross-origin redirects can lead to security vulnerabilities including credential
    /// leakage to untrusted domains.
    cross_origin_credentials_policy: CrossOriginCredentialsPolicy,
}

impl RedirectClientWithMiddleware {
    /// Convenience method to make a `GET` request to a URL.
    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder::new(self.client.get(url), self)
    }

    /// Convenience method to make a `POST` request to a URL.
    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder::new(self.client.post(url), self)
    }

    /// Convenience method to make a `HEAD` request to a URL.
    pub fn head<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder::new(self.client.head(url), self)
    }

    /// Executes a request, applying the redirect policy.
    pub async fn execute(&self, req: Request) -> reqwest_middleware::Result<Response> {
        match self.redirect_policy {
            RedirectPolicy::BypassMiddleware => self.client.execute(req).await,
            RedirectPolicy::RetriggerMiddleware => self.execute_with_redirect_handling(req).await,
        }
    }

    /// Executes a request. If the response is a redirect (one of HTTP 301, 302, 303, 307, or 308), the
    /// request is executed again with the redirect location URL (up to a maximum number of
    /// redirects).
    ///
    /// Unlike the built-in reqwest redirect policies, this sends the redirect request through the
    /// entire middleware pipeline again.
    ///
    /// See RFC 7231 7.1.2 <https://www.rfc-editor.org/rfc/rfc7231#section-7.1.2> for details on
    /// redirect semantics.
    async fn execute_with_redirect_handling(
        &self,
        req: Request,
    ) -> reqwest_middleware::Result<Response> {
        let mut request = req;
        let mut redirects = 0;
        let max_redirects = DEFAULT_MAX_REDIRECTS;

        loop {
            let result = self
                .client
                .execute(request.try_clone().expect("HTTP request must be cloneable"))
                .await;
            let Ok(response) = result else {
                return result;
            };

            if redirects >= max_redirects {
                return Ok(response);
            }

            let Some(redirect_request) =
                request_into_redirect(request, &response, self.cross_origin_credentials_policy)?
            else {
                return Ok(response);
            };

            redirects += 1;
            request = redirect_request;
        }
    }

    pub fn raw_client(&self) -> &ClientWithMiddleware {
        &self.client
    }
}

impl From<RedirectClientWithMiddleware> for ClientWithMiddleware {
    fn from(item: RedirectClientWithMiddleware) -> ClientWithMiddleware {
        item.client
    }
}

/// Check if this is should be a redirect and, if so, return a new redirect request.
///
/// This implementation is based on the [`reqwest`] crate redirect implementation.
/// It takes ownership of the original [`Request`] and mutates it to create the new
/// redirect [`Request`].
fn request_into_redirect(
    mut req: Request,
    res: &Response,
    cross_origin_credentials_policy: CrossOriginCredentialsPolicy,
) -> reqwest_middleware::Result<Option<Request>> {
    let original_req_url = DisplaySafeUrl::from(req.url().clone());
    let status = res.status();
    let should_redirect = match status {
        StatusCode::MOVED_PERMANENTLY
        | StatusCode::FOUND
        | StatusCode::TEMPORARY_REDIRECT
        | StatusCode::PERMANENT_REDIRECT => true,
        StatusCode::SEE_OTHER => {
            // Per RFC 7231, HTTP 303 is intended for the user agent
            // to perform a GET or HEAD request to the redirect target.
            // Historically, some browsers also changed method from POST
            // to GET on 301 or 302, but this is not required by RFC 7231
            // and was not intended by the HTTP spec.
            *req.body_mut() = None;
            for header in &[
                TRANSFER_ENCODING,
                CONTENT_ENCODING,
                CONTENT_TYPE,
                CONTENT_LENGTH,
            ] {
                req.headers_mut().remove(header);
            }

            match *req.method() {
                Method::GET | Method::HEAD => {}
                _ => {
                    *req.method_mut() = Method::GET;
                }
            }
            true
        }
        _ => false,
    };
    if !should_redirect {
        return Ok(None);
    }

    let location = res
        .headers()
        .get(LOCATION)
        .ok_or(reqwest_middleware::Error::Middleware(anyhow!(
            "Server returned redirect (HTTP {status}) without destination URL. This may indicate a server configuration issue"
        )))?
        .to_str()
        .map_err(|_| {
            reqwest_middleware::Error::Middleware(anyhow!(
                "Invalid HTTP {status} 'Location' value: must only contain visible ascii characters"
            ))
        })?;

    let mut redirect_url = match DisplaySafeUrl::parse(location) {
        Ok(url) => url,
        // Per RFC 7231, URLs should be resolved against the request URL.
        Err(ParseError::RelativeUrlWithoutBase) => original_req_url.join(location).map_err(|err| {
            reqwest_middleware::Error::Middleware(anyhow!(
                "Invalid HTTP {status} 'Location' value `{location}` relative to `{original_req_url}`: {err}"
            ))
        })?,
        Err(err) => {
            return Err(reqwest_middleware::Error::Middleware(anyhow!(
                "Invalid HTTP {status} 'Location' value `{location}`: {err}"
            )));
        }
    };
    // Per RFC 7231, fragments must be propagated
    if let Some(fragment) = original_req_url.fragment() {
        redirect_url.set_fragment(Some(fragment));
    }

    // Ensure the URL is a valid HTTP URI.
    if let Err(err) = redirect_url.as_str().parse::<http::Uri>() {
        return Err(reqwest_middleware::Error::Middleware(anyhow!(
            "HTTP {status} 'Location' value `{redirect_url}` is not a valid HTTP URI: {err}"
        )));
    }

    if redirect_url.scheme() != "http" && redirect_url.scheme() != "https" {
        return Err(reqwest_middleware::Error::Middleware(anyhow!(
            "Invalid HTTP {status} 'Location' value `{redirect_url}`: scheme needs to be https or http"
        )));
    }

    let mut headers = HeaderMap::new();
    std::mem::swap(req.headers_mut(), &mut headers);

    let cross_host = redirect_url.host_str() != original_req_url.host_str()
        || redirect_url.port_or_known_default() != original_req_url.port_or_known_default();
    if cross_host {
        if cross_origin_credentials_policy == CrossOriginCredentialsPolicy::Secure {
            debug!("Received a cross-origin redirect. Removing sensitive headers.");
            headers.remove(AUTHORIZATION);
            headers.remove(COOKIE);
            headers.remove(PROXY_AUTHORIZATION);
            headers.remove(WWW_AUTHENTICATE);
        }
    // If the redirect request is not a cross-origin request and the original request already
    // had a Referer header, attempt to set the Referer header for the redirect request.
    } else if headers.contains_key(REFERER) {
        if let Some(referer) = make_referer(&redirect_url, &original_req_url) {
            headers.insert(REFERER, referer);
        }
    }

    // Check if there are credentials on the redirect location itself.
    // If so, move them to Authorization header.
    if !redirect_url.username().is_empty() {
        if let Some(credentials) = Credentials::from_url(&redirect_url) {
            let _ = redirect_url.set_username("");
            let _ = redirect_url.set_password(None);
            headers.insert(AUTHORIZATION, credentials.to_header_value());
        }
    }

    std::mem::swap(req.headers_mut(), &mut headers);
    *req.url_mut() = Url::from(redirect_url);
    debug!(
        "Received HTTP {status}. Redirecting to {}",
        DisplaySafeUrl::ref_cast(req.url())
    );
    Ok(Some(req))
}

/// Return a Referer [`HeaderValue`] according to RFC 7231.
///
/// Return [`None`] if https has been downgraded in the redirect location.
fn make_referer(
    redirect_url: &DisplaySafeUrl,
    original_url: &DisplaySafeUrl,
) -> Option<HeaderValue> {
    if redirect_url.scheme() == "http" && original_url.scheme() == "https" {
        return None;
    }

    let mut referer = original_url.clone();
    referer.remove_credentials();
    referer.set_fragment(None);
    referer.as_str().parse().ok()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum CrossOriginCredentialsPolicy {
    /// Do not propagate credentials on cross-origin requests.
    #[default]
    Secure,

    /// Propagate credentials on cross-origin requests.
    ///
    /// WARNING: This should only be available for tests. In production code, preserving credentials
    /// during cross-origin redirects can lead to security vulnerabilities including credential
    /// leakage to untrusted domains.
    #[cfg(test)]
    Insecure,
}

/// A builder to construct the properties of a `Request`.
///
/// This wraps [`reqwest_middleware::RequestBuilder`] to ensure that the [`BaseClient`]
/// redirect policy is respected if `send()` is called.
#[derive(Debug)]
#[must_use]
pub struct RequestBuilder<'a> {
    builder: reqwest_middleware::RequestBuilder,
    client: &'a RedirectClientWithMiddleware,
}

impl<'a> RequestBuilder<'a> {
    pub fn new(
        builder: reqwest_middleware::RequestBuilder,
        client: &'a RedirectClientWithMiddleware,
    ) -> Self {
        Self { builder, client }
    }

    /// Add a `Header` to this Request.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.builder = self.builder.header(key, value);
        self
    }

    /// Add a set of Headers to the existing ones on this Request.
    ///
    /// The headers will be merged in to any already set.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.builder = self.builder.headers(headers);
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn version(mut self, version: reqwest::Version) -> Self {
        self.builder = self.builder.version(version);
        self
    }

    #[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
    pub fn multipart(mut self, multipart: multipart::Form) -> Self {
        self.builder = self.builder.multipart(multipart);
        self
    }

    /// Build a `Request`.
    pub fn build(self) -> reqwest::Result<Request> {
        self.builder.build()
    }

    /// Constructs the Request and sends it to the target URL, returning a
    /// future Response.
    pub async fn send(self) -> reqwest_middleware::Result<Response> {
        self.client.execute(self.build()?).await
    }

    pub fn raw_builder(&self) -> &reqwest_middleware::RequestBuilder {
        &self.builder
    }
}

/// Extends [`DefaultRetryableStrategy`], to log transient request failures and additional retry cases.
pub struct UvRetryableStrategy;

impl RetryableStrategy for UvRetryableStrategy {
    fn handle(&self, res: &Result<Response, reqwest_middleware::Error>) -> Option<Retryable> {
        // Use the default strategy and check for additional transient error cases.
        let retryable = match DefaultRetryableStrategy.handle(res) {
            None | Some(Retryable::Fatal)
                if res
                    .as_ref()
                    .is_err_and(|err| is_extended_transient_error(err)) =>
            {
                Some(Retryable::Transient)
            }
            default => default,
        };

        // Log on transient errors
        if retryable == Some(Retryable::Transient) {
            match res {
                Ok(response) => {
                    debug!("Transient request failure for: {}", response.url());
                }
                Err(err) => {
                    let context = iter::successors(err.source(), |&err| err.source())
                        .map(|err| format!("  Caused by: {err}"))
                        .join("\n");
                    debug!(
                        "Transient request failure for {}, retrying: {err}\n{context}",
                        err.url().map(Url::as_str).unwrap_or("unknown URL")
                    );
                }
            }
        }
        retryable
    }
}

/// Check for additional transient error kinds not supported by the default retry strategy in `reqwest_retry`.
///
/// These cases should be safe to retry with [`Retryable::Transient`].
pub fn is_extended_transient_error(err: &dyn Error) -> bool {
    // First, try to show a nice trace log
    if let Some((Some(status), Some(url))) = find_source::<crate::WrappedReqwestError>(&err)
        .map(|request_err| (request_err.status(), request_err.url()))
    {
        trace!("Considering retry of response HTTP {status} for {url}");
    } else {
        trace!("Considering retry of error: {err:?}");
    }

    // IO Errors may be nested through custom IO errors.
    for io_err in find_sources::<io::Error>(&err) {
        if io_err.kind() == io::ErrorKind::ConnectionReset
            || io_err.kind() == io::ErrorKind::UnexpectedEof
            || io_err.kind() == io::ErrorKind::BrokenPipe
        {
            trace!("Retrying error: `ConnectionReset` or `UnexpectedEof`");
            return true;
        }
        trace!("Cannot retry IO error: not one of `ConnectionReset` or `UnexpectedEof`");
    }

    trace!("Cannot retry error: not an IO error");
    false
}

/// Find the first source error of a specific type.
///
/// See <https://github.com/seanmonstar/reqwest/issues/1602#issuecomment-1220996681>
fn find_source<E: Error + 'static>(orig: &dyn Error) -> Option<&E> {
    let mut cause = orig.source();
    while let Some(err) = cause {
        if let Some(typed) = err.downcast_ref() {
            return Some(typed);
        }
        cause = err.source();
    }
    None
}

/// Return all errors in the chain of a specific type.
///
/// This handles cases such as nested `io::Error`s.
///
/// See <https://github.com/seanmonstar/reqwest/issues/1602#issuecomment-1220996681>
fn find_sources<E: Error + 'static>(orig: &dyn Error) -> impl Iterator<Item = &E> {
    iter::successors(find_source::<E>(orig), |&err| find_source(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    use reqwest::{Client, Method};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::base_client::request_into_redirect;

    #[tokio::test]
    async fn test_redirect_preserves_authorization_header_on_same_origin() -> Result<()> {
        for status in &[301, 302, 303, 307, 308] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(*status)
                        .insert_header("location", format!("{}/redirect", server.uri())),
                )
                .mount(&server)
                .await;

            let request = Client::new()
                .get(server.uri())
                .basic_auth("username", Some("password"))
                .build()
                .unwrap();

            assert!(request.headers().contains_key(AUTHORIZATION));

            let response = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap()
                .execute(request.try_clone().unwrap())
                .await
                .unwrap();

            let redirect_request =
                request_into_redirect(request, &response, CrossOriginCredentialsPolicy::Secure)?
                    .unwrap();
            assert!(redirect_request.headers().contains_key(AUTHORIZATION));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_removes_authorization_header_on_cross_origin() -> Result<()> {
        for status in &[301, 302, 303, 307, 308] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(*status)
                        .insert_header("location", "https://cross-origin.com/simple"),
                )
                .mount(&server)
                .await;

            let request = Client::new()
                .get(server.uri())
                .basic_auth("username", Some("password"))
                .build()
                .unwrap();

            assert!(request.headers().contains_key(AUTHORIZATION));

            let response = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap()
                .execute(request.try_clone().unwrap())
                .await
                .unwrap();

            let redirect_request =
                request_into_redirect(request, &response, CrossOriginCredentialsPolicy::Secure)?
                    .unwrap();
            assert!(!redirect_request.headers().contains_key(AUTHORIZATION));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_303_changes_post_to_get() -> Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(303)
                    .insert_header("location", format!("{}/redirect", server.uri())),
            )
            .mount(&server)
            .await;

        let request = Client::new()
            .post(server.uri())
            .basic_auth("username", Some("password"))
            .build()
            .unwrap();

        assert_eq!(request.method(), Method::POST);

        let response = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .execute(request.try_clone().unwrap())
            .await
            .unwrap();

        let redirect_request =
            request_into_redirect(request, &response, CrossOriginCredentialsPolicy::Secure)?
                .unwrap();
        assert_eq!(redirect_request.method(), Method::GET);

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_no_referer_if_disabled() -> Result<()> {
        for status in &[301, 302, 303, 307, 308] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(*status)
                        .insert_header("location", format!("{}/redirect", server.uri())),
                )
                .mount(&server)
                .await;

            let request = Client::builder()
                .referer(false)
                .build()
                .unwrap()
                .get(server.uri())
                .basic_auth("username", Some("password"))
                .build()
                .unwrap();

            assert!(!request.headers().contains_key(REFERER));

            let response = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap()
                .execute(request.try_clone().unwrap())
                .await
                .unwrap();

            let redirect_request =
                request_into_redirect(request, &response, CrossOriginCredentialsPolicy::Secure)?
                    .unwrap();

            assert!(!redirect_request.headers().contains_key(REFERER));
        }

        Ok(())
    }
}
