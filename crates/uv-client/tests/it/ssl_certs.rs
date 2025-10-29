use std::str::FromStr;

use anyhow::Result;
use rustls::AlertDescription;
use url::Url;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_client::RegistryClientBuilder;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::http_util::{generate_self_signed_certs, start_https_user_agent_server, test_cert_dir};

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

    // Persist the certs in PKCS8 format as the env vars expect a path on disk
    fs_err::write(
        standalone_public_pem_path.as_path(),
        standalone_server_cert.public.pem(),
    )?;
    fs_err::write(
        standalone_private_pem_path.as_path(),
        standalone_server_cert.private.serialize_pem(),
    )?;

    // ** Set SSL_CERT_FILE to non-existent location
    // ** Then verify our request fails to establish a connection

    unsafe {
        std::env::set_var(EnvVars::SSL_CERT_FILE, does_not_exist_cert_dir.as_os_str());
    }
    let (server_task, addr) = start_https_user_agent_server(&standalone_server_cert).await?;
    let url = DisplaySafeUrl::from_str(&format!("https://{addr}"))?;
    let cache = Cache::temp()?.init()?;
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
    let cache = Cache::temp()?.init()?;
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
    let cache = Cache::temp()?.init()?;
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
    let cache = Cache::temp()?.init()?;
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

    Ok(())
}
