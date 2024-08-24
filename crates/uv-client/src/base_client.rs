use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::{env, iter};
use std::collections::HashSet;
use std::sync::Arc;
use itertools::Itertools;
use reqwest::{Client, ClientBuilder, Response};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    DefaultRetryableStrategy, RetryTransientMiddleware, Retryable, RetryableStrategy,
};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
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


struct CustomCertVerifier {
    hosts_to_skip_tls_verification: HashSet<String>,
    default_verifier: Arc<dyn ServerCertVerifier>,
}

impl Debug for CustomCertVerifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl ServerCertVerifier for CustomCertVerifier {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], server_name: &ServerName<'_>, ocsp_response: &[u8], now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        // Check if the server_name matches any host in the list to skip verification
        // Resolve the server name from DNS.
        if let ServerName::DnsName(dns_name) = server_name {
            if self.hosts_to_skip_tls_verification.contains(dns_name.as_ref()) {
                return Ok(ServerCertVerified::assertion());
            }
        }

        // Perform default certificate verification
        self.default_verifier.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )
    }


    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.default_verifier.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.default_verifier.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.default_verifier.supported_verify_schemes()
    }
}


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
                        path.simplified_display().cyan()
                    );
                }
                path_exists
            });

            // Configure TLS.
            let tls = {
                // Set root certificates.
                let mut root_cert_store = rustls::RootCertStore::empty();

                if self.native_tls || ssl_cert_file_exists {
                    let mut valid_count = 0;
                    let mut invalid_count = 0;
                    for cert in rustls_native_certs::load_native_certs().unwrap()
                    {
                        // Continue on parsing errors, as native stores often include ancient or syntactically
                        // invalid certificates, like root certificates without any X509 extensions.
                        // Inspiration: https://github.com/rustls/rustls/blob/633bf4ba9d9521a95f68766d04c22e2b01e68318/rustls/src/anchors.rs#L105-L112
                        match root_cert_store.add(cert.into()) {
                            Ok(_) => valid_count += 1,
                            Err(err) => {
                                invalid_count += 1;
                                debug!("rustls failed to parse DER certificate: {err:?}");
                            }
                        }
                    }
                    if valid_count == 0 && invalid_count > 0 {
                        // return Err(crate::error::builder(
                        //     "zero valid certificates found in native root store",
                        // ));
                    }
                } else{
                    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                }

                // Set TLS versions.
                let versions = rustls::ALL_VERSIONS.to_vec();

                // Allow user to have installed a runtime default.
                // If not, we use ring.
                let provider = rustls::crypto::CryptoProvider::get_default()
                    .map(|arc| arc.clone())
                    .unwrap();

                let default_verifier = WebPkiServerVerifier::builder(Arc::new(root_cert_store)).build().unwrap();

                // Build TLS config
                let config_builder = ClientConfig::builder_with_provider(provider)
                    .with_protocol_versions(&versions)
                    .unwrap()
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(CustomCertVerifier { hosts_to_skip_tls_verification: Default::default(), default_verifier }));

                // Finalize TLS config
                // STOPSHIP(charlie): Add `SSL_CERT` thing, it's private though? Ugh.
                let mut tls = config_builder.with_no_client_auth();
                tls.enable_sni = true;

                // ALPN protocol
                tls.alpn_protocols = vec![
                    "http/1.1".into(),
                ];

                tls
            };

            // Configure the builder.
            let client_core = ClientBuilder::new()
                .user_agent(user_agent_string)
                .pool_max_idle_per_host(20)
                .read_timeout(std::time::Duration::from_secs(timeout))
                .use_preconfigured_tls(tls);


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
