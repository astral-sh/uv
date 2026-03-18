use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
use temp_env::async_with_vars;
use url::Url;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_client::RegistryClientBuilder;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::http_util::{
    SelfSigned, generate_self_signed_certs, generate_self_signed_certs_with_ca,
    start_https_mtls_user_agent_server, start_https_user_agent_server, test_cert_dir,
};

/// Test certificate paths and data returned by setup.
struct TestCertificates {
    _temp_dir: tempfile::TempDir,
    standalone_cert: SelfSigned,
    standalone_public_path: PathBuf,
    ca_cert: SelfSigned,
    ca_public_path: PathBuf,
    server_cert: SelfSigned,
    client_combined_path: PathBuf,
}

/// Set up test certificates and return paths.
fn setup_test_certificates() -> Result<TestCertificates> {
    let cert_dir = test_cert_dir();
    fs_err::create_dir_all(&cert_dir)?;
    let temp_dir = tempfile::TempDir::new_in(cert_dir)?;

    // Generate self-signed standalone cert
    let standalone_cert = generate_self_signed_certs()?;
    let standalone_public_path = temp_dir.path().join("standalone_public.pem");

    // Generate self-signed CA, server, and client certs
    let (ca_cert, server_cert, client_cert) = generate_self_signed_certs_with_ca()?;
    let ca_public_path = temp_dir.path().join("ca_public.pem");
    let client_combined_path = temp_dir.path().join("client_combined.pem");

    // Persist the certs
    fs_err::write(&standalone_public_path, standalone_cert.public.pem())?;
    fs_err::write(&ca_public_path, ca_cert.public.pem())?;
    fs_err::write(
        &client_combined_path,
        format!(
            "{}\n{}",
            client_cert.public.pem(),
            client_cert.private.serialize_pem()
        ),
    )?;

    Ok(TestCertificates {
        _temp_dir: temp_dir,
        standalone_cert,
        standalone_public_path,
        ca_cert,
        ca_public_path,
        server_cert,
        client_combined_path,
    })
}

/// Returns the list of SSL-related environment variables to clear (set to `None`).
fn cleared_ssl_vars() -> Vec<(&'static str, Option<&'static str>)> {
    vec![
        (EnvVars::UV_NATIVE_TLS, None),
        (EnvVars::UV_SYSTEM_CERTS, None),
        (EnvVars::SSL_CERT_FILE, None),
        (EnvVars::SSL_CERT_DIR, None),
        (EnvVars::SSL_CLIENT_CERT, None),
    ]
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

/// Assert that a connection error occurred due to TLS issues.
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

/// Assert that the server encountered a "no certificates presented" error.
async fn assert_server_no_cert_error(server_task: tokio::task::JoinHandle<Result<()>>) {
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
}

/// Test that a self-signed server certificate is rejected when no custom certs are configured.
///
/// With all `SSL_CERT_*` variables cleared, the client uses the bundled webpki roots which do
/// not include our test CA.
#[tokio::test]
async fn test_no_custom_certs_rejects_self_signed() -> Result<()> {
    let certs = setup_test_certificates()?;

    async_with_vars(cleared_ssl_vars(), async {
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert_connection_error(&res);

        // The server accepted the TCP connection but the TLS handshake failed on the client
        // side, so the server task may or may not have errored — we just ensure it doesn't panic.
        let _ = server_task.await;
    })
    .await;

    Ok(())
}

/// Test that a server presenting an untrusted certificate is rejected, even when a different
/// custom cert is configured via `SSL_CERT_FILE`.
#[tokio::test]
async fn test_ssl_cert_file_wrong_cert_rejected() -> Result<()> {
    let certs = setup_test_certificates()?;

    // Trust only the standalone cert, but the server presents the CA-signed server cert.
    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.standalone_public_path.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.server_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert_connection_error(&res);

        let _ = server_task.await;
    })
    .await;

    Ok(())
}

/// Test that a nonexistent `SSL_CERT_FILE` is ignored and the client falls back to defaults.
///
/// Since the defaults (webpki roots) don't include our self-signed cert, the connection fails.
#[tokio::test]
async fn test_ssl_cert_file_nonexistent_falls_back() -> Result<()> {
    let certs = setup_test_certificates()?;

    let missing_path = certs._temp_dir.path().join("missing.pem");
    let mut vars = cleared_ssl_vars();
    vars.push((EnvVars::SSL_CERT_FILE, Some(missing_path.to_str().unwrap())));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert_connection_error(&res);

        let _ = server_task.await;
    })
    .await;

    Ok(())
}

/// Test that a valid certificate from `SSL_CERT_FILE` is trusted by the client.
#[tokio::test]
async fn test_ssl_cert_file_valid() -> Result<()> {
    let certs = setup_test_certificates()?;

    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.standalone_public_path.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that a PEM bundle from `SSL_CERT_FILE` is trusted by the client.
#[tokio::test]
async fn test_ssl_cert_file_bundle() -> Result<()> {
    let certs = setup_test_certificates()?;

    let bundle_path = certs._temp_dir.path().join("bundle.pem");
    fs_err::write(
        &bundle_path,
        format!(
            "{}\n{}",
            certs.ca_cert.public.pem(),
            certs.server_cert.public.pem()
        ),
    )?;

    let mut vars = cleared_ssl_vars();
    vars.push((EnvVars::SSL_CERT_FILE, Some(bundle_path.to_str().unwrap())));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.server_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that certificates from both `SSL_CERT_FILE` and `SSL_CERT_DIR` are trusted.
#[tokio::test]
async fn test_ssl_cert_file_and_dir_combined() -> Result<()> {
    let certs = setup_test_certificates()?;

    let second_cert_dir = certs._temp_dir.path().join("second_certs");
    fs_err::create_dir_all(&second_cert_dir)?;
    fs_err::write(
        second_cert_dir.join("second.pem"),
        certs.ca_cert.public.pem(),
    )?;

    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.standalone_public_path.to_str().unwrap()),
    ));
    vars.push((
        EnvVars::SSL_CERT_DIR,
        Some(second_cert_dir.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        // Test with standalone cert (from SSL_CERT_FILE)
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();

        // Test with CA-signed cert (from SSL_CERT_DIR)
        let (server_task, addr) = start_https_user_agent_server(&certs.server_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that PEM bundles in `SSL_CERT_DIR` are loaded correctly.
#[tokio::test]
async fn test_ssl_cert_dir_bundle_files() -> Result<()> {
    let certs = setup_test_certificates()?;

    let bundle_dir = certs._temp_dir.path().join("bundles");
    fs_err::create_dir_all(&bundle_dir)?;

    fs_err::write(
        bundle_dir.join("bundle.pem"),
        format!(
            "{}\n{}",
            certs.standalone_cert.public.pem(),
            certs.ca_cert.public.pem()
        ),
    )?;

    let mut vars = cleared_ssl_vars();
    vars.push((EnvVars::SSL_CERT_DIR, Some(bundle_dir.to_str().unwrap())));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that OpenSSL hash-based filenames in `SSL_CERT_DIR` are loaded correctly.
///
/// The filename `5d30f3c5.3` is not the actual OpenSSL hash of the CA cert — it's an arbitrary
/// name matching the `[hex].[digit]` pattern to verify that such files are loaded from the
/// directory.
#[tokio::test]
async fn test_ssl_cert_dir_hash_named_files() -> Result<()> {
    let certs = setup_test_certificates()?;

    let hash_dir = certs._temp_dir.path().join("hashes");
    fs_err::create_dir_all(&hash_dir)?;
    fs_err::write(hash_dir.join("5d30f3c5.3"), certs.ca_cert.public.pem())?;

    let mut vars = cleared_ssl_vars();
    vars.push((EnvVars::SSL_CERT_DIR, Some(hash_dir.to_str().unwrap())));

    async_with_vars(vars, async {
        let (server_task, addr) = start_https_user_agent_server(&certs.server_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that `SSL_CERT_DIR` supports multiple colon-separated directories.
///
/// Certs are split across two directories; each directory only has one cert, but both servers
/// should be trusted.
#[tokio::test]
async fn test_ssl_cert_dir_multiple_directories() -> Result<()> {
    let certs = setup_test_certificates()?;

    let dir_a = certs._temp_dir.path().join("dir_a");
    let dir_b = certs._temp_dir.path().join("dir_b");
    fs_err::create_dir_all(&dir_a)?;
    fs_err::create_dir_all(&dir_b)?;

    // Standalone cert in dir_a, CA cert in dir_b
    fs_err::write(
        dir_a.join("standalone.pem"),
        certs.standalone_cert.public.pem(),
    )?;
    fs_err::write(dir_b.join("ca.pem"), certs.ca_cert.public.pem())?;

    let combined = std::env::join_paths([&dir_a, &dir_b]).unwrap();

    let mut vars = cleared_ssl_vars();
    vars.push((EnvVars::SSL_CERT_DIR, Some(combined.to_str().unwrap())));

    async_with_vars(vars, async {
        // Server using standalone cert (from dir_a)
        let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();

        // Server using CA-signed cert (from dir_b)
        let (server_task, addr) = start_https_user_agent_server(&certs.server_cert)
            .await
            .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that `SSL_CLIENT_CERT` with invalid content is ignored and mTLS fails.
///
/// The server requires a client certificate, but `SSL_CLIENT_CERT` points to a file with
/// garbage content so no valid identity can be loaded.
#[tokio::test]
async fn test_mtls_with_invalid_client_cert() -> Result<()> {
    let certs = setup_test_certificates()?;

    let invalid_cert_path = certs._temp_dir.path().join("invalid_client.pem");
    fs_err::write(&invalid_cert_path, "not a valid certificate or key")?;

    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.ca_public_path.to_str().unwrap()),
    ));
    vars.push((
        EnvVars::SSL_CLIENT_CERT,
        Some(invalid_cert_path.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        let (server_task, addr) =
            start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert)
                .await
                .unwrap();

        let res = send_request(addr).await;
        assert_connection_error(&res);

        assert_server_no_cert_error(server_task).await;
    })
    .await;

    Ok(())
}

/// Test that mTLS works when `SSL_CLIENT_CERT` is set.
#[tokio::test]
async fn test_mtls_with_client_cert() -> Result<()> {
    let certs = setup_test_certificates()?;

    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.ca_public_path.to_str().unwrap()),
    ));
    vars.push((
        EnvVars::SSL_CLIENT_CERT,
        Some(certs.client_combined_path.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        let (server_task, addr) =
            start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert)
                .await
                .unwrap();

        let res = send_request(addr).await;
        assert!(res.is_ok());

        let _ = server_task.await.unwrap();
    })
    .await;

    Ok(())
}

/// Test that mTLS rejects connections without a client certificate.
#[tokio::test]
async fn test_mtls_without_client_cert() -> Result<()> {
    let certs = setup_test_certificates()?;

    let mut vars = cleared_ssl_vars();
    vars.push((
        EnvVars::SSL_CERT_FILE,
        Some(certs.ca_public_path.to_str().unwrap()),
    ));

    async_with_vars(vars, async {
        let (server_task, addr) =
            start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert)
                .await
                .unwrap();

        let res = send_request(addr).await;
        assert_connection_error(&res);

        assert_server_no_cert_error(server_task).await;
    })
    .await;

    Ok(())
}
