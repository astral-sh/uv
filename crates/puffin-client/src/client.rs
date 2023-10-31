use std::fmt::Debug;
use std::path::PathBuf;

use futures::{AsyncRead, StreamExt, TryStreamExt};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::ClientBuilder;
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use tracing::trace;
use url::Url;

use puffin_package::package_name::PackageName;
use puffin_package::pypi_types::{File, Metadata21, SimpleJson};

use crate::error::Error;

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder {
    index: Url,
    extra_index: Vec<Url>,
    no_index: bool,
    proxy: Url,
    retries: u32,
    cache: Option<PathBuf>,
}

impl Default for RegistryClientBuilder {
    fn default() -> Self {
        Self {
            index: Url::parse("https://pypi.org/simple").unwrap(),
            extra_index: vec![],
            no_index: false,
            proxy: Url::parse("https://pypi-metadata.ruff.rs").unwrap(),
            cache: None,
            retries: 0,
        }
    }
}

impl RegistryClientBuilder {
    #[must_use]
    pub fn index(mut self, index: Url) -> Self {
        self.index = index;
        self
    }

    #[must_use]
    pub fn extra_index(mut self, extra_index: Vec<Url>) -> Self {
        self.extra_index = extra_index;
        self
    }

    #[must_use]
    pub fn no_index(mut self) -> Self {
        self.no_index = true;
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

    pub fn build(self) -> RegistryClient {
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

        RegistryClient {
            index: self.index,
            extra_index: self.extra_index,
            no_index: self.no_index,
            proxy: self.proxy,
            client: client_builder.build(),
            uncached_client: uncached_client_builder.build(),
        }
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    pub(crate) index: Url,
    pub(crate) extra_index: Vec<Url>,
    pub(crate) no_index: bool,
    pub(crate) proxy: Url,
    pub(crate) client: ClientWithMiddleware,
    pub(crate) uncached_client: ClientWithMiddleware,
}

impl RegistryClient {
    /// Fetch a package from the `PyPI` simple API.
    pub async fn simple(&self, package_name: impl AsRef<str>) -> Result<SimpleJson, Error> {
        if self.no_index {
            return Err(Error::PackageNotFound(package_name.as_ref().to_string()));
        }

        for index in std::iter::once(&self.index).chain(self.extra_index.iter()) {
            // Format the URL for PyPI.
            let mut url = index.clone();
            url.path_segments_mut()
                .unwrap()
                .push(PackageName::normalize(&package_name).as_ref());
            url.path_segments_mut().unwrap().push("");
            url.set_query(Some("format=application/vnd.pypi.simple.v1+json"));

            trace!(
                "Fetching metadata for {} from {}",
                package_name.as_ref(),
                url
            );

            // Fetch from the index.
            match self.simple_impl(&url).await {
                Ok(text) => {
                    return serde_json::from_str(&text)
                        .map_err(move |e| Error::from_json_err(e, String::new()));
                }
                Err(err) => {
                    if err.status() == Some(StatusCode::NOT_FOUND) {
                        continue;
                    }
                    return Err(err.into());
                }
            }
        }

        Err(Error::PackageNotFound(package_name.as_ref().to_string()))
    }

    async fn simple_impl(&self, url: &Url) -> Result<String, reqwest_middleware::Error> {
        Ok(self
            .client
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    /// Fetch the metadata from a wheel file.
    pub async fn file(&self, file: File) -> Result<Metadata21, Error> {
        if self.no_index {
            return Err(Error::FileNotFound(file.filename));
        }

        // Per PEP 658, if `data-dist-info-metadata` is available, we can request it directly;
        // otherwise, send to our dedicated caching proxy.
        let url = if file.data_dist_info_metadata.is_available() {
            Url::parse(&format!("{}.metadata", file.url))?
        } else {
            self.proxy.join(file.url.parse::<Url>()?.path())?
        };

        trace!("Fetching file {} from {}", file.filename, url);

        // Fetch from the index.
        let text = self.file_impl(&url).await.map_err(|err| {
            if err.status() == Some(StatusCode::NOT_FOUND) {
                Error::FileNotFound(file.filename.to_string())
            } else {
                err.into()
            }
        })?;
        Metadata21::parse(text.as_bytes()).map_err(std::convert::Into::into)
    }

    async fn file_impl(&self, url: &Url) -> Result<String, reqwest_middleware::Error> {
        Ok(self
            .client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    /// Stream a file from an external URL.
    pub async fn stream_external(
        &self,
        url: &Url,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send + Sync>, Error> {
        if self.no_index {
            return Err(Error::ResourceNotFound(url.clone()));
        }

        Ok(Box::new(
            self.uncached_client
                .get(url.to_string())
                .send()
                .await?
                .error_for_status()?
                .bytes_stream()
                .map(|r| match r {
                    Ok(bytes) => Ok(bytes),
                    Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
                })
                .into_async_read(),
        ))
    }
}
