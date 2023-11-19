use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;

use async_http_range_reader::{AsyncHttpRangeReader, AsyncHttpRangeReaderError};
use async_zip::tokio::read::seek::ZipFileReader;
use futures::TryStreamExt;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use reqwest::{Client, ClientBuilder, Response, StatusCode};
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use tempfile::tempfile;
use tokio::io::BufWriter;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, trace};
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{BuiltDist, Metadata};
use install_wheel_rs::find_dist_info;
use puffin_cache::WheelMetadataCache;
use puffin_normalize::PackageName;
use pypi_types::{File, IndexUrl, Metadata21, SimpleJson};

use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::{CachedClient, CachedClientError, Error};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder {
    index: IndexUrl,
    extra_index: Vec<IndexUrl>,
    no_index: bool,
    proxy: Url,
    retries: u32,
    cache: PathBuf,
}

impl RegistryClientBuilder {
    pub fn new(cache: impl Into<PathBuf>) -> Self {
        Self {
            index: IndexUrl::Pypi,
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
        self.index = IndexUrl::from(index);
        self
    }

    #[must_use]
    pub fn extra_index(mut self, extra_index: Vec<Url>) -> Self {
        self.extra_index = extra_index.into_iter().map(IndexUrl::from).collect();
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

        let uncached_client = reqwest_middleware::ClientBuilder::new(client_raw.clone())
            .with(retry_strategy)
            .build();

        let cached_client = CachedClient::new(uncached_client.clone());
        RegistryClient {
            index: self.index,
            extra_index: self.extra_index,
            no_index: self.no_index,
            client: client_builder.build(),
            client_raw: client_raw.clone(),
            uncached_client,
            cache: self.cache,
            cached_client,
        }
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
// TODO(konstin): Clean up the clients once we moved everything to common caching
#[derive(Debug, Clone)]
pub struct RegistryClient {
    pub(crate) index: IndexUrl,
    pub(crate) extra_index: Vec<IndexUrl>,
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
    pub fn cached_client(&self) -> &CachedClient {
        &self.cached_client
    }

    /// Fetch a package from the `PyPI` simple API.
    pub async fn simple(&self, package_name: PackageName) -> Result<(IndexUrl, SimpleJson), Error> {
        if self.no_index {
            return Err(Error::NoIndex(package_name.as_ref().to_string()));
        }

        for index in std::iter::once(&self.index).chain(self.extra_index.iter()) {
            // Format the URL for PyPI.
            let mut url: Url = index.clone().into();
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
                    let data = serde_json::from_str(&text)
                        .map_err(move |e| Error::from_json_err(e, url))?;
                    return Ok((index.clone(), data));
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

    /// Fetch the metadata for a remote wheel file.
    ///
    /// For a remote wheel, we try the following ways to fetch the metadata:  
    /// 1. From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
    /// 2. From a remote wheel by partial zip reading
    /// 3. From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
    pub async fn wheel_metadata(&self, built_dist: &BuiltDist) -> Result<Metadata21, Error> {
        let metadata = match &built_dist {
            BuiltDist::Registry(wheel) => {
                self.wheel_metadata_registry(wheel.index.clone(), wheel.file.clone())
                    .await?
            }
            BuiltDist::DirectUrl(wheel) => {
                self.wheel_metadata_no_pep658(&wheel.filename, &wheel.url, WheelMetadataCache::Url)
                    .await?
            }
            BuiltDist::Path(wheel) => {
                let reader = fs_err::tokio::File::open(&wheel.path).await?;
                Self::metadata_from_async_read(&wheel.filename, built_dist.to_string(), reader)
                    .await?
            }
        };

        if metadata.name != *built_dist.name() {
            return Err(Error::NameMismatch {
                metadata: metadata.name,
                given: built_dist.name().clone(),
            });
        }

        Ok(metadata)
    }

    /// Fetch the metadata from a wheel file.
    async fn wheel_metadata_registry(
        &self,
        index: IndexUrl,
        file: File,
    ) -> Result<Metadata21, Error> {
        if self.no_index {
            return Err(Error::NoIndex(file.filename));
        }

        // If the metadata file is available at its own url (PEP 658), download it from there
        let url = Url::parse(&file.url)?;
        let filename = WheelFilename::from_str(&file.filename)?;
        if file
            .dist_info_metadata
            .as_ref()
            .is_some_and(pypi_types::Metadata::is_available)
        {
            let url = Url::parse(&format!("{}.metadata", file.url))?;

            let cache_dir = WheelMetadataCache::Index(index).wheel_cache_dir(&self.cache, &url);
            let cache_file = format!("{}.json", filename.stem());

            let response_callback = |response: Response| async {
                Metadata21::parse(response.bytes().await?.as_ref())
                    .map_err(|err| Error::MetadataParseError(filename, url.to_string(), err))
            };
            let req = self.client_raw.get(url.clone()).build()?;
            Ok(self
                .cached_client
                .get_cached_with_callback(req, &cache_dir, &cache_file, response_callback)
                .await?)
        } else {
            // If we lack PEP 658 support, try using HTTP range requests to read only the
            // `.dist-info/METADATA` file from the zip, and if that also fails, download the whole wheel
            // into the cache and read from there
            self.wheel_metadata_no_pep658(&filename, &url, WheelMetadataCache::Index(index))
                .await
        }
    }

    /// Get the wheel metadata if it isn't available in an index through PEP 658
    async fn wheel_metadata_no_pep658(
        &self,
        filename: &WheelFilename,
        url: &Url,
        cache_shard: WheelMetadataCache,
    ) -> Result<Metadata21, Error> {
        if self.no_index {
            return Err(Error::NoIndex(url.to_string()));
        }

        let cache_dir = cache_shard.wheel_cache_dir(&self.cache, url);
        let cache_file = format!("{}.json", filename.stem());

        // This response callback is special, we actually make a number of subsequent requests to
        // fetch the file from the remote zip.
        let client = self.client_raw.clone();
        let read_metadata_from_initial_response = |response: Response| async {
            let mut reader = AsyncHttpRangeReader::from_head_response(client, response).await?;
            trace!("Getting metadata for {filename} by range request");
            let text = wheel_metadata_from_remote_zip(filename, &mut reader).await?;
            let metadata = Metadata21::parse(text.as_bytes())
                .map_err(|err| Error::MetadataParseError(filename.clone(), url.to_string(), err))?;
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
            Err(CachedClientError::Client(Error::AsyncHttpRangeReader(
                AsyncHttpRangeReaderError::HttpRangeRequestUnsupported,
            ))) => {}
            Err(err) => return Err(err.into()),
        }

        // The range request version failed (this is bad, the webserver should support this), fall
        // back to downloading the entire file and the reading the file from the zip the regular way

        debug!("Range requests not supported for {filename}, downloading whole wheel");
        // TODO(konstin): Download the wheel into a cache shared with the installer instead
        // Note that this branch is only hit when you're not using and the server where
        // you host your wheels for some reasons doesn't support range requests
        // (tbh we should probably warn here and tell users to get a better registry because
        // their current one makes resolution unnecessary slow)
        let temp_download = tempfile()?;
        let mut writer = BufWriter::new(tokio::fs::File::from_std(temp_download));
        let mut reader = self.stream_external(url).await?.compat();
        tokio::io::copy(&mut reader, &mut writer).await?;
        let reader = writer.into_inner();

        Self::metadata_from_async_read(filename, url.to_string(), reader).await
    }

    async fn metadata_from_async_read(
        filename: &WheelFilename,
        debug_source: String,
        reader: impl tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    ) -> Result<Metadata21, Error> {
        let mut zip_reader = ZipFileReader::with_tokio(reader)
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?;

        let (metadata_idx, _dist_info_dir) = find_dist_info(
            filename,
            zip_reader
                .file()
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(idx, e)| Some((idx, e.entry().filename().as_str().ok()?))),
        )?;

        // Read the contents of the METADATA file
        let mut contents = Vec::new();
        zip_reader
            .reader_with_entry(metadata_idx)
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?
            .read_to_end_checked(&mut contents)
            .await
            .map_err(|err| Error::Zip(filename.clone(), err))?;

        let metadata = Metadata21::parse(&contents)
            .map_err(|err| Error::MetadataParseError(filename.clone(), debug_source, err))?;
        Ok(metadata)
    }

    /// Stream a file from an external URL.
    pub async fn stream_external(
        &self,
        url: &Url,
    ) -> Result<Box<dyn futures::AsyncRead + Unpin + Send + Sync>, Error> {
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
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                .into_async_read(),
        ))
    }
}
