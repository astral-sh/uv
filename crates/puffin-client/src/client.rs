use std::path::PathBuf;
use std::sync::Arc;

use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::ClientBuilder;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use url::Url;

#[derive(Debug, Clone)]
pub struct PypiClientBuilder {
    registry: Url,
    proxy: Url,
    retries: u32,
    cache: Option<PathBuf>,
}

impl Default for PypiClientBuilder {
    fn default() -> Self {
        Self {
            registry: Url::parse("https://pypi.org").unwrap(),
            proxy: Url::parse("https://pypi-metadata.ruff.rs").unwrap(),
            cache: None,
            retries: 0,
        }
    }
}

impl PypiClientBuilder {
    #[must_use]
    pub fn registry(mut self, registry: Url) -> Self {
        self.registry = registry;
        self
    }

    #[must_use]
    pub fn proxy(mut self, proxy: Url) -> Self {
        self.proxy = proxy;
        self
    }

    #[must_use]
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    #[must_use]
    pub fn cache<T>(mut self, cache: Option<T>) -> Self
    where
        T: Into<PathBuf>,
    {
        self.cache = cache.map(Into::into);
        self
    }

    pub fn build(self) -> PypiClient {
        let client_raw = {
            let client_core = ClientBuilder::new()
                .user_agent("puffin")
                .pool_max_idle_per_host(20)
                .timeout(std::time::Duration::from_secs(60 * 5));

            client_core.build().expect("Fail to build HTTP client.")
        };

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(self.retries);
        let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);

        let mut client_builder =
            reqwest_middleware::ClientBuilder::new(client_raw.clone()).with(retry_strategy);

        if let Some(path) = self.cache {
            client_builder = client_builder.with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: CACacheManager { path },
                options: HttpCacheOptions::default(),
            }));
        }

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(self.retries);
        let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);

        let uncached_client_builder =
            reqwest_middleware::ClientBuilder::new(client_raw).with(retry_strategy);

        PypiClient {
            registry: Arc::new(self.registry),
            proxy: Arc::new(self.proxy),
            client: client_builder.build(),
            uncached_client: uncached_client_builder.build(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PypiClient {
    pub(crate) registry: Arc<Url>,
    pub(crate) proxy: Arc<Url>,
    pub(crate) client: ClientWithMiddleware,
    pub(crate) uncached_client: ClientWithMiddleware,
}
