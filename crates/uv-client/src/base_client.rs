use pep508_rs::MarkerEnvironment;
use platform_tags::Platform;
use reqwest::{Client, ClientBuilder};
use reqwest_middleware::{ClientWithMiddleware, Middleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use std::env;
use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;
use std::slice::Iter;
use std::sync::Arc;
use tracing::debug;
use uv_auth::AuthMiddleware;
use uv_configuration::KeyringProviderType;
use uv_fs::Simplified;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::linehaul::LineHaul;
use crate::middleware::OfflineMiddleware;
use crate::Connectivity;

/// Newtype to implement [`Debug`] on [`Middleware`].
#[derive(Clone, Default)]
pub struct MiddlewareStack(Vec<Arc<dyn Middleware>>);

impl MiddlewareStack {
    /// Add an arbitrary middleware layer.
    // This function exists for rustlib users.
    pub fn with(mut self, middleware: impl Into<Arc<dyn Middleware>>) -> Self {
        self.0.push(middleware.into());
        self
    }

    /// Add an [`ExponentialBackoff`] layer with the given number of retries.
    pub fn with_retries(mut self, retries: u32) -> Self {
        if retries > 0 {
            let retry_policy = ExponentialBackoff::builder().build_with_max_retries(retries);
            let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);
            self.0.push(Arc::new(retry_strategy))
        }
        self
    }

    /// Add an [`AuthMiddleware`] layer with the given keyring provider.
    pub fn with_auth(mut self, keyring: KeyringProviderType) -> Self {
        let auth_middleware = AuthMiddleware::new().with_keyring(keyring.to_provider());
        self.0.push(Arc::new(auth_middleware));
        self
    }
}

impl Debug for MiddlewareStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiddlewareStack").finish()
    }
}

impl<'a> IntoIterator for &'a MiddlewareStack {
    type Item = &'a Arc<dyn Middleware>;
    type IntoIter = Iter<'a, Arc<dyn Middleware>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A builder for an [`BaseClient`].
#[derive(Debug, Clone, Default)]
pub struct BaseClientBuilder<'a> {
    native_tls: bool,
    connectivity: Connectivity,
    client: Option<Client>,
    markers: Option<&'a MarkerEnvironment>,
    platform: Option<&'a Platform>,
    middleware_stack: MiddlewareStack,
}

impl BaseClientBuilder<'_> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> BaseClientBuilder<'a> {
    #[must_use]
    pub fn connectivity(mut self, connectivity: Connectivity) -> Self {
        self.connectivity = connectivity;
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

    #[must_use]
    pub fn middleware_stack(mut self, middleware_stack: MiddlewareStack) -> Self {
        self.middleware_stack = middleware_stack;
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
                user_agent_string += &format!(" {}", output);
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
                        warn_user_once!("Ignoring invalid value from environment for UV_HTTP_TIMEOUT. Expected integer number of seconds, got \"{value}\".");
                        Ok(default_timeout)
                    })
            })
            .unwrap_or(default_timeout);
        debug!("Using registry request timeout of {timeout}s");

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

            client_core.build().expect("Failed to build HTTP client.")
        });

        // Wrap in any relevant middleware.
        let client = match self.connectivity {
            Connectivity::Online => {
                let mut client = reqwest_middleware::ClientBuilder::new(client.clone());
                for middleware in &self.middleware_stack {
                    client = client.with_arc(middleware.clone());
                }
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
