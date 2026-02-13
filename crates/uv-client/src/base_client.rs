use std::error::Error;
use std::fmt::Debug;
use std::fmt::Write;
use std::num::ParseIntError;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
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
use reqwest::{Client, ClientBuilder, IntoUrl, NoProxy, Proxy, Request, Response, multipart};
use reqwest_middleware::{ClientWithMiddleware, Middleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    RetryPolicy, RetryTransientMiddleware, Retryable, RetryableStrategy, default_on_request_error,
    default_on_request_success,
};
use thiserror::Error;
use tracing::{debug, trace};
use url::ParseError;
use url::Url;

use uv_auth::{AuthMiddleware, Credentials, CredentialsCache, Indexes, PyxTokenStore};
use uv_configuration::ProxyUrlKind;
use uv_configuration::{KeyringProviderType, ProxyUrl, TrustedHost};
use uv_fs::Simplified;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_preview::Preview;
use uv_redacted::DisplaySafeUrl;
use uv_redacted::DisplaySafeUrlError;
use uv_static::EnvVars;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::linehaul::LineHaul;
use crate::middleware::OfflineMiddleware;
use crate::tls::read_identity;
use crate::{Connectivity, WrappedReqwestError};

pub const DEFAULT_RETRIES: u32 = 3;

/// Maximum number of redirects to follow before giving up.
///
/// This is the default used by [`reqwest`].
pub const DEFAULT_MAX_REDIRECTS: u32 = 10;

/// The maximum time between two reads.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The maximum time to connect to a server.
///
/// This value is set lower to fail relatively quickly when the index is unreachable or down.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total duration an upload may take.
///
/// reqwest does not support something like a read timeout for uploads, so we have to set a (large)
/// timeout on the entire upload.
pub const DEFAULT_READ_TIMEOUT_UPLOAD: Duration = Duration::from_mins(15);

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
    preview: Preview,
    allow_insecure_host: Vec<TrustedHost>,
    native_tls: bool,
    built_in_root_certs: bool,
    retries: u32,
    pub connectivity: Connectivity,
    markers: Option<&'a MarkerEnvironment>,
    platform: Option<&'a Platform>,
    auth_integration: AuthIntegration,
    /// Global authentication cache for a uv invocation to share credentials across uv clients.
    credentials_cache: Arc<CredentialsCache>,
    indexes: Indexes,
    read_timeout: Duration,
    connect_timeout: Duration,
    extra_middleware: Option<ExtraMiddleware>,
    proxies: Vec<Proxy>,
    http_proxy: Option<ProxyUrl>,
    https_proxy: Option<ProxyUrl>,
    no_proxy: Option<Vec<String>>,
    redirect_policy: RedirectPolicy,
    /// Whether credentials should be propagated during cross-origin redirects.
    ///
    /// A policy allowing propagation is insecure and should only be available for test code.
    cross_origin_credential_policy: CrossOriginCredentialsPolicy,
    /// Optional custom reqwest client to use instead of creating a new one.
    custom_client: Option<Client>,
    /// uv subcommand in which this client is being used
    subcommand: Option<Vec<String>>,
    /// Optional name for this client, used in debug logging.
    client_name: Option<&'static str>,
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
    /// No redirect for non-cloneable (e.g., streaming) requests with custom redirect logic.
    NoRedirect,
}

impl RedirectPolicy {
    pub fn reqwest_policy(self) -> reqwest::redirect::Policy {
        match self {
            Self::BypassMiddleware => reqwest::redirect::Policy::default(),
            Self::RetriggerMiddleware => reqwest::redirect::Policy::none(),
            Self::NoRedirect => reqwest::redirect::Policy::none(),
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
        Self {
            keyring: KeyringProviderType::default(),
            preview: Preview::default(),
            allow_insecure_host: vec![],
            native_tls: false,
            built_in_root_certs: false,
            connectivity: Connectivity::Online,
            retries: DEFAULT_RETRIES,
            markers: None,
            platform: None,
            auth_integration: AuthIntegration::default(),
            credentials_cache: Arc::new(CredentialsCache::default()),
            indexes: Indexes::new(),
            read_timeout: DEFAULT_READ_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            extra_middleware: None,
            proxies: vec![],
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            redirect_policy: RedirectPolicy::default(),
            cross_origin_credential_policy: CrossOriginCredentialsPolicy::Secure,
            custom_client: None,
            subcommand: None,
            client_name: None,
        }
    }
}

impl<'a> BaseClientBuilder<'a> {
    pub fn new(
        connectivity: Connectivity,
        native_tls: bool,
        allow_insecure_host: Vec<TrustedHost>,
        preview: Preview,
        read_timeout: Duration,
        connect_timeout: Duration,
        retries: u32,
    ) -> Self {
        Self {
            preview,
            allow_insecure_host,
            native_tls,
            retries,
            connectivity,
            read_timeout,
            connect_timeout,
            ..Self::default()
        }
    }

    /// Use a custom reqwest client instead of creating a new one.
    ///
    /// This allows you to provide your own reqwest client with custom configuration.
    /// Note that some configuration options from this builder will still be applied
    /// to the client via middleware.
    #[must_use]
    pub fn custom_client(mut self, client: Client) -> Self {
        self.custom_client = Some(client);
        self
    }

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
    pub fn built_in_root_certs(mut self, built_in_root_certs: bool) -> Self {
        self.built_in_root_certs = built_in_root_certs;
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
    pub fn read_timeout(mut self, read_timeout: Duration) -> Self {
        self.read_timeout = read_timeout;
        self
    }

    #[must_use]
    pub fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
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
    pub fn http_proxy(mut self, http_proxy: Option<ProxyUrl>) -> Self {
        self.http_proxy = http_proxy;
        self
    }

    #[must_use]
    pub fn https_proxy(mut self, https_proxy: Option<ProxyUrl>) -> Self {
        self.https_proxy = https_proxy;
        self
    }

    #[must_use]
    pub fn no_proxy(mut self, no_proxy: Option<Vec<String>>) -> Self {
        self.no_proxy = no_proxy;
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

    #[must_use]
    pub fn subcommand(mut self, subcommand: Vec<String>) -> Self {
        self.subcommand = Some(subcommand);
        self
    }

    #[must_use]
    pub fn client_name(mut self, name: &'static str) -> Self {
        self.client_name = Some(name);
        self
    }

    pub fn credentials_cache(&self) -> &CredentialsCache {
        &self.credentials_cache
    }

    /// See [`CredentialsCache::store_credentials_from_url`].
    pub fn store_credentials_from_url(&self, url: &DisplaySafeUrl) -> bool {
        self.credentials_cache.store_credentials_from_url(url)
    }

    /// See [`CredentialsCache::store_credentials`].
    pub fn store_credentials(&self, url: &DisplaySafeUrl, credentials: Credentials) {
        self.credentials_cache.store_credentials(url, credentials);
    }

    pub fn is_native_tls(&self) -> bool {
        self.native_tls
    }

    pub fn is_offline(&self) -> bool {
        matches!(self.connectivity, Connectivity::Offline)
    }

    /// Create a [`RetryPolicy`] for the client.
    pub fn retry_policy(&self) -> ExponentialBackoff {
        let mut builder = ExponentialBackoff::builder();
        if env::var_os(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY).is_some() {
            builder = builder.retry_bounds(Duration::from_millis(0), Duration::from_millis(0));
        }
        builder.build_with_max_retries(self.retries)
    }

    pub fn build(&self) -> BaseClient {
        if let Some(name) = self.client_name {
            debug!(
                "Using request connect timeout of {}s and read timeout of {}s for {} client",
                self.connect_timeout.as_secs(),
                self.read_timeout.as_secs(),
                name
            );
        } else {
            debug!(
                "Using request connect timeout of {}s and read timeout of {}s",
                self.connect_timeout.as_secs(),
                self.read_timeout.as_secs()
            );
        }

        // Use the custom client if provided, otherwise create a new one
        let (raw_client, raw_dangerous_client) = match &self.custom_client {
            Some(client) => (client.clone(), client.clone()),
            None => {
                self.create_secure_and_insecure_clients(self.read_timeout, self.connect_timeout)
            }
        };

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
            read_timeout: self.read_timeout,
            connect_timeout: self.connect_timeout,
            credentials_cache: self.credentials_cache.clone(),
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
            read_timeout: existing.read_timeout,
            connect_timeout: existing.connect_timeout,
            credentials_cache: existing.credentials_cache.clone(),
        }
    }

    fn create_secure_and_insecure_clients(
        &self,
        read_timeout: Duration,
        connect_timeout: Duration,
    ) -> (Client, Client) {
        // Create user agent.
        let mut user_agent_string = format!("uv/{}", version());

        // Add linehaul metadata.
        let linehaul = LineHaul::new(self.markers, self.platform, self.subcommand.clone());
        if let Ok(output) = serde_json::to_string(&linehaul) {
            let _ = write!(user_agent_string, " {output}");
        }

        // Checks for the presence of `SSL_CERT_FILE`.
        // Certificate loading support is delegated to `rustls-native-certs`.
        // See https://github.com/rustls/rustls-native-certs/blob/813790a297ad4399efe70a8e5264ca1b420acbec/src/lib.rs#L118-L125
        let ssl_cert_file_exists = env::var_os(EnvVars::SSL_CERT_FILE).is_some_and(|path| {
            let path = Path::new(&path);
            match path.metadata() {
                Ok(metadata) if metadata.is_file() => true,
                Ok(_) => {
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_FILE`. Path is not a file: {}.",
                        path.simplified_display().cyan()
                    );
                    false
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_FILE`. Path does not exist: {}.",
                        path.simplified_display().cyan()
                    );
                    false
                }
                Err(err) => {
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_FILE`. Path is not accessible: {} ({err}).",
                        path.simplified_display().cyan()
                    );
                    false
                }
            }
        });

        // Checks for the presence of `SSL_CERT_DIR`.
        // Certificate loading support is delegated to `rustls-native-certs`.
        // See https://github.com/rustls/rustls-native-certs/blob/813790a297ad4399efe70a8e5264ca1b420acbec/src/lib.rs#L118-L125
        let ssl_cert_dir_exists = env::var_os(EnvVars::SSL_CERT_DIR)
            .filter(|v| !v.is_empty())
            .is_some_and(|dirs| {
                // Parse `SSL_CERT_DIR`, with support for multiple entries using
                // a platform-specific delimiter (`:` on Unix, `;` on Windows)
                let (existing, missing): (Vec<_>, Vec<_>) =
                    env::split_paths(&dirs).partition(|p| p.exists());

                if existing.is_empty() {
                    let end_note = if missing.len() == 1 {
                        "The directory does not exist."
                    } else {
                        "The entries do not exist."
                    };
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_DIR`. {end_note}: {}.",
                        missing
                            .iter()
                            .map(Simplified::simplified_display)
                            .join(", ")
                            .cyan()
                    );
                    return false;
                }

                // Warn on any missing entries
                if !missing.is_empty() {
                    let end_note = if missing.len() == 1 {
                        "The following directory does not exist:"
                    } else {
                        "The following entries do not exist:"
                    };
                    warn_user_once!(
                        "Invalid entries in `SSL_CERT_DIR`. {end_note}: {}.",
                        missing
                            .iter()
                            .map(Simplified::simplified_display)
                            .join(", ")
                            .cyan()
                    );
                }

                // Proceed while ignoring missing entries
                true
            });

        // Create a secure client that validates certificates.
        let raw_client = self.create_client(
            &user_agent_string,
            read_timeout,
            connect_timeout,
            ssl_cert_file_exists,
            ssl_cert_dir_exists,
            Security::Secure,
            self.redirect_policy,
        );

        // Create an insecure client that accepts invalid certificates.
        let raw_dangerous_client = self.create_client(
            &user_agent_string,
            read_timeout,
            connect_timeout,
            ssl_cert_file_exists,
            ssl_cert_dir_exists,
            Security::Insecure,
            self.redirect_policy,
        );

        (raw_client, raw_dangerous_client)
    }

    fn create_client(
        &self,
        user_agent: &str,
        read_timeout: Duration,
        connect_timeout: Duration,
        ssl_cert_file_exists: bool,
        ssl_cert_dir_exists: bool,
        security: Security,
        redirect_policy: RedirectPolicy,
    ) -> Client {
        // Configure the builder.
        let client_builder = ClientBuilder::new()
            .http1_title_case_headers()
            .user_agent(user_agent)
            .pool_max_idle_per_host(20)
            .read_timeout(read_timeout)
            .connect_timeout(connect_timeout)
            .tls_built_in_root_certs(self.built_in_root_certs)
            .redirect(redirect_policy.reqwest_policy());

        // If necessary, accept invalid certificates.
        let client_builder = match security {
            Security::Secure => client_builder,
            Security::Insecure => client_builder.danger_accept_invalid_certs(true),
        };

        let client_builder = if self.native_tls || ssl_cert_file_exists || ssl_cert_dir_exists {
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

        let no_proxy = self
            .no_proxy
            .as_ref()
            .and_then(|no_proxy| NoProxy::from_string(&no_proxy.join(",")));

        if let Some(http_proxy) = &self.http_proxy {
            let proxy = http_proxy
                .as_proxy(ProxyUrlKind::Http)
                .no_proxy(no_proxy.clone());
            client_builder = client_builder.proxy(proxy);
        }

        if let Some(https_proxy) = &self.https_proxy {
            let proxy = https_proxy.as_proxy(ProxyUrlKind::Https).no_proxy(no_proxy);
            client_builder = client_builder.proxy(proxy);
        }

        let client_builder = client_builder;

        client_builder
            .build()
            .expect("Failed to build HTTP client.")
    }

    fn apply_middleware(&self, client: Client) -> ClientWithMiddleware {
        match self.connectivity {
            Connectivity::Online => {
                // Create a base client to using in the authentication middleware.
                let base_client = {
                    let mut client = reqwest_middleware::ClientBuilder::new(client.clone());

                    // Avoid uncloneable errors with a streaming body during publish.
                    if self.retries > 0 {
                        // Initialize the retry strategy.
                        let retry_strategy = RetryTransientMiddleware::new_with_policy_and_strategy(
                            self.retry_policy(),
                            UvRetryableStrategy,
                        );
                        client = client.with(retry_strategy);
                    }

                    // When supplied, add the extra middleware.
                    if let Some(extra_middleware) = &self.extra_middleware {
                        for middleware in &extra_middleware.0 {
                            client = client.with_arc(middleware.clone());
                        }
                    }

                    client.build()
                };

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

                // When supplied, add the extra middleware.
                if let Some(extra_middleware) = &self.extra_middleware {
                    for middleware in &extra_middleware.0 {
                        client = client.with_arc(middleware.clone());
                    }
                }

                // Initialize the authentication middleware to set headers.
                match self.auth_integration {
                    AuthIntegration::Default => {
                        let mut auth_middleware = AuthMiddleware::new()
                            .with_cache_arc(self.credentials_cache.clone())
                            .with_base_client(base_client)
                            .with_indexes(self.indexes.clone())
                            .with_keyring(self.keyring.to_provider())
                            .with_preview(self.preview);
                        if let Ok(token_store) = PyxTokenStore::from_settings() {
                            auth_middleware = auth_middleware.with_pyx_token_store(token_store);
                        }
                        client = client.with(auth_middleware);
                    }
                    AuthIntegration::OnlyAuthenticated => {
                        let mut auth_middleware = AuthMiddleware::new()
                            .with_cache_arc(self.credentials_cache.clone())
                            .with_base_client(base_client)
                            .with_indexes(self.indexes.clone())
                            .with_keyring(self.keyring.to_provider())
                            .with_preview(self.preview)
                            .with_only_authenticated(true);
                        if let Ok(token_store) = PyxTokenStore::from_settings() {
                            auth_middleware = auth_middleware.with_pyx_token_store(token_store);
                        }
                        client = client.with(auth_middleware);
                    }
                    AuthIntegration::NoAuthMiddleware => {
                        // The downstream code uses custom auth logic.
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
    /// Configured client read timeout.
    read_timeout: Duration,
    /// Configured client connect timeout.
    connect_timeout: Duration,
    /// Hosts that are trusted to use the insecure client.
    allow_insecure_host: Vec<TrustedHost>,
    /// The number of retries to attempt on transient errors.
    retries: u32,
    /// Global authentication cache for a uv invocation to share credentials across uv clients.
    credentials_cache: Arc<CredentialsCache>,
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
        let client = self.for_host(&DisplaySafeUrl::from_url(req.url().clone()));
        client.execute(req).await
    }

    /// Returns `true` if the host is trusted to use the insecure client.
    pub fn disable_ssl(&self, url: &DisplaySafeUrl) -> bool {
        self.allow_insecure_host
            .iter()
            .any(|allow_insecure_host| allow_insecure_host.matches(url))
    }

    /// The configured client read timeout.
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
    }

    /// The configured client connect timeout.
    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// The configured connectivity mode.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// The [`RetryPolicy`] for the client.
    pub fn retry_policy(&self) -> ExponentialBackoff {
        let mut builder = ExponentialBackoff::builder();
        if env::var_os(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY).is_some() {
            builder = builder.retry_bounds(Duration::from_millis(0), Duration::from_millis(0));
        }
        builder.build_with_max_retries(self.retries)
    }

    pub fn credentials_cache(&self) -> &CredentialsCache {
        &self.credentials_cache
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
    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder<'_> {
        RequestBuilder::new(self.client.get(url), self)
    }

    /// Convenience method to make a `POST` request to a URL.
    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder<'_> {
        RequestBuilder::new(self.client.post(url), self)
    }

    /// Convenience method to make a `HEAD` request to a URL.
    pub fn head<U: IntoUrl>(&self, url: U) -> RequestBuilder<'_> {
        RequestBuilder::new(self.client.head(url), self)
    }

    /// Executes a request, applying the redirect policy.
    pub async fn execute(&self, req: Request) -> reqwest_middleware::Result<Response> {
        match self.redirect_policy {
            RedirectPolicy::BypassMiddleware => self.client.execute(req).await,
            RedirectPolicy::RetriggerMiddleware => self.execute_with_redirect_handling(req).await,
            RedirectPolicy::NoRedirect => self.client.execute(req).await,
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
    fn from(item: RedirectClientWithMiddleware) -> Self {
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
    let original_req_url = DisplaySafeUrl::from_url(req.url().clone());
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
        Err(DisplaySafeUrlError::Url(ParseError::RelativeUrlWithoutBase)) => original_req_url.join(location).map_err(|err| {
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

/// An extension over [`DefaultRetryableStrategy`] that logs transient request failures and
/// adds additional retry cases.
pub struct UvRetryableStrategy;

impl RetryableStrategy for UvRetryableStrategy {
    fn handle(&self, res: &Result<Response, reqwest_middleware::Error>) -> Option<Retryable> {
        let retryable = match res {
            Ok(success) => default_on_request_success(success),
            Err(err) => retryable_on_request_failure(err),
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

/// Whether the error looks like a network error that should be retried.
///
/// This is an extension over [`reqwest_middleware::default_on_request_failure`], which is missing
/// a number of cases:
/// * Inside the reqwest or reqwest-middleware error is an `io::Error` such as a broken pipe
/// * When streaming a response, a reqwest error may be hidden several layers behind errors
///   of different crates processing the stream, including `io::Error` layers
/// * Any `h2` error
fn retryable_on_request_failure(err: &(dyn Error + 'static)) -> Option<Retryable> {
    // First, try to show a nice trace log
    if let Some((Some(status), Some(url))) = find_source::<WrappedReqwestError>(&err)
        .map(|request_err| (request_err.status(), request_err.url()))
    {
        trace!(
            "Considering retry of response HTTP {status} for {url}",
            url = DisplaySafeUrl::from_url(url.clone())
        );
    } else {
        trace!("Considering retry of error: {err:?}");
    }

    let mut has_known_error = false;
    // IO Errors or reqwest errors may be nested through custom IO errors or stream processing
    // crates
    let mut current_source = Some(err);
    while let Some(source) = current_source {
        // Handle different kinds of reqwest error nesting not accessible by downcast.
        let reqwest_err = if let Some(reqwest_err) = source.downcast_ref::<reqwest::Error>() {
            Some(reqwest_err)
        } else if let Some(reqwest_err) = source
            .downcast_ref::<WrappedReqwestError>()
            .and_then(|err| err.inner())
        {
            Some(reqwest_err)
        } else if let Some(reqwest_middleware::Error::Reqwest(reqwest_err)) =
            source.downcast_ref::<reqwest_middleware::Error>()
        {
            Some(reqwest_err)
        } else {
            None
        };

        if let Some(reqwest_err) = reqwest_err {
            has_known_error = true;
            // Ignore the default retry strategy returning fatal.
            if default_on_request_error(reqwest_err) == Some(Retryable::Transient) {
                trace!("Retrying nested reqwest error");
                return Some(Retryable::Transient);
            }
            if is_retryable_status_error(reqwest_err) {
                trace!("Retrying nested reqwest status code error");
                return Some(Retryable::Transient);
            }

            trace!("Cannot retry nested reqwest error");
        } else if source.downcast_ref::<h2::Error>().is_some() {
            // All h2 errors look like errors that should be retried
            // https://github.com/astral-sh/uv/issues/15916
            trace!("Retrying nested h2 error");
            return Some(Retryable::Transient);
        } else if let Some(io_err) = source.downcast_ref::<io::Error>() {
            has_known_error = true;
            let retryable_io_err_kinds = [
                // https://github.com/astral-sh/uv/issues/12054
                io::ErrorKind::BrokenPipe,
                // From reqwest-middleware
                io::ErrorKind::ConnectionAborted,
                // https://github.com/astral-sh/uv/issues/3514
                io::ErrorKind::ConnectionReset,
                // https://github.com/astral-sh/uv/issues/14699
                io::ErrorKind::InvalidData,
                // https://github.com/astral-sh/uv/issues/17697#issuecomment-3817060484
                io::ErrorKind::TimedOut,
                // https://github.com/astral-sh/uv/issues/9246
                io::ErrorKind::UnexpectedEof,
            ];
            if retryable_io_err_kinds.contains(&io_err.kind()) {
                trace!("Retrying error: `{}`", io_err.kind());
                return Some(Retryable::Transient);
            }

            trace!(
                "Cannot retry IO error `{}`, not a retryable IO error kind",
                io_err.kind()
            );
        }

        current_source = source.source();
    }

    if !has_known_error {
        trace!("Cannot retry error: Neither an IO error nor a reqwest error");
    }

    None
}

/// Per-request retry state and policy.
pub struct RetryState {
    retry_policy: ExponentialBackoff,
    start_time: SystemTime,
    total_retries: u32,
    url: DisplaySafeUrl,
}

impl RetryState {
    /// Initialize the [`RetryState`] and record the start time for the retry policy.
    pub fn start(retry_policy: ExponentialBackoff, url: impl Into<DisplaySafeUrl>) -> Self {
        Self {
            retry_policy,
            start_time: SystemTime::now(),
            total_retries: 0,
            url: url.into(),
        }
    }

    /// The number of retries across all requests.
    ///
    /// After a failed retryable request, this equals the maximum number of retries.
    pub fn total_retries(&self) -> u32 {
        self.total_retries
    }

    /// Determines whether request should be retried.
    ///
    /// Takes the number of retries from nested layers associated with the specific `err` type as
    /// `error_retries`.
    ///
    /// Returns the backoff duration if the request should be retried.
    #[must_use]
    pub fn should_retry(
        &mut self,
        err: &(dyn Error + 'static),
        error_retries: u32,
    ) -> Option<Duration> {
        // If the middleware performed any retries, consider them in our budget.
        self.total_retries += error_retries;
        match retryable_on_request_failure(err) {
            Some(Retryable::Transient) => {
                // Capture `now` before calling the policy so that `execute_after`
                // (computed from a `SystemTime::now()` inside the library) is always
                // >= `now`, making `duration_since` reliable.
                let now = SystemTime::now();
                let retry_decision = self
                    .retry_policy
                    .should_retry(self.start_time, self.total_retries);
                if let reqwest_retry::RetryDecision::Retry { execute_after } = retry_decision {
                    let duration = execute_after
                        .duration_since(now)
                        .unwrap_or_else(|_| Duration::default());

                    self.total_retries += 1;
                    return Some(duration);
                }

                None
            }
            Some(Retryable::Fatal) | None => None,
        }
    }

    /// Wait before retrying the request.
    pub async fn sleep_backoff(&self, duration: Duration) {
        debug!(
            "Transient failure while handling response from {}; retrying after {:.1}s...",
            self.url,
            duration.as_secs_f32(),
        );
        // TODO(konsti): Should we show a spinner plus a message in the CLI while
        // waiting?
        tokio::time::sleep(duration).await;
    }
}

/// Whether the error is a status code error that is retryable.
///
/// Port of `reqwest_retry::default_on_request_success`.
fn is_retryable_status_error(reqwest_err: &reqwest::Error) -> bool {
    let Some(status) = reqwest_err.status() else {
        return false;
    };
    status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
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

// TODO(konsti): Remove once we find a native home for `retries_from_env`
#[derive(Debug, Error)]
pub enum RetryParsingError {
    #[error("Failed to parse `UV_HTTP_RETRIES`")]
    ParseInt(#[from] ParseIntError),
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use insta::assert_debug_snapshot;
    use reqwest::{Client, Method};
    use wiremock::matchers::{method, path};
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
    async fn test_redirect_preserves_fragment() -> Result<()> {
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
                .get(format!("{}#fragment", server.uri()))
                .build()
                .unwrap();

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
            assert!(
                redirect_request
                    .url()
                    .fragment()
                    .is_some_and(|fragment| fragment == "fragment")
            );
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

    /// Enumerate which status codes we are retrying.
    #[tokio::test]
    async fn retried_status_codes() -> Result<()> {
        let server = MockServer::start().await;
        let client = Client::default();
        let middleware_client = ClientWithMiddleware::default();
        let mut retried = Vec::new();
        for status in 100..599 {
            // Test all standard status codes and an example for a non-RFC code used in the wild.
            if StatusCode::from_u16(status)?.canonical_reason().is_none() && status != 420 {
                continue;
            }

            Mock::given(path(format!("/{status}")))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let response = middleware_client
                .get(format!("{}/{}", server.uri(), status))
                .send()
                .await;

            let middleware_retry =
                UvRetryableStrategy.handle(&response) == Some(Retryable::Transient);

            let response = client
                .get(format!("{}/{}", server.uri(), status))
                .send()
                .await?;

            let uv_retry = match response.error_for_status() {
                Ok(_) => false,
                Err(err) => retryable_on_request_failure(&err) == Some(Retryable::Transient),
            };

            // Ensure we're retrying the same status code as the reqwest_retry crate. We may choose
            // to deviate from this later.
            assert_eq!(middleware_retry, uv_retry);
            if uv_retry {
                retried.push(status);
            }
        }

        assert_debug_snapshot!(retried, @r"
        [
            100,
            102,
            103,
            408,
            429,
            500,
            501,
            502,
            503,
            504,
            505,
            506,
            507,
            508,
            510,
            511,
        ]
        ");

        Ok(())
    }
}
