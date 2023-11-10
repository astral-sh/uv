use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;

use async_http_range_reader::{AsyncHttpRangeReader, AsyncHttpRangeReaderError};
use async_zip::tokio::read::seek::ZipFileReader;
use futures::{AsyncRead, StreamExt, TryStreamExt};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, ClientBuilder, Response, StatusCode};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use tempfile::tempfile;
use tokio::io::BufWriter;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, trace};
use url::Url;

use distribution_filename::WheelFilename;
use install_wheel_rs::find_dist_info;
use puffin_normalize::PackageName;
use pypi_types::{File, Metadata21, SimpleJson};

use crate::cached_client::CachedClient;
use crate::error::Error;
use crate::remote_metadata::{
    wheel_metadata_from_remote_zip, WHEEL_METADATA_FROM_INDEX, WHEEL_METADATA_FROM_ZIP_CACHE,
};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder {
    index: Url,
    extra_index: Vec<Url>,
    no_index: bool,
    proxy: Url,
    retries: u32,
    cache: PathBuf,
}

impl RegistryClientBuilder {
    pub fn new(cache: impl Into<PathBuf>) -> Self {
        Self {
            index: Url::parse("https://pypi.org/simple").unwrap(),
            extra_index: vec![],
            no_index: false,
            proxy: Url::parse("https://pypi-metadata.ruff.rs").unwrap(),
            cache: cache.into(),
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
    pub fn cache<T>(mut self, cache: impl Into<PathBuf>) -> Self {
        self.cache = cache.into();
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

        client_builder = client_builder.with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager {
                path: self.cache.clone(),
            },
            options: HttpCacheOptions::default(),
        }));

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(self.retries);
        let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);

        let uncached_client_builder =
            reqwest_middleware::ClientBuilder::new(client_raw.clone()).with(retry_strategy);

        RegistryClient {
            index: self.index,
            extra_index: self.extra_index,
            no_index: self.no_index,
            client: client_builder.build(),
            client_raw: client_raw.clone(),
            uncached_client: uncached_client_builder.build(),
            cache: self.cache,
            cached_client: CachedClient::new(client_raw),
        }
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
// TODO(konstin): Clean up the clients once we moved everything to common caching
#[derive(Debug, Clone)]
pub struct RegistryClient {
    pub(crate) index: Url,
    pub(crate) extra_index: Vec<Url>,
    /// Ignore the package index, instead relying on local archives and caches.
    pub(crate) no_index: bool,
    pub(crate) client: ClientWithMiddleware,
    pub(crate) uncached_client: ClientWithMiddleware,
    pub(crate) client_raw: Client,
    pub(crate) cached_client: CachedClient,
    /// Used for the remote wheel METADATA cache
    pub(crate) cache: PathBuf,
}

impl RegistryClient {
    /// Fetch a package from the `PyPI` simple API.
    pub async fn simple(&self, package_name: PackageName) -> Result<SimpleJson, Error> {
        if self.no_index {
            return Err(Error::NoIndex(package_name.as_ref().to_string()));
        }

        for index in std::iter::once(&self.index).chain(self.extra_index.iter()) {
            // Format the URL for PyPI.
            let mut url = index.clone();
            url.path_segments_mut().unwrap().push(package_name.as_ref());
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
    pub async fn wheel_metadata(&self, file: File) -> Result<Metadata21, Error> {
        if self.no_index {
            return Err(Error::NoIndex(file.filename));
        }

        // If the metadata file is available at its own url (PEP 658), download it from there
        let url = Url::parse(&file.url)?;
        let filename = WheelFilename::from_str(&file.filename)?;
        if file.data_dist_info_metadata.is_available() {
            let url = Url::parse(&format!("{}.metadata", file.url))?;

            let cache_dir = self.cache.join(WHEEL_METADATA_FROM_INDEX).join("pypi");
            let cache_file = format!("{}.json", filename.stem());

            let response_callback = |response: Response| async {
                Metadata21::parse(response.bytes().await?.as_ref())
                    .map_err(|err| Error::MetadataParseError(filename, url.clone(), err))
            };
            let req = self.client_raw.get(url.clone()).build()?;
            self.cached_client
                .get_cached_with_callback(req, &cache_dir, &cache_file, response_callback)
                .await
        // If we lack PEP 658 support, try using HTTP range requests to read only the
        // `.dist-info/METADATA` file from the zip, and if that also fails, download the whole wheel
        // into the cache and read from there
        } else {
            self.wheel_metadata_no_index(&filename, &url).await
        }
    }

    /// Get the wheel metadata if it isn't available in an index through PEP 658
    pub async fn wheel_metadata_no_index(
        &self,
        filename: &WheelFilename,
        url: &Url,
    ) -> Result<Metadata21, Error> {
        let cache_dir = self.cache.join(WHEEL_METADATA_FROM_ZIP_CACHE).join("pypi");
        let cache_file = format!("{}.json", filename.stem());

        // This response callback is special, we actually make a number of subsequent requests to
        // fetch the file from the remote zip.
        let client = self.client_raw.clone();
        let read_metadata_from_initial_response = |response: Response| async {
            let mut reader = AsyncHttpRangeReader::from_head_response(client, response).await?;
            trace!("Getting metadata for {filename} by range request");
            let text = wheel_metadata_from_remote_zip(filename, &mut reader).await?;
            let metadata = Metadata21::parse(text.as_bytes())
                .map_err(|err| Error::MetadataParseError(filename.clone(), url.clone(), err))?;
            Ok(metadata)
        };

        let req = self.client_raw.head(url.clone()).build()?;
        let result = self
            .cached_client
            .get_cached_with_callback(
                req,
                &cache_dir,
                &cache_file,
                read_metadata_from_initial_response,
            )
            .await;

        match result {
            Ok(metadata) => {
                return Ok(metadata);
            }
            Err(Error::AsyncHttpRangeReader(
                AsyncHttpRangeReaderError::HttpRangeRequestUnsupported,
            )) => {}
            Err(err) => return Err(err),
        }

        // The range request version failed (this is bad, the webserver should support this), fall
        // back to downloading the entire file and the reading the file from the zip the regular way

        debug!("Range requests not supported for {filename}, downloading whole wheel");
        // TODO(konstin): Download the wheel into a cache shared with the installer instead
        // Note that this branch is only hit when you're not using and the server where
        // you host your wheels for some reasons doesn't support range requests
        // (tbh we should probably warn here and tekk users to get a better registry because
        // their current one makes resolution unnecessary slow)
        let temp_download = tempfile()?;
        let mut writer = BufWriter::new(tokio::fs::File::from_std(temp_download));
        let mut reader = self.stream_external(url).await?.compat();
        tokio::io::copy(&mut reader, &mut writer).await?;
        let temp_download = writer.into_inner();

        let mut reader = ZipFileReader::new(temp_download.compat())
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?;

        let (metadata_idx, _dist_info_dir) = find_dist_info(
            filename,
            reader
                .file()
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(idx, e)| Some((idx, e.entry().filename().as_str().ok()?))),
        )
        .map_err(|err| Error::InvalidDistInfo(filename.clone(), err))?;

        // Read the contents of the METADATA file
        let mut contents = Vec::new();
        reader
            .reader_with_entry(metadata_idx)
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?
            .read_to_end_checked(&mut contents)
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?;

        let metadata = Metadata21::parse(&contents)
            .map_err(|err| Error::MetadataParseError(filename.clone(), url.clone(), err))?;
        Ok(metadata)
    }

    /// Stream a file from an external URL.
    pub async fn stream_external(
        &self,
        url: &Url,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send + Sync>, Error> {
        if self.no_index {
            return Err(Error::NoIndex(url.to_string()));
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
