use std::str::FromStr;

use anyhow::Result;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_distribution_types::IndexUrl;

#[tokio::test]
async fn registry_errors_include_retry_context() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .mount(&server)
        .await;

    let cache = Cache::temp()?.init().await?;
    let base = BaseClientBuilder::default().retries(2).no_retry_delay(true);
    let client = RegistryClientBuilder::new(base, cache).build()?;
    let index = IndexUrl::from_str(&format!("{}/simple", server.uri()))?;
    let error = client.fetch_simple_index(&index).await.unwrap_err();
    assert_eq!(error.retries(), 2);
    server.verify().await;

    let chain = format!("{:#}", anyhow::Error::new(error)).replace(&server.uri(), "[HOST]");
    insta::with_settings!({filters => [(r"in \d+\.\ds", "in [TIME]s")]}, {
        insta::assert_snapshot!(chain, @"Request failed after 2 retries in [TIME]s: Failed to fetch: `[HOST]/simple/`: HTTP status server error (503 Service Unavailable) for url ([HOST]/simple/)");
    });
    Ok(())
}
