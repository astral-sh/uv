//! An integration test for proxy support in `uv-client`.

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_client::BaseClientBuilder;

use super::http_util;

#[tokio::test]
async fn http_proxy() -> Result<()> {
    // Start a mock server to act as the target.
    let target_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&target_server)
        .await;

    // Start a mock server to act as the proxy.
    let proxy_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&proxy_server)
        .await;

    // Create a client with the proxy.
    let client = BaseClientBuilder::new(
        uv_client::Connectivity::Online,
        false,
        vec![],
        uv_preview::Preview::default(),
        std::time::Duration::from_secs(30),
        3,
    )
    .http_proxy(Some(proxy_server.uri()))
    .build();

    // Make a request to the target.
    let response = client
        .for_host(&target_server.uri().parse()?)
        .get(target_server.uri())
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Assert that the proxy was called.
    let received_requests = proxy_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), 1);

    Ok(())
}

#[tokio::test]
async fn https_proxy() -> Result<()> {
    // Generate a self-signed certificate for the target server.
    let server_cert = http_util::generate_self_signed_certs()?;

    // Start an HTTPS server to act as the target.
    let (target_server_handle, addr) =
        http_util::start_https_user_agent_server(&server_cert).await?;
    let target_uri = format!("https://{addr}");

    // Start a TCP listener to act as the proxy.
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;

    let proxy_handle = tokio::spawn(async move {
        let (mut stream, _) = proxy_listener.accept().await.unwrap();
        let mut buf = vec![0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("CONNECT"));
        // The mock proxy doesn't need to do anything else.
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
    });

    let trusted_host = addr.ip().to_string().parse()?;

    // Create a client with the proxy, and allow insecure connections to the target server.
    let client = BaseClientBuilder::new(
        uv_client::Connectivity::Online,
        false,
        vec![trusted_host], // Allow insecure for the target server
        uv_preview::Preview::default(),
        std::time::Duration::from_secs(30),
        3,
    )
    .https_proxy(Some(format!("http://{proxy_addr}")))
    .build();

    // Make a request to the target.
    let result = client
        .for_host(&target_uri.parse()?)
        .get(target_uri)
        .send()
        .await;

    // We expect the request to fail because our mock proxy doesn't actually tunnel.
    assert!(result.is_err());

    // Wait for the proxy to finish.
    proxy_handle.await?;

    // Shutdown the server
    target_server_handle.abort();

    Ok(())
}

#[tokio::test]
async fn no_proxy() -> Result<()> {
    // Start a mock server to act as the target.
    let target_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&target_server)
        .await;

    // Start a mock server to act as the proxy.
    let proxy_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&proxy_server)
        .await;

    // The host of the target server should be excluded from proxying.
    let target_host = target_server.address().ip().to_string();

    // Create a client with the proxy.
    let client = BaseClientBuilder::new(
        uv_client::Connectivity::Online,
        false,
        vec![],
        uv_preview::Preview::default(),
        std::time::Duration::from_secs(30),
        3,
    )
    .http_proxy(Some(proxy_server.uri()))
    .no_proxy(Some(vec![target_host]))
    .build();

    // Make a request to the target.
    let response = client
        .for_host(&target_server.uri().parse()?)
        .get(target_server.uri())
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Assert that the proxy was NOT called.
    let received_requests = proxy_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), 0);

    Ok(())
}
