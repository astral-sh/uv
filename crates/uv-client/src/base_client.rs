use itertools::Itertools;
use reqwest::{Client, ClientBuilder, Proxy, Response};
use reqwest_middleware::{ClientWithMiddleware, Middleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    DefaultRetryableStrategy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use std::error::Error;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::{env, iter};
use tracing::{debug, trace};
use url::Url;
use uv_auth::{AuthMiddleware, UrlAuthPolicies};
use uv_configuration::{KeyringProviderType, TrustedHost};
use uv_fs::Simplified;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_static::EnvVars;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::linehaul::LineHaul;
use crate::middleware::OfflineMiddleware;
use crate::tls::read_identity;
use crate::Connectivity;

pub const DEFAULT_RETRIES: u32 = 3;

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
    url_auth_policies: Option<UrlAuthPolicies>,
    default_timeout: Duration,
    extra_middleware: Option<ExtraMiddleware>,
    proxies: Vec<Proxy>,
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
            url_auth_policies: None,
            default_timeout: Duration::from_secs(30),
            extra_middleware: None,
            proxies: vec![],
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
    pub fn url_auth_policies(mut self, auth_policies: UrlAuthPolicies) -> Self {
        self.url_auth_policies = Some(auth_policies);
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
                user_agent_string += &format!(" {output}");
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
        );

        // Create an insecure client that accepts invalid certificates.
        let raw_dangerous_client = self.create_client(
            &user_agent_string,
            timeout,
            ssl_cert_file_exists,
            Security::Insecure,
        );

        // Wrap in any relevant middleware and handle connectivity.
        let client = self.apply_middleware(raw_client.clone());
        let dangerous_client = self.apply_middleware(raw_dangerous_client.clone());

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
        let client = self.apply_middleware(existing.raw_client.clone());
        let dangerous_client = self.apply_middleware(existing.raw_dangerous_client.clone());

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
    ) -> Client {
        // Configure the builder.
        let client_builder = ClientBuilder::new()
            .http1_title_case_headers()
            .user_agent(user_agent)
            .pool_max_idle_per_host(20)
            .read_timeout(timeout)
            .tls_built_in_root_certs(false);

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
                        let mut auth_middleware =
                            AuthMiddleware::new().with_keyring(self.keyring.to_provider());
                        if let Some(url_auth_policies) = &self.url_auth_policies {
                            auth_middleware =
                                auth_middleware.with_url_auth_policies(url_auth_policies.clone());
                        }
                        client = client.with(auth_middleware);
                    }
                    AuthIntegration::OnlyAuthenticated => {
                        client = client.with(
                            AuthMiddleware::new()
                                .with_keyring(self.keyring.to_provider())
                                .with_only_authenticated(true),
                        );
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
    client: ClientWithMiddleware,
    /// The underlying HTTP client that accepts invalid certificates.
    dangerous_client: ClientWithMiddleware,
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
    pub fn for_host(&self, url: &Url) -> &ClientWithMiddleware {
        if self.disable_ssl(url) {
            &self.dangerous_client
        } else {
            &self.client
        }
    }

    /// Returns `true` if the host is trusted to use the insecure client.
    pub fn disable_ssl(&self, url: &Url) -> bool {
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
    trace!("Considering retry of error: {err:?}");

    if let Some(io) = find_source::<std::io::Error>(&err) {
        if io.kind() == std::io::ErrorKind::ConnectionReset
            || io.kind() == std::io::ErrorKind::UnexpectedEof
        {
            trace!("Retrying error: `ConnectionReset` or `UnexpectedEof`");
            return true;
        }
        trace!("Cannot retry error: not one of `ConnectionReset` or `UnexpectedEof`");
    } else {
        trace!("Cannot retry error: not an IO error");
    }

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
