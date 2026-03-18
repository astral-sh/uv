use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use temp_env::async_with_vars;
use tempfile::{NamedTempFile, TempDir};
use url::Url;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_client::RegistryClientBuilder;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::http_util::{
    SelfSigned, generate_self_signed_certs_with_ca, start_https_mtls_user_agent_server,
    start_https_user_agent_server, test_cert_dir,
};

/// A self-signed CA together with a server certificate and a client certificate
/// it has issued.  Every [`TestCertificate`] is an independent trust domain.
struct TestCertificate {
    _temp_dir: TempDir,
    /// The CA certificate (root of trust).
    ca: SelfSigned,
    /// A server certificate signed by [`ca`](Self::ca).
    server: SelfSigned,
    /// Path to the CA public cert PEM — the file you put in `SSL_CERT_FILE` to
    /// trust this certificate family.
    trust_path: PathBuf,
    /// Path to the combined client cert + key PEM — the file you put in
    /// `SSL_CLIENT_CERT` for mTLS.
    client_cert_path: PathBuf,
}

impl TestCertificate {
    /// Generate a fresh CA, server cert, and client cert, persisting the
    /// relevant PEM files to a temporary directory.
    fn new() -> Result<Self> {
        let cert_dir = test_cert_dir();
        fs_err::create_dir_all(&cert_dir)?;
        let temp_dir = TempDir::new_in(cert_dir)?;

        let (ca, server, client) = generate_self_signed_certs_with_ca()?;

        let trust_path = temp_dir.path().join("ca.pem");
        fs_err::write(&trust_path, ca.public.pem())?;

        let client_cert_path = temp_dir.path().join("client.pem");
        fs_err::write(
            &client_cert_path,
            format!(
                "{}\n{}",
                client.public.pem(),
                client.private.serialize_pem()
            ),
        )?;

        Ok(Self {
            _temp_dir: temp_dir,
            ca,
            server,
            trust_path,
            client_cert_path,
        })
    }

    /// Write a CA + server PEM bundle to a [`NamedTempFile`].
    fn write_bundle_pem(&self) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "{}\n{}",
            self.ca.public.pem(),
            self.server.public.pem()
        )
        .unwrap();
        file
    }

    /// Write the CA public PEM into a fresh temporary directory, returning it.
    fn ca_pem_dir(&self) -> TempDir {
        self.ca_pem_dir_as("ca.pem")
    }

    /// Write the CA public PEM with a custom filename into a fresh temporary
    /// directory, returning it.
    fn ca_pem_dir_as(&self, filename: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs_err::write(dir.path().join(filename), self.ca.public.pem()).unwrap();
        dir
    }

    /// Write a CA + server PEM bundle into a fresh temporary directory,
    /// returning it.
    fn bundle_pem_dir(&self) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs_err::write(
            dir.path().join("bundle.pem"),
            format!("{}\n{}", self.ca.public.pem(), self.server.public.pem()),
        )
        .unwrap();
        dir
    }
}

/// Client-side configuration builder.  Collects environment variable overrides
/// and provides terminal assertion methods that start a server, send a request,
/// and verify the outcome.
struct TestClient {
    overrides: Vec<(&'static str, String)>,
}

/// Create a [`TestClient`] with no environment overrides.
fn client() -> TestClient {
    TestClient {
        overrides: Vec::new(),
    }
}

impl TestClient {
    /// Set `SSL_CERT_FILE` to `path`.
    fn ssl_cert_file(self, path: &Path) -> Self {
        self.with_env(EnvVars::SSL_CERT_FILE, path.to_str().unwrap())
    }

    /// Set `SSL_CERT_DIR` to a single directory.
    fn ssl_cert_dir(self, path: &Path) -> Self {
        self.with_env(EnvVars::SSL_CERT_DIR, path.to_str().unwrap())
    }

    /// Set `SSL_CERT_DIR` to multiple directories joined with the
    /// platform-specific path separator.
    fn ssl_cert_dirs(self, paths: &[&Path]) -> Self {
        let joined = std::env::join_paths(paths).unwrap();
        self.with_env(EnvVars::SSL_CERT_DIR, joined.to_str().unwrap())
    }

    /// Set `SSL_CLIENT_CERT` to `path`.
    fn ssl_client_cert(self, path: &Path) -> Self {
        self.with_env(EnvVars::SSL_CLIENT_CERT, path.to_str().unwrap())
    }

    /// Set an arbitrary environment variable.
    fn with_env(mut self, key: &'static str, value: &str) -> Self {
        self.overrides.push((key, value.to_string()));
        self
    }

    /// Assert that an HTTPS connection to `cert`'s server succeeds.
    async fn expect_https_connect_succeeds(&self, cert: &TestCertificate) {
        self.run_https(cert, |response, server_task| async move {
            assert!(
                response.is_ok(),
                "expected successful response, got: {:?}",
                response.err()
            );
            server_task.await.unwrap().unwrap();
        })
        .await;
    }

    /// Assert that an HTTPS connection to `cert`'s server fails with a TLS
    /// error on the client side.
    async fn expect_https_connect_fails(&self, cert: &TestCertificate) {
        self.run_https(cert, |response, server_task| async move {
            assert_connection_error(&response);
            // Server may or may not have errored — just ensure no panic.
            let _ = server_task.await;
        })
        .await;
    }

    /// Assert that an mTLS connection to `cert`'s server succeeds.
    async fn expect_mtls_connect_succeeds(&self, cert: &TestCertificate) {
        self.run_mtls(cert, |response, server_task| async move {
            assert!(
                response.is_ok(),
                "expected successful response, got: {:?}",
                response.err()
            );
            server_task.await.unwrap().unwrap();
        })
        .await;
    }

    /// Assert that an mTLS connection to `cert`'s server fails because no
    /// valid client certificate was presented.
    async fn expect_mtls_connect_fails(&self, cert: &TestCertificate) {
        self.run_mtls(cert, |response, server_task| async move {
            assert_connection_error(&response);

            let server_res = server_task.await.expect("server task panicked");
            let Err(anyhow_err) = server_res else {
                panic!("expected server error, got Ok");
            };
            let Some(io_err) = anyhow_err.downcast_ref::<std::io::Error>() else {
                panic!("expected io::Error, got: {anyhow_err}");
            };
            let Some(wrapped_err) = io_err.get_ref() else {
                panic!("expected wrapped error in io::Error, got: {io_err}");
            };
            let Some(tls_err) = wrapped_err.downcast_ref::<rustls::Error>() else {
                panic!("expected rustls::Error, got: {wrapped_err}");
            };
            assert!(
                matches!(tls_err, rustls::Error::NoCertificatesPresented),
                "expected NoCertificatesPresented, got: {tls_err}"
            );
        })
        .await;
    }

    /// Build the full environment variable list: clear all SSL-related
    /// variables, then apply the accumulated overrides.
    fn ssl_vars(&self) -> Vec<(&'static str, Option<&str>)> {
        let mut vars: Vec<(&'static str, Option<&str>)> = vec![
            (EnvVars::UV_NATIVE_TLS, None),
            (EnvVars::UV_SYSTEM_CERTS, None),
            (EnvVars::SSL_CERT_FILE, None),
            (EnvVars::SSL_CERT_DIR, None),
            (EnvVars::SSL_CLIENT_CERT, None),
        ];
        vars.extend(self.overrides.iter().map(|(k, v)| (*k, Some(v.as_str()))));
        vars
    }

    /// Start an HTTPS server, send a request inside `async_with_vars`, and
    /// hand the response + server task to `check`.
    async fn run_https<F, Fut>(&self, cert: &TestCertificate, check: F)
    where
        F: FnOnce(
            Result<reqwest::Response, reqwest_middleware::Error>,
            tokio::task::JoinHandle<Result<()>>,
        ) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let vars = self.ssl_vars();
        async_with_vars(vars, async {
            let (server_task, addr) = start_https_user_agent_server(&cert.server).await.unwrap();
            let response = send_request(addr).await;
            check(response, server_task).await;
        })
        .await;
    }

    /// Start an mTLS server, send a request inside `async_with_vars`, and
    /// hand the response + server task to `check`.
    async fn run_mtls<F, Fut>(&self, cert: &TestCertificate, check: F)
    where
        F: FnOnce(
            Result<reqwest::Response, reqwest_middleware::Error>,
            tokio::task::JoinHandle<Result<()>>,
        ) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let vars = self.ssl_vars();
        async_with_vars(vars, async {
            let (server_task, addr) = start_https_mtls_user_agent_server(&cert.ca, &cert.server)
                .await
                .unwrap();
            let response = send_request(addr).await;
            check(response, server_task).await;
        })
        .await;
    }
}

/// Send a GET request to the given server address using a fresh registry client.
async fn send_request(addr: SocketAddr) -> Result<reqwest::Response, reqwest_middleware::Error> {
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}")).unwrap();
    let cache = Cache::temp().unwrap().init().await.unwrap();
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();
    client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await
}

/// Assert that a request result is a TLS connection error.
fn assert_connection_error(res: &Result<reqwest::Response, reqwest_middleware::Error>) {
    let Some(reqwest_middleware::Error::Middleware(middleware_error)) = res.as_ref().err() else {
        panic!("expected middleware error, got: {res:?}");
    };
    let reqwest_error = middleware_error
        .chain()
        .find_map(|err| {
            err.downcast_ref::<reqwest_middleware::Error>().map(|err| {
                if let reqwest_middleware::Error::Reqwest(inner) = err {
                    inner
                } else {
                    panic!("expected reqwest error, got: {err}")
                }
            })
        })
        .expect("expected reqwest error");
    assert!(reqwest_error.is_connect());
}

/// A self-signed server certificate is rejected when no custom certs are
/// configured — the bundled webpki roots don't include our test CA.
#[tokio::test]
async fn test_no_custom_certs_rejects_self_signed() -> Result<()> {
    let cert = TestCertificate::new()?;
    client().expect_https_connect_fails(&cert).await;
    Ok(())
}

/// Trusting cert A does not let you connect to a server presenting cert B.
#[tokio::test]
async fn test_ssl_cert_file_wrong_cert_rejected() -> Result<()> {
    let cert_a = TestCertificate::new()?;
    let cert_b = TestCertificate::new()?;
    client()
        .ssl_cert_file(&cert_a.trust_path)
        .expect_https_connect_fails(&cert_b)
        .await;
    Ok(())
}

/// A nonexistent `SSL_CERT_FILE` is ignored; the client falls back to webpki
/// roots which don't include our test CA.
#[tokio::test]
async fn test_ssl_cert_file_nonexistent_falls_back() -> Result<()> {
    let cert = TestCertificate::new()?;
    let dir = TempDir::new()?;
    let missing = dir.path().join("missing.pem");
    client()
        .ssl_cert_file(&missing)
        .expect_https_connect_fails(&cert)
        .await;
    Ok(())
}

/// A valid `SSL_CERT_FILE` pointing to the server's CA cert is trusted.
#[tokio::test]
async fn test_ssl_cert_file_valid() -> Result<()> {
    let cert = TestCertificate::new()?;
    client()
        .ssl_cert_file(&cert.trust_path)
        .expect_https_connect_succeeds(&cert)
        .await;
    Ok(())
}

/// A PEM bundle containing multiple certificates in `SSL_CERT_FILE` is loaded.
#[tokio::test]
async fn test_ssl_cert_file_bundle() -> Result<()> {
    let cert = TestCertificate::new()?;
    let bundle = cert.write_bundle_pem();
    client()
        .ssl_cert_file(bundle.path())
        .expect_https_connect_succeeds(&cert)
        .await;
    Ok(())
}

/// Certificates from both `SSL_CERT_FILE` and `SSL_CERT_DIR` are trusted.
#[tokio::test]
async fn test_ssl_cert_file_and_dir_combined() -> Result<()> {
    let cert_a = TestCertificate::new()?;
    let cert_b = TestCertificate::new()?;

    let dir = cert_b.ca_pem_dir();
    let c = client()
        .ssl_cert_file(&cert_a.trust_path)
        .ssl_cert_dir(dir.path());
    c.expect_https_connect_succeeds(&cert_a).await;
    c.expect_https_connect_succeeds(&cert_b).await;
    Ok(())
}

/// PEM bundles inside `SSL_CERT_DIR` are loaded correctly.
#[tokio::test]
async fn test_ssl_cert_dir_bundle_files() -> Result<()> {
    let cert = TestCertificate::new()?;
    let dir = cert.bundle_pem_dir();
    client()
        .ssl_cert_dir(dir.path())
        .expect_https_connect_succeeds(&cert)
        .await;
    Ok(())
}

/// OpenSSL hash-based filenames in `SSL_CERT_DIR` are loaded correctly.
///
/// The filename `5d30f3c5.3` is not the actual OpenSSL hash of the CA cert —
/// it's an arbitrary name matching the `[hex].[digit]` pattern to verify that
/// such files are loaded from the directory.
#[tokio::test]
async fn test_ssl_cert_dir_hash_named_files() -> Result<()> {
    let cert = TestCertificate::new()?;
    let dir = cert.ca_pem_dir_as("5d30f3c5.3");
    client()
        .ssl_cert_dir(dir.path())
        .expect_https_connect_succeeds(&cert)
        .await;
    Ok(())
}

/// `SSL_CERT_DIR` supports multiple colon-separated directories.  Certs are
/// split across two directories; each only has one cert, but both are trusted.
#[tokio::test]
async fn test_ssl_cert_dir_multiple_directories() -> Result<()> {
    let cert_a = TestCertificate::new()?;
    let cert_b = TestCertificate::new()?;

    let dir_a = cert_a.ca_pem_dir();
    let dir_b = cert_b.ca_pem_dir();
    let c = client().ssl_cert_dirs(&[dir_a.path(), dir_b.path()]);
    c.expect_https_connect_succeeds(&cert_a).await;
    c.expect_https_connect_succeeds(&cert_b).await;
    Ok(())
}

/// `SSL_CLIENT_CERT` with invalid content is ignored and the mTLS server
/// rejects the connection.
#[tokio::test]
async fn test_mtls_with_invalid_client_cert() -> Result<()> {
    let cert = TestCertificate::new()?;

    let mut invalid = NamedTempFile::new()?;
    write!(invalid, "not a valid certificate or key")?;

    client()
        .ssl_cert_file(&cert.trust_path)
        .ssl_client_cert(invalid.path())
        .expect_mtls_connect_fails(&cert)
        .await;
    Ok(())
}

/// mTLS succeeds when `SSL_CLIENT_CERT` contains a valid client certificate
/// and key.
#[tokio::test]
async fn test_mtls_with_client_cert() -> Result<()> {
    let cert = TestCertificate::new()?;
    client()
        .ssl_cert_file(&cert.trust_path)
        .ssl_client_cert(&cert.client_cert_path)
        .expect_mtls_connect_succeeds(&cert)
        .await;
    Ok(())
}

/// mTLS rejects connections when no client certificate is presented.
#[tokio::test]
async fn test_mtls_without_client_cert() -> Result<()> {
    let cert = TestCertificate::new()?;
    client()
        .ssl_cert_file(&cert.trust_path)
        .expect_mtls_connect_fails(&cert)
        .await;
    Ok(())
}
