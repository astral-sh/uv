use reqwest::{Client, ClientBuilder};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::env;
use std::fmt::Debug;
use std::path::Path;
use tracing::debug;
use uv_auth::{AuthMiddleware, KeyringProvider};
use uv_fs::Simplified;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::middleware::OfflineMiddleware;
use crate::tls::Roots;
use crate::{tls, Connectivity};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct BaseClientBuilder {
    keyring_provider: KeyringProvider,
    native_tls: bool,
    retries: u32,
    connectivity: Connectivity,
    client: Option<Client>,
}

impl BaseClientBuilder {
    pub fn new() -> Self {
        Self {
            keyring_provider: KeyringProvider::default(),
            native_tls: false,
            connectivity: Connectivity::Online,
            retries: 3,
            client: None,
        }
    }
}

impl BaseClientBuilder {
    #[must_use]
    pub fn keyring_provider(mut self, keyring_provider: KeyringProvider) -> Self {
        self.keyring_provider = keyring_provider;
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

    pub fn build(self) -> BaseClient {
        // Create user agent.
        let user_agent_string = format!("uv/{}", version());

        // Timeout options, matching https://doc.rust-lang.org/nightly/cargo/reference/config.html#httptimeout
        // `UV_REQUEST_TIMEOUT` is provided for backwards compatibility with v0.1.6
        let default_timeout = 5 * 60;
        let timeout = env::var("UV_HTTP_TIMEOUT")
            .or_else(|_| env::var("UV_REQUEST_TIMEOUT"))
            .or_else(|_| env::var("HTTP_TIMEOUT"))
            .and_then(|value| {
                value.parse::<u64>()
                    .or_else(|_| {
                        // On parse error, warn and use the default timeout
                        warn_user_once!("Ignoring invalid value from environment for UV_HTTP_TIMEOUT. Expected integer number of seconds, got \"{value}\".");
                        Ok(default_timeout)
                    })
            })
            .unwrap_or(default_timeout);
        debug!("Using registry request timeout of {}s", timeout);

        // Initialize the base client.
        let client = self.client.unwrap_or_else(|| {
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
            // Load the TLS configuration.
            let tls = tls::load(if self.native_tls || ssl_cert_file_exists {
                Roots::Native
            } else {
                Roots::Webpki
            })
            .expect("Failed to load TLS configuration.");

            let client_core = ClientBuilder::new()
                .user_agent(user_agent_string)
                .pool_max_idle_per_host(20)
                .timeout(std::time::Duration::from_secs(timeout))
                .use_preconfigured_tls(tls);

            client_core.build().expect("Failed to build HTTP client.")
        });

        // Wrap in any relevant middleware.
        let client = match self.connectivity {
            Connectivity::Online => {
                let client = reqwest_middleware::ClientBuilder::new(client.clone());

                // Initialize the retry strategy.
                let retry_policy =
                    ExponentialBackoff::builder().build_with_max_retries(self.retries);
                let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);
                let client = client.with(retry_strategy);

                // Initialize the authentication middleware to set headers.
                let client = client.with(AuthMiddleware::new(self.keyring_provider));

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
    /// The underyling [`ClientWithMiddleware`].
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
