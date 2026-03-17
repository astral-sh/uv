use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Result;
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

/// Test certificate paths and data returned by setup
struct TestCertificates {
    _temp_dir: tempfile::TempDir,
    standalone_cert: SelfSigned,
    standalone_public_path: PathBuf,
    ca_cert: SelfSigned,
    ca_public_path: PathBuf,
    server_cert: SelfSigned,
    client_combined_path: PathBuf,
}

/// Set up test certificates and return paths
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

/// Clear all SSL-related environment variables
#[allow(unsafe_code)]
unsafe fn clear_ssl_env_vars() {
    unsafe {
        std::env::remove_var(EnvVars::UV_NATIVE_TLS);
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
        std::env::remove_var(EnvVars::SSL_CERT_DIR);
        std::env::remove_var(EnvVars::SSL_CLIENT_CERT);
    }
}

/// Assert that a connection error occurred due to TLS issues
fn assert_connection_error(res: &Result<reqwest::Response, reqwest_middleware::Error>) {
    let Some(reqwest_middleware::Error::Middleware(middleware_error)) = res.as_ref().err() else {
        panic!("expected middleware error");
    };
    let reqwest_error = middleware_error
        .chain()
        .find_map(|err| {
            err.downcast_ref::<reqwest_middleware::Error>().map(|err| {
                if let reqwest_middleware::Error::Reqwest(inner) = err {
                    inner
                } else {
                    panic!("expected reqwest error")
                }
            })
        })
        .expect("expected reqwest error");
    assert!(reqwest_error.is_connect());
}

/// Assert that the server encountered a "no certificates presented" error
async fn assert_server_no_cert_error(server_task: tokio::task::JoinHandle<Result<()>>) {
    let server_res = server_task.await.expect("server task panicked");
    let expected_err = if let Err(anyhow_err) = server_res
        && let Some(io_err) = anyhow_err.downcast_ref::<std::io::Error>()
        && let Some(wrapped_err) = io_err.get_ref()
        && let Some(tls_err) = wrapped_err.downcast_ref::<rustls::Error>()
        && matches!(tls_err, rustls::Error::NoCertificatesPresented)
    {
        true
    } else {
        false
    };
    assert!(expected_err);
}

/// Test that a valid certificate from `SSL_CERT_FILE` is trusted by the client.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_valid() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

    let certs = setup_test_certificates()?;

    unsafe {
        std::env::set_var(
            EnvVars::SSL_CERT_FILE,
            certs.standalone_public_path.as_os_str(),
        );
    }

    let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache).build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that a PEM bundle from `SSL_CERT_FILE` is trusted by the client.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_bundle() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

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

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, bundle_path.as_os_str());
    }

    let (server_task, addr) = start_https_user_agent_server(&certs.server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that certificates from both `SSL_CERT_FILE` and `SSL_CERT_DIR` are trusted.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_and_dir_combined() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

    let certs = setup_test_certificates()?;

    let second_cert_dir = certs._temp_dir.path().join("second_certs");
    fs_err::create_dir_all(&second_cert_dir)?;
    fs_err::write(
        second_cert_dir.join("second.pem"),
        certs.ca_cert.public.pem(),
    )?;

    unsafe {
        std::env::set_var(
            EnvVars::SSL_CERT_FILE,
            certs.standalone_public_path.as_os_str(),
        );
        std::env::set_var(EnvVars::SSL_CERT_DIR, second_cert_dir.as_os_str());
    }

    // Test with standalone cert (from SSL_CERT_FILE)
    let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    assert!(res.is_ok());
    let _ = server_task.await?;

    // Test with CA-signed cert (from SSL_CERT_DIR)
    let (server_task, addr) = start_https_user_agent_server(&certs.server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that PEM bundles in `SSL_CERT_DIR` are loaded correctly.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_bundle_files() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

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

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, bundle_dir.as_os_str());
    }

    let (server_task, addr) = start_https_user_agent_server(&certs.standalone_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that OpenSSL hash-based filenames in `SSL_CERT_DIR` are loaded correctly.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_hash_named_files() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

    let certs = setup_test_certificates()?;

    let hash_dir = certs._temp_dir.path().join("hashes");
    fs_err::create_dir_all(&hash_dir)?;
    fs_err::write(hash_dir.join("5d30f3c5.3"), certs.ca_cert.public.pem())?;

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, hash_dir.as_os_str());
    }

    let (server_task, addr) = start_https_user_agent_server(&certs.server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that mTLS works when `SSL_CLIENT_CERT` is set.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_mtls_with_client_cert() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

    let certs = setup_test_certificates()?;

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.ca_public_path.as_os_str());
        std::env::set_var(
            EnvVars::SSL_CLIENT_CERT,
            certs.client_combined_path.as_os_str(),
        );
    }

    let (server_task, addr) =
        start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

/// Test that mTLS rejects connections without a client certificate.
// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_mtls_without_client_cert() -> Result<()> {
    unsafe {
        clear_ssl_env_vars();
    }

    let certs = setup_test_certificates()?;

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.ca_public_path.as_os_str());
    }

    let (server_task, addr) =
        start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init().await?;
    let client =
        RegistryClientBuilder::new(BaseClientBuilder::default().no_retry_delay(true), cache)
            .build();

    let res = client
        .cached_client()
        .uncached()
        .for_host(&url)
        .get(Url::from(url))
        .send()
        .await;

    unsafe {
        clear_ssl_env_vars();
    }

    assert_connection_error(&res);
    assert_server_no_cert_error(server_task).await;

    Ok(())
}
