//! An integration test for proxy support in `uv-client`.

use std::str::FromStr;

use anyhow::Result;
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::ProxyUrl;
use uv_distribution_types::IndexUrl;
use uv_errors::{ErrorOptions, Hinted, write_error_chain_with_options};
use uv_redacted::SafeUrl;

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
    let client = BaseClientBuilder::default()
        .http_proxy(Some(proxy_server.uri().parse::<ProxyUrl>()?))
        .build()?;

    // Make a request to the target.
    let target_url = SafeUrl::parse_relaxed(&target_server.uri())?;
    let response = client.for_host(&target_url).get(target_url).send().await?;

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
    let client = BaseClientBuilder::default()
        .http_proxy(Some(proxy_server.uri().parse::<ProxyUrl>()?))
        .no_proxy(Some(vec![target_host]))
        .build()?;

    // Make a request to the target.
    let target_url = SafeUrl::parse_relaxed(&target_server.uri())?;
    let response = client.for_host(&target_url).get(target_url).send().await?;

    assert_eq!(response.status(), 200);

    // Assert that the proxy was NOT called.
    let received_requests = proxy_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), 0);

    Ok(())
}

#[tokio::test]
async fn proxy_unreachable_error_reported_clearly() -> Result<()> {
    let cache = Cache::temp()?.init().await?;
    let proxy_url = "http://127.0.0.1:1".parse::<ProxyUrl>()?;
    let base = BaseClientBuilder::default()
        .retries(0)
        .no_retry_delay(true)
        .https_proxy(Some(proxy_url));
    let client = RegistryClientBuilder::new(base, cache).build()?;

    let index = IndexUrl::from_str("https://pypi.org/simple")?;
    let error = client
        .fetch_simple_index(&index)
        .await
        .expect_err("connecting through unreachable proxy should fail");

    let mut rendered = String::new();
    write_error_chain_with_options(
        &error,
        &error.hints(),
        ErrorOptions::default().with_stream(&mut rendered),
    )?;

    assert!(
        rendered.contains("Failed to connect to proxy `http://127.0.0.1:1/`"),
        "expected proxy error in {rendered:?}"
    );
    assert!(
        rendered.contains(
            "Check that your proxy settings (e.g., `HTTP_PROXY`, `HTTPS_PROXY`) are correct"
        ),
        "expected proxy hint in {rendered:?}"
    );
    assert!(
        !rendered.contains("Could not connect, are you offline?"),
        "unexpected offline hint in {rendered:?}"
    );

    Ok(())
}

#[tokio::test]
async fn proxy_credentials_redacted_in_error() -> Result<()> {
    let cache = Cache::temp()?.init().await?;
    let proxy_url = "http://user:secretpassword@127.0.0.1:1".parse::<ProxyUrl>()?;
    let base = BaseClientBuilder::default()
        .retries(0)
        .no_retry_delay(true)
        .https_proxy(Some(proxy_url));
    let client = RegistryClientBuilder::new(base, cache).build()?;

    let index = IndexUrl::from_str("https://pypi.org/simple")?;
    let error = client
        .fetch_simple_index(&index)
        .await
        .expect_err("connecting through unreachable proxy should fail");

    let mut rendered = String::new();
    write_error_chain_with_options(
        &error,
        &error.hints(),
        ErrorOptions::default().with_stream(&mut rendered),
    )?;

    assert!(
        rendered.contains("Failed to connect to proxy `http://user:****@127.0.0.1:1/`"),
        "expected redacted proxy in {rendered:?}"
    );
    assert!(
        !rendered.contains("secretpassword"),
        "credentials leaked in error: {rendered:?}"
    );

    Ok(())
}

#[test]
fn proxy_for_url_selection() -> Result<()> {
    let http_proxy = "http://http-proxy:8080".parse::<ProxyUrl>()?;
    let https_proxy = "http://https-proxy:8443".parse::<ProxyUrl>()?;
    let client = BaseClientBuilder::default()
        .http_proxy(Some(http_proxy))
        .https_proxy(Some(https_proxy))
        .no_proxy(Some(vec![
            "localhost".to_string(),
            ".example.com".to_string(),
        ]))
        .build()?;

    let pypi = SafeUrl::parse("https://pypi.org/simple")?;
    assert_eq!(
        client.proxy_for_url(&pypi).map(|u| u.to_string()),
        Some("http://https-proxy:8443/".to_string())
    );

    let http_site = SafeUrl::parse("http://insecure.org/pkg")?;
    assert_eq!(
        client.proxy_for_url(&http_site).map(|u| u.to_string()),
        Some("http://http-proxy:8080/".to_string())
    );

    let no_proxy_host = SafeUrl::parse("https://localhost:8000/pkg")?;
    assert_eq!(client.proxy_for_url(&no_proxy_host), None);

    let no_proxy_domain = SafeUrl::parse("https://sub.example.com/pkg")?;
    assert_eq!(client.proxy_for_url(&no_proxy_domain), None);

    Ok(())
}
