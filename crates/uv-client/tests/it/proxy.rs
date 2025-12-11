//! An integration test for proxy support in `uv-client`.

use anyhow::Result;
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_client::BaseClientBuilder;
use uv_configuration::ProxyUrl;

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
    .http_proxy(Some(proxy_server.uri().parse::<ProxyUrl>()?))
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
    .http_proxy(Some(proxy_server.uri().parse::<ProxyUrl>()?))
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
