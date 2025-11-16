use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::future;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::USER_AGENT;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType, date_time_ymd,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use uv_fs::Simplified;

/// An issued certificate, together with the subject keypair.
#[derive(Debug)]
pub(crate) struct SelfSigned {
    /// An issued certificate.
    pub public: Certificate,
    /// The certificate's subject signing key.
    pub private: KeyPair,
}

/// Defines the base location for temporary generated certs.
///
/// See [`TestContext::test_bucket_dir`] for implementation rationale.
pub(crate) fn test_cert_dir() -> PathBuf {
    std::env::temp_dir()
        .simple_canonicalize()
        .expect("failed to canonicalize temp dir")
        .join("uv")
        .join("tests")
        .join("certs")
}

/// Generates a self-signed server certificate for `uv-test-server`, `localhost` and `127.0.0.1`.
/// This certificate is standalone and not issued by a self-signed Root CA.
///
/// Use sparingly as generation of certs is a slow operation.
pub(crate) fn generate_self_signed_certs() -> Result<SelfSigned> {
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::NoCa;
    params.not_before = date_time_ymd(1975, 1, 1);
    params.not_after = date_time_ymd(4096, 1, 1);
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params.key_usages.push(KeyUsagePurpose::KeyEncipherment);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Astral Software Inc.");
    params
        .distinguished_name
        .push(DnType::CommonName, "uv-test-server");
    params
        .subject_alt_names
        .push(SanType::DnsName("uv-test-server".try_into()?));
    params
        .subject_alt_names
        .push(SanType::DnsName("localhost".try_into()?));
    params
        .subject_alt_names
        .push(SanType::IpAddress("127.0.0.1".parse()?));
    let private = KeyPair::generate()?;
    let public = params.self_signed(&private)?;

    Ok(SelfSigned { public, private })
}

/// Generates a self-signed root CA, server certificate, and client certificate.
/// There are no intermediate certs generated as part of this function.
/// The server certificate is for `uv-test-server`, `localhost` and `127.0.0.1` issued by this CA.
/// The client certificate is for `uv-test-client` issued by this CA.
///
/// Use sparingly as generation of these certs is a very slow operation.
pub(crate) fn generate_self_signed_certs_with_ca() -> Result<(SelfSigned, SelfSigned, SelfSigned)> {
    // Generate the CA
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained); // root cert
    ca_params.not_before = date_time_ymd(1975, 1, 1);
    ca_params.not_after = date_time_ymd(4096, 1, 1);
    ca_params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    ca_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
    ca_params.key_usages.push(KeyUsagePurpose::CrlSign);
    ca_params
        .distinguished_name
        .push(DnType::OrganizationName, "Astral Software Inc.");
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "uv-test-ca");
    ca_params
        .subject_alt_names
        .push(SanType::DnsName("uv-test-ca".try_into()?));
    let ca_private_key = KeyPair::generate()?;
    let ca_public_cert = ca_params.self_signed(&ca_private_key)?;
    let ca_cert_issuer = Issuer::new(ca_params, &ca_private_key);

    // Generate server cert issued by this CA
    let mut server_params = CertificateParams::default();
    server_params.is_ca = IsCa::NoCa;
    server_params.not_before = date_time_ymd(1975, 1, 1);
    server_params.not_after = date_time_ymd(4096, 1, 1);
    server_params.use_authority_key_identifier_extension = true;
    server_params
        .key_usages
        .push(KeyUsagePurpose::DigitalSignature);
    server_params
        .key_usages
        .push(KeyUsagePurpose::KeyEncipherment);
    server_params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    server_params
        .distinguished_name
        .push(DnType::OrganizationName, "Astral Software Inc.");
    server_params
        .distinguished_name
        .push(DnType::CommonName, "uv-test-server");
    server_params
        .subject_alt_names
        .push(SanType::DnsName("uv-test-server".try_into()?));
    server_params
        .subject_alt_names
        .push(SanType::DnsName("localhost".try_into()?));
    server_params
        .subject_alt_names
        .push(SanType::IpAddress("127.0.0.1".parse()?));
    let server_private_key = KeyPair::generate()?;
    let server_public_cert = server_params.signed_by(&server_private_key, &ca_cert_issuer)?;

    // Generate client cert issued by this CA
    let mut client_params = CertificateParams::default();
    client_params.is_ca = IsCa::NoCa;
    client_params.not_before = date_time_ymd(1975, 1, 1);
    client_params.not_after = date_time_ymd(4096, 1, 1);
    client_params.use_authority_key_identifier_extension = true;
    client_params
        .key_usages
        .push(KeyUsagePurpose::DigitalSignature);
    client_params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ClientAuth);
    client_params
        .distinguished_name
        .push(DnType::OrganizationName, "Astral Software Inc.");
    client_params
        .distinguished_name
        .push(DnType::CommonName, "uv-test-client");
    client_params
        .subject_alt_names
        .push(SanType::DnsName("uv-test-client".try_into()?));
    let client_private_key = KeyPair::generate()?;
    let client_public_cert = client_params.signed_by(&client_private_key, &ca_cert_issuer)?;

    let ca_self_signed = SelfSigned {
        public: ca_public_cert,
        private: ca_private_key,
    };
    let server_self_signed = SelfSigned {
        public: server_public_cert,
        private: server_private_key,
    };
    let client_self_signed = SelfSigned {
        public: client_public_cert,
        private: client_private_key,
    };

    Ok((ca_self_signed, server_self_signed, client_self_signed))
}

// Plain is fine for now; Arc/Box could be used later if we need to support move.
type ServerSvcFn =
    fn(
        Request<Incoming>,
    ) -> future::Ready<Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>>;

#[derive(Default)]
pub(crate) struct TestServerBuilder<'a> {
    // Custom server response function
    svc_fn: Option<ServerSvcFn>,
    // CA certificate
    ca_cert: Option<&'a SelfSigned>,
    // Server certificate
    server_cert: Option<&'a SelfSigned>,
    // Enable mTLS Verification
    mutual_tls: bool,
}

impl<'a> TestServerBuilder<'a> {
    pub(crate) fn new() -> Self {
        Self {
            svc_fn: None,
            server_cert: None,
            ca_cert: None,
            mutual_tls: false,
        }
    }

    #[expect(unused)]
    /// Provide a custom server response function.
    pub(crate) fn with_svc_fn(mut self, svc_fn: ServerSvcFn) -> Self {
        self.svc_fn = Some(svc_fn);
        self
    }

    /// Provide the server certificate. This will enable TLS (HTTPS).
    pub(crate) fn with_server_cert(mut self, server_cert: &'a SelfSigned) -> Self {
        self.server_cert = Some(server_cert);
        self
    }

    /// CA certificate used to build the `RootCertStore` for client verification.
    /// Requires `with_server_cert`.
    pub(crate) fn with_ca_cert(mut self, ca_cert: &'a SelfSigned) -> Self {
        self.ca_cert = Some(ca_cert);
        self
    }

    /// Enforce mutual TLS (client cert auth).
    /// Requires `with_server_cert` and `with_ca_cert`.
    pub(crate) fn with_mutual_tls(mut self, mutual: bool) -> Self {
        self.mutual_tls = mutual;
        self
    }

    /// Starts the HTTP(S) server with optional mTLS enforcement.
    pub(crate) async fn start(self) -> Result<(JoinHandle<Result<()>>, SocketAddr)> {
        // Validate builder input combinations
        if self.ca_cert.is_some() && self.server_cert.is_none() {
            anyhow::bail!("server certificate is required when CA certificate is provided");
        }
        if self.mutual_tls && (self.ca_cert.is_none() || self.server_cert.is_none()) {
            anyhow::bail!("ca certificate is required for mTLS");
        }

        // Set up the TCP listener on a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        // Setup TLS Config (if any)
        let tls_acceptor = if let Some(server_cert) = self.server_cert {
            // Prepare Server Cert and KeyPair
            let server_key = PrivateKeyDer::try_from(server_cert.private.serialize_der()).unwrap();
            let server_cert = vec![CertificateDer::from(server_cert.public.der().to_vec())];

            // Setup CA Verifier
            let client_verifier = if let Some(ca_cert) = self.ca_cert {
                let mut root_store = RootCertStore::empty();
                root_store
                    .add(CertificateDer::from(ca_cert.public.der().to_vec()))
                    .expect("failed to add CA cert");
                if self.mutual_tls {
                    // Setup mTLS CA config
                    WebPkiClientVerifier::builder(root_store.into())
                        .build()
                        .expect("failed to setup client verifier")
                } else {
                    // Only load the CA roots
                    WebPkiClientVerifier::builder(root_store.into())
                        .allow_unauthenticated()
                        .build()
                        .expect("failed to setup client verifier")
                }
            } else {
                WebPkiClientVerifier::no_client_auth()
            };

            let mut tls_config = ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(server_cert, server_key)?;
            tls_config.alpn_protocols = vec![b"http/1.1".to_vec(), b"http/1.0".to_vec()];

            Some(TlsAcceptor::from(Arc::new(tls_config)))
        } else {
            None
        };

        // Setup Response Handler
        let svc_fn = if let Some(custom_svc_fn) = self.svc_fn {
            custom_svc_fn
        } else {
            |req: Request<Incoming>| {
                // Get User Agent Header and send it back in the response
                let user_agent = req
                    .headers()
                    .get(USER_AGENT)
                    .and_then(|v| v.to_str().ok())
                    .map(ToString::to_string)
                    .unwrap_or_default(); // Empty Default
                let response_content = Full::new(Bytes::from(user_agent))
                    .map_err(|_| unreachable!())
                    .boxed();
                // If we ever want a true echo server, we can use instead
                // let response_content = req.into_body().boxed();
                // although uv-client doesn't expose post currently.
                future::ok::<_, hyper::Error>(Response::new(response_content))
            }
        };

        // Spawn the server loop in a background task
        let server_task = tokio::spawn(async move {
            let svc = service_fn(move |req: Request<Incoming>| svc_fn(req));

            let (tcp_stream, _remote_addr) = listener
                .accept()
                .await
                .context("Failed to accept TCP connection")?;

            // Start Server (not wrapped in loop {} since we want a single response server)
            // If we want server to accept multiple connections, we can wrap it in loop {}
            // but we'll need to ensure to handle termination signals in the tests otherwise
            // it may never stop.
            if let Some(tls_acceptor) = tls_acceptor {
                let tls_stream = tls_acceptor
                    .accept(tcp_stream)
                    .await
                    .context("Failed to accept TLS connection")?;
                let socket = TokioIo::new(tls_stream);
                tokio::task::spawn(async move {
                    Builder::new(TokioExecutor::new())
                        .serve_connection(socket, svc)
                        .await
                        .expect("HTTPS Server Started");
                });
            } else {
                let socket = TokioIo::new(tcp_stream);
                tokio::task::spawn(async move {
                    Builder::new(TokioExecutor::new())
                        .serve_connection(socket, svc)
                        .await
                        .expect("HTTP Server Started");
                });
            }

            Ok(())
        });

        Ok((server_task, addr))
    }
}

/// Single Request HTTP server that echoes the User Agent Header.
pub(crate) async fn start_http_user_agent_server() -> Result<(JoinHandle<Result<()>>, SocketAddr)> {
    TestServerBuilder::new().start().await
}

/// Single Request HTTPS server that echoes the User Agent Header.
pub(crate) async fn start_https_user_agent_server(
    server_cert: &SelfSigned,
) -> Result<(JoinHandle<Result<()>>, SocketAddr)> {
    TestServerBuilder::new()
        .with_server_cert(server_cert)
        .start()
        .await
}

/// Single Request HTTPS mTLS server that echoes the User Agent Header.
pub(crate) async fn start_https_mtls_user_agent_server(
    ca_cert: &SelfSigned,
    server_cert: &SelfSigned,
) -> Result<(JoinHandle<Result<()>>, SocketAddr)> {
    TestServerBuilder::new()
        .with_ca_cert(ca_cert)
        .with_server_cert(server_cert)
        .with_mutual_tls(true)
        .start()
        .await
}
