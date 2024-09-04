use std::error::Error;
use std::fmt::Debug;
use std::path::Path;
use std::{env, iter};

use itertools::Itertools;
use pep508_rs::MarkerEnvironment;
use platform_tags::Platform;
use reqwest::{Client, ClientBuilder, Response};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    DefaultRetryableStrategy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use tracing::debug;
use url::Url;
use uv_auth::AuthMiddleware;
use uv_configuration::{KeyringProviderType, TrustedHost};
use uv_fs::Simplified;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::linehaul::LineHaul;
use crate::middleware::OfflineMiddleware;
use crate::tls::read_identity;
use crate::Connectivity;

/// A builder for an [`BaseClient`].
#[derive(Debug, Clone)]
pub struct BaseClientBuilder<'a> {
    keyring: KeyringProviderType,
    allow_insecure_host: Vec<TrustedHost>,
    native_tls: bool,
    retries: u32,
    pub connectivity: Connectivity,
    client: Option<Client>,
    markers: Option<&'a MarkerEnvironment>,
    platform: Option<&'a Platform>,
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
            retries: 3,
            client: None,
            markers: None,
            platform: None,
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
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
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

    pub fn is_offline(&self) -> bool {
        matches!(self.connectivity, Connectivity::Offline)
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
        let ssl_cert_file_exists = env::var_os("SSL_CERT_FILE").is_some_and(|path| {
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
        let default_timeout = 30;
        let timeout = env::var("UV_HTTP_TIMEOUT")
            .or_else(|_| env::var("UV_REQUEST_TIMEOUT"))
            .or_else(|_| env::var("HTTP_TIMEOUT"))
            .and_then(|value| {
                value.parse::<u64>()
                    .or_else(|_| {
                        // On parse error, warn and use the default timeout
                        warn_user_once!("Ignoring invalid value from environment for `UV_HTTP_TIMEOUT`. Expected an integer number of seconds, got \"{value}\".");
                        Ok(default_timeout)
                    })
            })
            .unwrap_or(default_timeout);
        debug!("Using request timeout of {timeout}s");

        // Create a secure client that validates certificates.
        let client = self.create_client(
            &user_agent_string,
            timeout,
            ssl_cert_file_exists,
            Security::Secure,
        );

        // Create an insecure client that accepts invalid certificates.
        let dangerous_client = self.create_client(
            &user_agent_string,
            timeout,
            ssl_cert_file_exists,
            Security::Insecure,
        );

        // Wrap in any relevant middleware and handle connectivity.
        let client = self.apply_middleware(client);
        let dangerous_client = self.apply_middleware(dangerous_client);

        BaseClient {
            connectivity: self.connectivity,
            allow_insecure_host: self.allow_insecure_host.clone(),
            client,
            dangerous_client,
            timeout,
        }
    }

    fn create_client(
        &self,
        user_agent: &str,
        timeout: u64,
        ssl_cert_file_exists: bool,
        security: Security,
    ) -> Client {
        // Configure the builder.
        let client_builder = ClientBuilder::new()
            .http1_title_case_headers()
            .user_agent(user_agent)
            .pool_max_idle_per_host(20)
            .read_timeout(std::time::Duration::from_secs(timeout))
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
        let client_builder = if let Some(ssl_client_cert) = env::var_os("SSL_CLIENT_CERT") {
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

        client_builder
            .build()
            .expect("Failed to build HTTP client.")
    }

    fn apply_middleware(&self, client: Client) -> ClientWithMiddleware {
        match self.connectivity {
            Connectivity::Online => {
                let client = reqwest_middleware::ClientBuilder::new(client);

                // Initialize the retry strategy.
                let retry_policy =
                    ExponentialBackoff::builder().build_with_max_retries(self.retries);
                let retry_strategy = RetryTransientMiddleware::new_with_policy_and_strategy(
                    retry_policy,
                    UvRetryableStrategy,
                );
                let client = client.with(retry_strategy);

                // Initialize the authentication middleware to set headers.
                let client =
                    client.with(AuthMiddleware::new().with_keyring(self.keyring.to_provider()));

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
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: u64,
    /// Hosts that are trusted to use the insecure client.
    allow_insecure_host: Vec<TrustedHost>,
}

#[derive(Debug, Clone, Copy)]
enum Security {
    /// The client should use secure settings, i.e., valid certificates.
    Secure,
    /// The client should use insecure settings, i.e., skip certificate validation.
    Insecure,
}

impl BaseClient {
    /// The underlying [`ClientWithMiddleware`] for secure requests.
    pub fn client(&self) -> ClientWithMiddleware {
        self.client.clone()
    }

    /// Selects the appropriate client based on the host's trustworthiness.
    pub fn for_host(&self, url: &Url) -> &ClientWithMiddleware {
        if self
            .allow_insecure_host
            .iter()
            .any(|allow_insecure_host| allow_insecure_host.matches(url))
        {
            &self.dangerous_client
        } else {
            &self.client
        }
    }

    /// The configured client timeout, in seconds.
    pub fn timeout(&self) -> u64 {
        self.timeout
    }

    /// The configured connectivity mode.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }
}

/// Extends [`DefaultRetryableStrategy`], to log transient request failures and additional retry cases.
struct UvRetryableStrategy;

impl RetryableStrategy for UvRetryableStrategy {
    fn handle(&self, res: &Result<Response, reqwest_middleware::Error>) -> Option<Retryable> {
        // Use the default strategy and check for additional transient error cases.
        let retryable = match DefaultRetryableStrategy.handle(res) {
            None | Some(Retryable::Fatal) if is_extended_transient_error(res) => {
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
                        err.url().map(reqwest::Url::as_str).unwrap_or("unknown URL")
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
fn is_extended_transient_error(res: &Result<Response, reqwest_middleware::Error>) -> bool {
    // Check for connection reset errors, these are usually `Body` errors which are not retried by default.
    if let Err(reqwest_middleware::Error::Reqwest(err)) = res {
        if let Some(io) = find_source::<std::io::Error>(&err) {
            if io.kind() == std::io::ErrorKind::ConnectionReset
                || io.kind() == std::io::ErrorKind::UnexpectedEof
            {
                return true;
            }
        }
    }

    false
}

/// Find the first source error of a specific type.
///
/// See <https://github.com/seanmonstar/reqwest/issues/1602#issuecomment-1220996681>
fn find_source<E: std::error::Error + 'static>(orig: &dyn std::error::Error) -> Option<&E> {
    let mut cause = orig.source();
    while let Some(err) = cause {
        if let Some(typed) = err.downcast_ref() {
            return Some(typed);
        }
        cause = err.source();
    }

    // else
    None
}
