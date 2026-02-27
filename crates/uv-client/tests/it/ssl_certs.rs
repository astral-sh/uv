use std::str::FromStr;

use anyhow::Result;
use rustls::AlertDescription;
use url::Url;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_client::RegistryClientBuilder;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::http_util::{
    generate_self_signed_certs, generate_self_signed_certs_with_ca,
    start_https_mtls_user_agent_server, start_https_user_agent_server, test_cert_dir,
};

// SAFETY: This test is meant to run with single thread configuration
#[tokio::test]
#[allow(unsafe_code)]
async fn ssl_env_vars() -> Result<()> {
    // Ensure our environment is not polluted with anything that may affect `rustls-native-certs`
    unsafe {
        std::env::remove_var(EnvVars::UV_NATIVE_TLS);
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
        std::env::remove_var(EnvVars::SSL_CERT_DIR);
        std::env::remove_var(EnvVars::SSL_CLIENT_CERT);
    }

    // Create temporary cert dirs
    let cert_dir = test_cert_dir();
    fs_err::create_dir_all(&cert_dir).expect("Failed to create test cert bucket");
    let cert_dir =
        tempfile::TempDir::new_in(cert_dir).expect("Failed to create test cert directory");
    let does_not_exist_cert_dir = cert_dir.path().join("does_not_exist");

    // Generate self-signed standalone cert
    let standalone_server_cert = generate_self_signed_certs()?;
    let standalone_public_pem_path = cert_dir.path().join("standalone_public.pem");
    let standalone_private_pem_path = cert_dir.path().join("standalone_private.pem");

    // Generate self-signed CA, server, and client certs
    let (ca_cert, server_cert, client_cert) = generate_self_signed_certs_with_ca()?;
    let ca_public_pem_path = cert_dir.path().join("ca_public.pem");
    let ca_private_pem_path = cert_dir.path().join("ca_private.pem");
    let server_public_pem_path = cert_dir.path().join("server_public.pem");
    let server_private_pem_path = cert_dir.path().join("server_private.pem");
    let client_combined_pem_path = cert_dir.path().join("client_combined.pem");

    // Persist the certs in PKCS8 format as the env vars expect a path on disk
    fs_err::write(
        standalone_public_pem_path.as_path(),
        standalone_server_cert.public.pem(),
    )?;
    fs_err::write(
        standalone_private_pem_path.as_path(),
        standalone_server_cert.private.serialize_pem(),
    )?;
    fs_err::write(ca_public_pem_path.as_path(), ca_cert.public.pem())?;
    fs_err::write(
        ca_private_pem_path.as_path(),
        ca_cert.private.serialize_pem(),
    )?;
    fs_err::write(server_public_pem_path.as_path(), server_cert.public.pem())?;
    fs_err::write(
        server_private_pem_path.as_path(),
        server_cert.private.serialize_pem(),
    )?;
    fs_err::write(
        client_combined_pem_path.as_path(),
        // SSL_CLIENT_CERT expects a "combined" cert with the public and private key.
        format!(
            "{}\n{}",
            client_cert.public.pem(),
            client_cert.private.serialize_pem()
        ),
    )?;

    // ** Set SSL_CERT_FILE to non-existent location
    // ** Then verify our request fails to establish a connection

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, does_not_exist_cert_dir.as_os_str());
    }
    let (server_task, addr) = start_https_user_agent_server(&standalone_server_cert).await?;
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
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
    }

    // Validate the client error
    let Some(reqwest_middleware::Error::Middleware(middleware_error)) = res.err() else {
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

    // Validate the server error
    let server_res = server_task.await?;
    let expected_err = if let Err(anyhow_err) = server_res
        && let Some(io_err) = anyhow_err.downcast_ref::<std::io::Error>()
        && let Some(wrapped_err) = io_err.get_ref()
        && let Some(tls_err) = wrapped_err.downcast_ref::<rustls::Error>()
        && matches!(
            tls_err,
            rustls::Error::AlertReceived(AlertDescription::UnknownCA)
        ) {
        true
    } else {
        false
    };
    assert!(expected_err);

    // ** Set SSL_CERT_FILE to our public certificate
    // ** Then verify our request successfully establishes a connection

    unsafe {
        std::env::set_var(
            EnvVars::SSL_CERT_FILE,
            standalone_public_pem_path.as_os_str(),
        );
    }
    let (server_task, addr) = start_https_user_agent_server(&standalone_server_cert).await?;
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
    assert!(res.is_ok());
    let _ = server_task.await?; // wait for server shutdown
    unsafe {
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
    }

    // ** Set SSL_CERT_DIR to our cert dir as well as some other dir that does not exist
    // ** Then verify our request still successfully establishes a connection

    unsafe {
        std::env::set_var(
            EnvVars::SSL_CERT_DIR,
            std::env::join_paths(vec![
                cert_dir.path().as_os_str(),
                does_not_exist_cert_dir.as_os_str(),
            ])?,
        );
    }
    let (server_task, addr) = start_https_user_agent_server(&standalone_server_cert).await?;
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
    assert!(res.is_ok());
    let _ = server_task.await?; // wait for server shutdown
    unsafe {
        std::env::remove_var(EnvVars::SSL_CERT_DIR);
    }

    // ** Set SSL_CERT_DIR to only the dir that does not exist
    // ** Then verify our request fails to establish a connection

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_DIR, does_not_exist_cert_dir.as_os_str());
    }
    let (server_task, addr) = start_https_user_agent_server(&standalone_server_cert).await?;
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
        std::env::remove_var(EnvVars::SSL_CERT_DIR);
    }

    // Validate the client error
    let Some(reqwest_middleware::Error::Middleware(middleware_error)) = res.err() else {
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

    // Validate the server error
    let server_res = server_task.await?;
    let expected_err = if let Err(anyhow_err) = server_res
        && let Some(io_err) = anyhow_err.downcast_ref::<std::io::Error>()
        && let Some(wrapped_err) = io_err.get_ref()
        && let Some(tls_err) = wrapped_err.downcast_ref::<rustls::Error>()
        && matches!(
            tls_err,
            rustls::Error::AlertReceived(AlertDescription::UnknownCA)
        ) {
        true
    } else {
        false
    };
    assert!(expected_err);

    // *** mTLS Tests

    // ** Set SSL_CERT_FILE to our CA and SSL_CLIENT_CERT to our client cert
    // ** Then verify our request still successfully establishes a connection

    // We need to set SSL_CERT_FILE or SSL_CERT_DIR to our CA as we need to tell
    // our HTTP client that we trust certificates issued by our self-signed CA.
    // This inherently also tests that our server cert is also validated as part
    // of the certificate path validation algorithm.
    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, ca_public_pem_path.as_os_str());
        std::env::set_var(
            EnvVars::SSL_CLIENT_CERT,
            client_combined_pem_path.as_os_str(),
        );
    }
    let (server_task, addr) = start_https_mtls_user_agent_server(&ca_cert, &server_cert).await?;
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
    assert!(res.is_ok());
    let _ = server_task.await?; // wait for server shutdown
    unsafe {
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
        std::env::remove_var(EnvVars::SSL_CLIENT_CERT);
    }

    // ** Set SSL_CERT_FILE to our CA and unset SSL_CLIENT_CERT
    // ** Then verify our request fails to establish a connection

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, ca_public_pem_path.as_os_str());
    }
    let (server_task, addr) = start_https_mtls_user_agent_server(&ca_cert, &server_cert).await?;
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
        std::env::remove_var(EnvVars::SSL_CERT_FILE);
    }

    // Validate the client error
    let Some(reqwest_middleware::Error::Middleware(middleware_error)) = res.err() else {
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

    // Validate the server error
    let server_res = server_task.await?;
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

    // Fin.
    Ok(())
}
