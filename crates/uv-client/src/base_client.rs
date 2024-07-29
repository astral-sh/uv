use std::error::Error;
use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;
use std::{env, iter};

use itertools::Itertools;
use reqwest::{Client, ClientBuilder, Response};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    DefaultRetryableStrategy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use tracing::debug;

use pep508_rs::MarkerEnvironment;
use platform_tags::Platform;
use uv_auth::AuthMiddleware;
use uv_configuration::KeyringProviderType;
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

        // Initialize the base client.
        let client = self.client.clone().unwrap_or_else(|| {
            // Check for the presence of an `SSL_CERT_FILE`.
            let ssl_cert_file_exists = env::var_os("SSL_CERT_FILE").is_some_and(|path| {
                let path_exists = Path::new(&path).exists();
                if !path_exists {
                    warn_user_once!(
                        "Ignoring invalid `SSL_CERT_FILE`. File does not exist: {}.",
                        path.simplified_display()
                    );
                }
                path_exists
            });

            // Configure the builder.
            let client_core = ClientBuilder::new()
                .user_agent(user_agent_string)
                .pool_max_idle_per_host(20)
                .read_timeout(std::time::Duration::from_secs(timeout))
                .tls_built_in_root_certs(false);

            // Configure TLS.
            let client_core = if self.native_tls || ssl_cert_file_exists {
                client_core.tls_built_in_native_certs(true)
            } else {
                client_core.tls_built_in_webpki_certs(true)
            };

            // Configure mTLS.
            let client_core = if let Some(ssl_client_cert) = env::var_os("SSL_CLIENT_CERT") {
                match read_identity(&ssl_client_cert) {
                    Ok(identity) => client_core.identity(identity),
                    Err(err) => {
                        warn_user_once!("Ignoring invalid `SSL_CLIENT_CERT`: {err}");
                        client_core
                    }
                }
            } else {
                client_core
            };

            client_core.build().expect("Failed to build HTTP client")
        });

        // Wrap in any relevant middleware.
        let client = match self.connectivity {
            Connectivity::Online => {
                let client = reqwest_middleware::ClientBuilder::new(client.clone());

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
            Connectivity::Offline => reqwest_middleware::ClientBuilder::new(client.clone())
                .with(OfflineMiddleware)
                .build(),
        };

        BaseClient {
            connectivity: self.connectivity,
            client,
            timeout,
        }
    }
}

/// A base client for HTTP requests
#[derive(Debug, Clone)]
pub struct BaseClient {
    /// The underlying HTTP client.
    client: ClientWithMiddleware,
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: u64,
}

impl BaseClient {
    /// The underlying [`ClientWithMiddleware`].
    pub fn client(&self) -> ClientWithMiddleware {
        self.client.clone()
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

// To avoid excessively verbose call chains, as the [`BaseClient`] is often nested within other client types.
impl Deref for BaseClient {
    type Target = ClientWithMiddleware;

    /// Deference to the underlying [`ClientWithMiddleware`].
    fn deref(&self) -> &Self::Target {
        &self.client
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
