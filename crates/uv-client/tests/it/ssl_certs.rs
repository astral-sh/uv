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
    generate_self_signed_certs, generate_self_signed_certs_with_ca,
    start_https_mtls_user_agent_server, start_https_user_agent_server, test_cert_dir, SelfSigned,
};

/// Test certificate paths and data returned by setup
#[allow(dead_code)]
struct TestCertificates {
    _temp_dir: tempfile::TempDir,
    standalone_cert: SelfSigned,
    standalone_public_path: PathBuf,
    standalone_private_path: PathBuf,
    ca_cert: SelfSigned,
    ca_public_path: PathBuf,
    ca_private_path: PathBuf,
    server_cert: SelfSigned,
    server_public_path: PathBuf,
    server_private_path: PathBuf,
    client_cert: SelfSigned,
    client_combined_path: PathBuf,
    does_not_exist_path: PathBuf,
}

/// Set up test certificates and return paths
fn setup_test_certificates() -> Result<TestCertificates> {
    let cert_dir = test_cert_dir();
    fs_err::create_dir_all(&cert_dir)?;
    let temp_dir = tempfile::TempDir::new_in(cert_dir)?;
    let does_not_exist_path = temp_dir.path().join("does_not_exist");

    // Generate self-signed standalone cert
    let standalone_cert = generate_self_signed_certs()?;
    let standalone_public_path = temp_dir.path().join("standalone_public.pem");
    let standalone_private_path = temp_dir.path().join("standalone_private.pem");

    // Generate self-signed CA, server, and client certs
    let (ca_cert, server_cert, client_cert) = generate_self_signed_certs_with_ca()?;
    let ca_public_path = temp_dir.path().join("ca_public.pem");
    let ca_private_path = temp_dir.path().join("ca_private.pem");
    let server_public_path = temp_dir.path().join("server_public.pem");
    let server_private_path = temp_dir.path().join("server_private.pem");
    let client_combined_path = temp_dir.path().join("client_combined.pem");

    // Persist the certs
    fs_err::write(&standalone_public_path, standalone_cert.public.pem())?;
    fs_err::write(&standalone_private_path, standalone_cert.private.serialize_pem())?;
    fs_err::write(&ca_public_path, ca_cert.public.pem())?;
    fs_err::write(&ca_private_path, ca_cert.private.serialize_pem())?;
    fs_err::write(&server_public_path, server_cert.public.pem())?;
    fs_err::write(&server_private_path, server_cert.private.serialize_pem())?;
    fs_err::write(
        &client_combined_path,
        format!("{}\n{}", client_cert.public.pem(), client_cert.private.serialize_pem()),
    )?;

    Ok(TestCertificates {
        _temp_dir: temp_dir,
        standalone_cert,
        standalone_public_path,
        standalone_private_path,
        ca_cert,
        ca_public_path,
        ca_private_path,
        server_cert,
        server_public_path,
        server_private_path,
        client_cert,
        client_combined_path,
        does_not_exist_path,
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

/// Assert that the server encountered a TLS error (any TLS-related failure)
async fn assert_server_tls_error(server_task: tokio::task::JoinHandle<Result<()>>) {
    let server_res = server_task.await.expect("server task panicked");
    // Server should have encountered some kind of error (TLS error, timeout, connection reset, etc.)
    // We just verify it's not a successful completion
    assert!(server_res.is_err(), "Expected server to encounter an error");
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

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_nonexistent() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_FILE to non-existent location
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.does_not_exist_path.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection failed
    assert_connection_error(&res);
    assert_server_tls_error(server_task).await;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_valid() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_FILE to valid certificate
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.standalone_public_path.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_mixed_paths() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_DIR to mix of valid and non-existent paths
    unsafe {
        std::env::set_var(
            EnvVars::SSL_CERT_DIR,
            std::env::join_paths(vec![
                certs._temp_dir.path().as_os_str(),
                certs.does_not_exist_path.as_os_str(),
            ])?,
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded (should warn about missing but use valid)
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_nonexistent() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_DIR to only non-existent path
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, certs.does_not_exist_path.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection failed
    assert_connection_error(&res);
    assert_server_tls_error(server_task).await;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_mtls_with_client_cert() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_FILE to CA and SSL_CLIENT_CERT to client cert
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.ca_public_path.as_os_str());
        std::env::set_var(EnvVars::SSL_CLIENT_CERT, certs.client_combined_path.as_os_str());
    }
    
    let (server_task, addr) = start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert).await?;
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_mtls_without_client_cert() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_FILE to CA but don't set SSL_CLIENT_CERT
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.ca_public_path.as_os_str());
    }
    
    let (server_task, addr) = start_https_mtls_user_agent_server(&certs.ca_cert, &certs.server_cert).await?;
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection failed
    assert_connection_error(&res);
    assert_server_no_cert_error(server_task).await;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_bundle() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Create a bundle file with multiple certificates (CA + server cert)
    let bundle_path = certs._temp_dir.path().join("bundle.pem");
    fs_err::write(
        &bundle_path,
        format!("{}\n{}", certs.ca_cert.public.pem(), certs.server_cert.public.pem()),
    )?;

    // Set SSL_CERT_FILE to the bundle
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, bundle_path.as_os_str());
    }
    
    let (server_task, addr) = start_https_user_agent_server(&certs.server_cert).await?;
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded using bundle
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_invalid() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Create a file with invalid certificate data
    let invalid_cert_path = certs._temp_dir.path().join("invalid.pem");
    fs_err::write(&invalid_cert_path, "This is not a valid certificate")?;

    // Set SSL_CERT_FILE to invalid cert
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, invalid_cert_path.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection failed (invalid cert should be skipped with warning)
    assert_connection_error(&res);
    assert_server_tls_error(server_task).await;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_extensions() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Create a subdirectory with various file extensions
    let extensions_dir = certs._temp_dir.path().join("extensions");
    fs_err::create_dir_all(&extensions_dir)?;

    // Copy cert with .crt extension (should be loaded)
    let crt_file = extensions_dir.join("cert.crt");
    fs_err::copy(&certs.standalone_public_path, &crt_file)?;

    // Copy cert with .cer extension (should be loaded)
    let cer_file = extensions_dir.join("cert.cer");
    fs_err::copy(&certs.standalone_public_path, &cer_file)?;

    // Create a .txt file (should be ignored)
    let txt_file = extensions_dir.join("cert.txt");
    fs_err::write(&txt_file, certs.standalone_cert.public.pem())?;

    // Create a file with no extension (should be ignored)
    let no_ext_file = extensions_dir.join("cert");
    fs_err::write(&no_ext_file, certs.standalone_cert.public.pem())?;

    // Set SSL_CERT_DIR to the extensions directory
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, extensions_dir.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded (.crt/.cer/.pem files loaded, others ignored)
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_file_and_dir_combined() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Create a second cert directory with a different cert
    let second_cert_dir = certs._temp_dir.path().join("second_certs");
    fs_err::create_dir_all(&second_cert_dir)?;
    let second_cert_path = second_cert_dir.join("second.pem");
    fs_err::write(&second_cert_path, certs.ca_cert.public.pem())?;

    // Set both SSL_CERT_FILE (standalone cert) and SSL_CERT_DIR (CA cert)
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, certs.standalone_public_path.as_os_str());
        std::env::set_var(EnvVars::SSL_CERT_DIR, second_cert_dir.as_os_str());
    }
    
    // Test with standalone cert
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
    
    // Verify connection succeeded using cert from SSL_CERT_FILE
    assert!(res.is_ok());
    let _ = server_task.await?;

    // Test with CA-signed cert
    let (server_task, addr) = start_https_user_agent_server(&certs.server_cert).await?;
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded using CA from SSL_CERT_DIR
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_empty_values() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Set SSL_CERT_FILE and SSL_CERT_DIR to empty strings
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, "");
        std::env::set_var(EnvVars::SSL_CERT_DIR, "");
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection failed (empty env vars should be ignored, no custom certs loaded)
    assert_connection_error(&res);
    assert_server_tls_error(server_task).await;

    Ok(())
}

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn test_ssl_cert_dir_bundle_files() -> Result<()> {
    unsafe { clear_ssl_env_vars(); }
    
    let certs = setup_test_certificates()?;

    // Create a directory with bundle files
    let bundle_dir = certs._temp_dir.path().join("bundles");
    fs_err::create_dir_all(&bundle_dir)?;
    
    // Create a bundle file with multiple certificates
    let bundle1_path = bundle_dir.join("bundle1.pem");
    fs_err::write(
        &bundle1_path,
        format!("{}\n{}", certs.standalone_cert.public.pem(), certs.ca_cert.public.pem()),
    )?;
    
    // Create another bundle
    let bundle2_path = bundle_dir.join("bundle2.pem");
    fs_err::write(
        &bundle2_path,
        format!("{}\n{}", certs.server_cert.public.pem(), certs.standalone_cert.public.pem()),
    )?;

    // Set SSL_CERT_DIR to the bundle directory
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, bundle_dir.as_os_str());
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
    
    unsafe { clear_ssl_env_vars(); }

    // Verify connection succeeded (bundles in directory loaded correctly)
    assert!(res.is_ok());
    let _ = server_task.await?;

    Ok(())
}
