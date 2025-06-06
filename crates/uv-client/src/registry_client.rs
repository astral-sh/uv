use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_http_range_reader::AsyncHttpRangeReader;
use futures::{FutureExt, StreamExt, TryStreamExt};
use http::{HeaderMap, StatusCode};
use itertools::Either;
use reqwest::{Proxy, Response};
use reqwest_middleware::ClientWithMiddleware;
use rustc_hash::FxHashMap;
use tokio::sync::{Mutex, Semaphore};
use tracing::{Instrument, debug, info_span, instrument, trace, warn};
use url::Url;

use uv_auth::Indexes;
use uv_cache::{Cache, CacheBucket, CacheEntry, WheelCache};
use uv_configuration::KeyringProviderType;
use uv_configuration::{IndexStrategy, TrustedHost};
use uv_distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use uv_distribution_types::{
    BuiltDist, File, FileLocation, IndexCapabilities, IndexFormat, IndexLocations,
    IndexMetadataRef, IndexStatusCodeDecision, IndexStatusCodeStrategy, IndexUrl, IndexUrls, Name,
};
use uv_metadata::{read_metadata_async_seek, read_metadata_async_stream};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_pypi_types::{ResolutionMetadata, SimpleJson};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;
use uv_torch::TorchStrategy;

use crate::base_client::{BaseClientBuilder, ExtraMiddleware};
use crate::cached_client::CacheControl;
use crate::flat_index::FlatIndexEntry;
use crate::html::SimpleHtml;
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::rkyvutil::OwnedArchive;
use crate::{
    BaseClient, CachedClient, CachedClientError, Error, ErrorKind, FlatIndexClient,
    FlatIndexEntries,
};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder<'a> {
    index_urls: IndexUrls,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchStrategy>,
    cache: Cache,
    base_client_builder: BaseClientBuilder<'a>,
}

impl RegistryClientBuilder<'_> {
    pub fn new(cache: Cache) -> Self {
        Self {
            index_urls: IndexUrls::default(),
            index_strategy: IndexStrategy::default(),
            torch_backend: None,
            cache,
            base_client_builder: BaseClientBuilder::new(),
        }
    }
}

impl<'a> RegistryClientBuilder<'a> {
    #[must_use]
    pub fn index_locations(mut self, index_locations: &IndexLocations) -> Self {
        self.index_urls = index_locations.index_urls();
        self.base_client_builder = self
            .base_client_builder
            .indexes(Indexes::from(index_locations));
        self
    }

    #[must_use]
    pub fn index_strategy(mut self, index_strategy: IndexStrategy) -> Self {
        self.index_strategy = index_strategy;
        self
    }

    #[must_use]
    pub fn torch_backend(mut self, torch_backend: Option<TorchStrategy>) -> Self {
        self.torch_backend = torch_backend;
        self
    }

    #[must_use]
    pub fn keyring(mut self, keyring_type: KeyringProviderType) -> Self {
        self.base_client_builder = self.base_client_builder.keyring(keyring_type);
        self
    }

    #[must_use]
    pub fn allow_insecure_host(mut self, allow_insecure_host: Vec<TrustedHost>) -> Self {
        self.base_client_builder = self
            .base_client_builder
            .allow_insecure_host(allow_insecure_host);
        self
    }

    #[must_use]
    pub fn connectivity(mut self, connectivity: Connectivity) -> Self {
        self.base_client_builder = self.base_client_builder.connectivity(connectivity);
        self
    }

    #[must_use]
    pub fn retries(mut self, retries: u32) -> Self {
        self.base_client_builder = self.base_client_builder.retries(retries);
        self
    }

    #[must_use]
    pub fn native_tls(mut self, native_tls: bool) -> Self {
        self.base_client_builder = self.base_client_builder.native_tls(native_tls);
        self
    }

    #[must_use]
    pub fn cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }

    #[must_use]
    pub fn extra_middleware(mut self, middleware: ExtraMiddleware) -> Self {
        self.base_client_builder = self.base_client_builder.extra_middleware(middleware);
        self
    }

    #[must_use]
    pub fn markers(mut self, markers: &'a MarkerEnvironment) -> Self {
        self.base_client_builder = self.base_client_builder.markers(markers);
        self
    }

    #[must_use]
    pub fn platform(mut self, platform: &'a Platform) -> Self {
        self.base_client_builder = self.base_client_builder.platform(platform);
        self
    }

    #[must_use]
    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.base_client_builder = self.base_client_builder.proxy(proxy);
        self
    }

    pub fn build(self) -> RegistryClient {
        // Build a base client
        let builder = self.base_client_builder;

        let client = builder.build();

        let timeout = client.timeout();
        let connectivity = client.connectivity();

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        RegistryClient {
            index_urls: self.index_urls,
            index_strategy: self.index_strategy,
            torch_backend: self.torch_backend,
            cache: self.cache,
            connectivity,
            client,
            timeout,
            flat_indexes: Arc::default(),
        }
    }

    /// Share the underlying client between two different middleware configurations.
    pub fn wrap_existing(self, existing: &BaseClient) -> RegistryClient {
        // Wrap in any relevant middleware and handle connectivity.
        let client = self.base_client_builder.wrap_existing(existing);

        let timeout = client.timeout();
        let connectivity = client.connectivity();

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        RegistryClient {
            index_urls: self.index_urls,
            index_strategy: self.index_strategy,
            torch_backend: self.torch_backend,
            cache: self.cache,
            connectivity,
            client,
            timeout,
            flat_indexes: Arc::default(),
        }
    }
}

impl<'a> TryFrom<BaseClientBuilder<'a>> for RegistryClientBuilder<'a> {
    type Error = std::io::Error;

    fn try_from(value: BaseClientBuilder<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            index_urls: IndexUrls::default(),
            index_strategy: IndexStrategy::default(),
            torch_backend: None,
            cache: Cache::temp()?,
            base_client_builder: value,
        })
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// The index URLs to use for fetching packages.
    index_urls: IndexUrls,
    /// The strategy to use when fetching across multiple indexes.
    index_strategy: IndexStrategy,
    /// The strategy to use when selecting a PyTorch backend, if any.
    torch_backend: Option<TorchStrategy>,
    /// The underlying HTTP client.
    client: CachedClient,
    /// Used for the remote wheel METADATA cache.
    cache: Cache,
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: Duration,
    /// The flat index entries for each `--find-links`-style index URL.
    flat_indexes: Arc<Mutex<FlatIndexCache>>,
}

/// The format of the package metadata returned by querying an index.
#[derive(Debug)]
pub enum MetadataFormat {
    /// The metadata adheres to the Simple Repository API format.
    Simple(OwnedArchive<SimpleMetadata>),
    /// The metadata consists of a list of distributions from a "flat" index.
    Flat(Vec<FlatIndexEntry>),
}

impl RegistryClient {
    /// Return the [`CachedClient`] used by this client.
    pub fn cached_client(&self) -> &CachedClient {
        &self.client
    }

    /// Return the [`BaseClient`] used by this client.
    pub fn uncached_client(&self, url: &DisplaySafeUrl) -> &ClientWithMiddleware {
        self.client.uncached().for_host(url)
    }

    /// Returns `true` if SSL verification is disabled for the given URL.
    pub fn disable_ssl(&self, url: &DisplaySafeUrl) -> bool {
        self.client.uncached().disable_ssl(url)
    }

    /// Return the [`Connectivity`] mode used by this client.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// Return the timeout this client is configured with, in seconds.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Return the appropriate index URLs for the given [`PackageName`].
    fn index_urls_for(&self, package_name: &PackageName) -> impl Iterator<Item = IndexMetadataRef> {
        self.torch_backend
            .as_ref()
            .and_then(|torch_backend| {
                torch_backend
                    .applies_to(package_name)
                    .then(|| torch_backend.index_urls())
                    .map(|indexes| indexes.map(IndexMetadataRef::from))
            })
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(self.index_urls.indexes().map(IndexMetadataRef::from)))
    }

    /// Return the appropriate [`IndexStrategy`] for the given [`PackageName`].
    fn index_strategy_for(&self, package_name: &PackageName) -> IndexStrategy {
        self.torch_backend
            .as_ref()
            .and_then(|torch_backend| {
                torch_backend
                    .applies_to(package_name)
                    .then_some(IndexStrategy::UnsafeFirstMatch)
            })
            .unwrap_or(self.index_strategy)
    }

    /// Fetch package metadata from an index.
    ///
    /// Supports both the "Simple" API and `--find-links`-style flat indexes.
    ///
    /// "Simple" here refers to [PEP 503 – Simple Repository API](https://peps.python.org/pep-0503/)
    /// and [PEP 691 – JSON-based Simple API for Python Package Indexes](https://peps.python.org/pep-0691/),
    /// which the PyPI JSON API implements.
    #[instrument(skip_all, fields(package = % package_name))]
    pub async fn package_metadata<'index>(
        &'index self,
        package_name: &PackageName,
        index: Option<IndexMetadataRef<'index>>,
        capabilities: &IndexCapabilities,
        download_concurrency: &Semaphore,
    ) -> Result<Vec<(&'index IndexUrl, MetadataFormat)>, Error> {
        // If `--no-index` is specified, avoid fetching regardless of whether the index is implicit,
        // explicit, etc.
        if self.index_urls.no_index() {
            return Err(ErrorKind::NoIndex(package_name.to_string()).into());
        }

        let indexes = if let Some(index) = index {
            Either::Left(std::iter::once(index))
        } else {
            Either::Right(self.index_urls_for(package_name))
        };

        let mut results = Vec::new();

        match self.index_strategy_for(package_name) {
            // If we're searching for the first index that contains the package, fetch serially.
            IndexStrategy::FirstIndex => {
                for index in indexes {
                    let _permit = download_concurrency.acquire().await;
                    match index.format {
                        IndexFormat::Simple => {
                            let status_code_strategy =
                                self.index_urls.status_code_strategy_for(index.url);
                            match self
                                .simple_single_index(
                                    package_name,
                                    index.url,
                                    capabilities,
                                    &status_code_strategy,
                                )
                                .await?
                            {
                                SimpleMetadataSearchOutcome::Found(metadata) => {
                                    results.push((index.url, MetadataFormat::Simple(metadata)));
                                    break;
                                }
                                // Package not found, so we will continue on to the next index (if there is one)
                                SimpleMetadataSearchOutcome::NotFound => {}
                                // The search failed because of an HTTP status code that we don't ignore for
                                // this index. We end our search here.
                                SimpleMetadataSearchOutcome::StatusCodeFailure(status_code) => {
                                    debug!(
                                        "Indexes search failed because of status code failure: {status_code}"
                                    );
                                    break;
                                }
                            }
                        }
                        IndexFormat::Flat => {
                            let entries = self.flat_single_index(package_name, index.url).await?;
                            if !entries.is_empty() {
                                results.push((index.url, MetadataFormat::Flat(entries)));
                                break;
                            }
                        }
                    }
                }
            }

            // Otherwise, fetch concurrently.
            IndexStrategy::UnsafeBestMatch | IndexStrategy::UnsafeFirstMatch => {
                results = futures::stream::iter(indexes)
                    .map(async |index| {
                        let _permit = download_concurrency.acquire().await;
                        match index.format {
                            IndexFormat::Simple => {
                                // For unsafe matches, ignore authentication failures.
                                let status_code_strategy =
                                    IndexStatusCodeStrategy::ignore_authentication_error_codes();
                                let metadata = match self
                                    .simple_single_index(
                                        package_name,
                                        index.url,
                                        capabilities,
                                        &status_code_strategy,
                                    )
                                    .await?
                                {
                                    SimpleMetadataSearchOutcome::Found(metadata) => Some(metadata),
                                    _ => None,
                                };
                                Ok((index.url, metadata.map(MetadataFormat::Simple)))
                            }
                            IndexFormat::Flat => {
                                let entries =
                                    self.flat_single_index(package_name, index.url).await?;
                                Ok((index.url, Some(MetadataFormat::Flat(entries))))
                            }
                        }
                    })
                    .buffered(8)
                    .filter_map(async |result: Result<_, Error>| match result {
                        Ok((index, Some(metadata))) => Some(Ok((index, metadata))),
                        Ok((_, None)) => None,
                        Err(err) => Some(Err(err)),
                    })
                    .try_collect::<Vec<_>>()
                    .await?;
            }
        }

        if results.is_empty() {
            return match self.connectivity {
                Connectivity::Online => {
                    Err(ErrorKind::PackageNotFound(package_name.to_string()).into())
                }
                Connectivity::Offline => Err(ErrorKind::Offline(package_name.to_string()).into()),
            };
        }

        Ok(results)
    }

    /// Fetch the [`FlatIndexEntry`] entries for a given package from a single `--find-links` index.
    async fn flat_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
    ) -> Result<Vec<FlatIndexEntry>, Error> {
        // Store the flat index entries in a cache, to avoid redundant fetches. A flat index will
        // typically contain entries for multiple packages; as such, it's more efficient to cache
        // the entire index rather than re-fetching it for each package.
        let mut cache = self.flat_indexes.lock().await;
        if let Some(entries) = cache.get(index) {
            return Ok(entries.get(package_name).cloned().unwrap_or_default());
        }

        let client = FlatIndexClient::new(self.cached_client(), self.connectivity, &self.cache);

        // Fetch the entries for the index.
        let FlatIndexEntries { entries, .. } =
            client.fetch_index(index).await.map_err(ErrorKind::Flat)?;

        // Index by package name.
        let mut entries_by_package: FxHashMap<PackageName, Vec<FlatIndexEntry>> =
            FxHashMap::default();
        for entry in entries {
            entries_by_package
                .entry(entry.filename.name().clone())
                .or_default()
                .push(entry);
        }
        let package_entries = entries_by_package
            .get(package_name)
            .cloned()
            .unwrap_or_default();

        // Write to the cache.
        cache.insert(index.clone(), entries_by_package);

        Ok(package_entries)
    }

    /// Fetch the [`SimpleMetadata`] from a single index for a given package.
    ///
    /// The index can either be a PEP 503-compatible remote repository, or a local directory laid
    /// out in the same format.
    async fn simple_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
        capabilities: &IndexCapabilities,
        status_code_strategy: &IndexStatusCodeStrategy,
    ) -> Result<SimpleMetadataSearchOutcome, Error> {
        // Format the URL for PyPI.
        let mut url = index.url().clone();
        url.path_segments_mut()
            .map_err(|()| ErrorKind::CannotBeABase(index.url().clone()))?
            .pop_if_empty()
            .push(package_name.as_ref())
            // The URL *must* end in a trailing slash for proper relative path behavior
            // ref https://github.com/servo/rust-url/issues/333
            .push("");

        trace!("Fetching metadata for {package_name} from {url}");

        let cache_entry = self.cache.entry(
            CacheBucket::Simple,
            WheelCache::Index(index).root(),
            format!("{package_name}.rkyv"),
        );
        let cache_control = match self.connectivity {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, Some(package_name), None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = cache_entry.with_file(format!("{package_name}.lock"));
            lock_entry.lock().await.map_err(ErrorKind::CacheWrite)?
        };

        let result = if matches!(index, IndexUrl::Path(_)) {
            self.fetch_local_index(package_name, &url).await
        } else {
            self.fetch_remote_index(package_name, &url, &cache_entry, cache_control)
                .await
        };

        match result {
            Ok(metadata) => Ok(SimpleMetadataSearchOutcome::Found(metadata)),
            Err(err) => match err.into_kind() {
                // The package could not be found in the remote index.
                ErrorKind::WrappedReqwestError(url, err) => {
                    let Some(status_code) = err.status() else {
                        return Err(ErrorKind::WrappedReqwestError(url, err).into());
                    };
                    let decision =
                        status_code_strategy.handle_status_code(status_code, index, capabilities);
                    if let IndexStatusCodeDecision::Fail(status_code) = decision {
                        if !matches!(
                            status_code,
                            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                        ) {
                            return Err(ErrorKind::WrappedReqwestError(url, err).into());
                        }
                    }
                    Ok(SimpleMetadataSearchOutcome::from(decision))
                }

                // The package is unavailable due to a lack of connectivity.
                ErrorKind::Offline(_) => Ok(SimpleMetadataSearchOutcome::NotFound),

                // The package could not be found in the local index.
                ErrorKind::FileNotFound(_) => Ok(SimpleMetadataSearchOutcome::NotFound),

                err => Err(err.into()),
            },
        }
    }

    /// Fetch the [`SimpleMetadata`] from a remote URL, using the PEP 503 Simple Repository API.
    async fn fetch_remote_index(
        &self,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
    ) -> Result<OwnedArchive<SimpleMetadata>, Error> {
        let simple_request = self
            .uncached_client(url)
            .get(Url::from(url.clone()))
            .header("Accept-Encoding", "gzip")
            .header("Accept", MediaType::accepts())
            .build()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = DisplaySafeUrl::from(response.url().clone());

                let content_type = response
                    .headers()
                    .get("content-type")
                    .ok_or_else(|| Error::from(ErrorKind::MissingContentType(url.clone())))?;
                let content_type = content_type.to_str().map_err(|err| {
                    Error::from(ErrorKind::InvalidContentTypeHeader(url.clone(), err))
                })?;
                let media_type = content_type.split(';').next().unwrap_or(content_type);
                let media_type = MediaType::from_str(media_type).ok_or_else(|| {
                    Error::from(ErrorKind::UnsupportedMediaType(
                        url.clone(),
                        media_type.to_string(),
                    ))
                })?;

                let unarchived = match media_type {
                    MediaType::Json => {
                        let bytes = response
                            .bytes()
                            .await
                            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
                        let data: SimpleJson = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleMetadata::from_files(data.files, package_name, &url)
                    }
                    MediaType::Html => {
                        let text = response
                            .text()
                            .await
                            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
                        SimpleMetadata::from_html(&text, package_name, &url)?
                    }
                };
                OwnedArchive::from_unarchived(&unarchived)
            }
            .boxed_local()
            .instrument(info_span!("parse_simple_api", package = %package_name))
        };
        self.cached_client()
            .get_cacheable_with_retry(
                simple_request,
                cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await
            .map_err(|err| match err {
                CachedClientError::Client(err) => err,
                CachedClientError::Callback(err) => err,
            })
    }

    /// Fetch the [`SimpleMetadata`] from a local file, using a PEP 503-compatible directory
    /// structure.
    async fn fetch_local_index(
        &self,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
    ) -> Result<OwnedArchive<SimpleMetadata>, Error> {
        let path = url
            .to_file_path()
            .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?
            .join("index.html");
        let text = match fs_err::tokio::read_to_string(&path).await {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::from(ErrorKind::FileNotFound(
                    package_name.to_string(),
                )));
            }
            Err(err) => {
                return Err(Error::from(ErrorKind::Io(err)));
            }
        };
        let metadata = SimpleMetadata::from_html(&text, package_name, url)?;
        OwnedArchive::from_unarchived(&metadata)
    }

    /// Fetch the metadata for a remote wheel file.
    ///
    /// For a remote wheel, we try the following ways to fetch the metadata:
    /// 1. From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
    /// 2. From a remote wheel by partial zip reading
    /// 3. From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
    #[instrument(skip_all, fields(% built_dist))]
    pub async fn wheel_metadata(
        &self,
        built_dist: &BuiltDist,
        capabilities: &IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        let metadata = match &built_dist {
            BuiltDist::Registry(wheels) => {
                #[derive(Debug, Clone)]
                enum WheelLocation {
                    /// A local file path.
                    Path(PathBuf),
                    /// A remote URL.
                    Url(DisplaySafeUrl),
                }

                let wheel = wheels.best_wheel();

                let location = match &wheel.file.url {
                    FileLocation::RelativeUrl(base, url) => {
                        let url = uv_pypi_types::base_url_join_relative(base, url)
                            .map_err(ErrorKind::JoinRelativeUrl)?;
                        if url.scheme() == "file" {
                            let path = url
                                .to_file_path()
                                .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?;
                            WheelLocation::Path(path)
                        } else {
                            WheelLocation::Url(url)
                        }
                    }
                    FileLocation::AbsoluteUrl(url) => {
                        let url = url.to_url().map_err(ErrorKind::InvalidUrl)?;
                        if url.scheme() == "file" {
                            let path = url
                                .to_file_path()
                                .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?;
                            WheelLocation::Path(path)
                        } else {
                            WheelLocation::Url(url)
                        }
                    }
                };

                match location {
                    WheelLocation::Path(path) => {
                        let file = fs_err::tokio::File::open(&path)
                            .await
                            .map_err(ErrorKind::Io)?;
                        let reader = tokio::io::BufReader::new(file);
                        let contents = read_metadata_async_seek(&wheel.filename, reader)
                            .await
                            .map_err(|err| {
                                ErrorKind::Metadata(path.to_string_lossy().to_string(), err)
                            })?;
                        ResolutionMetadata::parse_metadata(&contents).map_err(|err| {
                            ErrorKind::MetadataParseError(
                                wheel.filename.clone(),
                                built_dist.to_string(),
                                Box::new(err),
                            )
                        })?
                    }
                    WheelLocation::Url(url) => {
                        self.wheel_metadata_registry(&wheel.index, &wheel.file, &url, capabilities)
                            .await?
                    }
                }
            }
            BuiltDist::DirectUrl(wheel) => {
                self.wheel_metadata_no_pep658(
                    &wheel.filename,
                    &wheel.url,
                    None,
                    WheelCache::Url(&wheel.url),
                    capabilities,
                )
                .await?
            }
            BuiltDist::Path(wheel) => {
                let file = fs_err::tokio::File::open(wheel.install_path.as_ref())
                    .await
                    .map_err(ErrorKind::Io)?;
                let reader = tokio::io::BufReader::new(file);
                let contents = read_metadata_async_seek(&wheel.filename, reader)
                    .await
                    .map_err(|err| {
                        ErrorKind::Metadata(wheel.install_path.to_string_lossy().to_string(), err)
                    })?;
                ResolutionMetadata::parse_metadata(&contents).map_err(|err| {
                    ErrorKind::MetadataParseError(
                        wheel.filename.clone(),
                        built_dist.to_string(),
                        Box::new(err),
                    )
                })?
            }
        };

        if metadata.name != *built_dist.name() {
            return Err(Error::from(ErrorKind::NameMismatch {
                metadata: metadata.name,
                given: built_dist.name().clone(),
            }));
        }

        Ok(metadata)
    }

    /// Fetch the metadata from a wheel file.
    async fn wheel_metadata_registry(
        &self,
        index: &IndexUrl,
        file: &File,
        url: &DisplaySafeUrl,
        capabilities: &IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        // If the metadata file is available at its own url (PEP 658), download it from there.
        let filename = WheelFilename::from_str(&file.filename).map_err(ErrorKind::WheelFilename)?;
        if file.dist_info_metadata {
            let mut url = url.clone();
            let path = format!("{}.metadata", url.path());
            url.set_path(&path);

            let cache_entry = self.cache.entry(
                CacheBucket::Wheels,
                WheelCache::Index(index).wheel_dir(filename.name.as_ref()),
                format!("{}.msgpack", filename.cache_key()),
            );
            let cache_control = match self.connectivity {
                Connectivity::Online => CacheControl::from(
                    self.cache
                        .freshness(&cache_entry, Some(&filename.name), None)
                        .map_err(ErrorKind::Io)?,
                ),
                Connectivity::Offline => CacheControl::AllowStale,
            };

            // Acquire an advisory lock, to guard against concurrent writes.
            #[cfg(windows)]
            let _lock = {
                let lock_entry = cache_entry.with_file(format!("{}.lock", filename.stem()));
                lock_entry.lock().await.map_err(ErrorKind::CacheWrite)?
            };

            let response_callback = async |response: Response| {
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

                info_span!("parse_metadata21")
                    .in_scope(|| ResolutionMetadata::parse_metadata(bytes.as_ref()))
                    .map_err(|err| {
                        Error::from(ErrorKind::MetadataParseError(
                            filename.clone(),
                            url.to_string(),
                            Box::new(err),
                        ))
                    })
            };
            let req = self
                .uncached_client(&url)
                .get(Url::from(url.clone()))
                .build()
                .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
            Ok(self
                .cached_client()
                .get_serde_with_retry(req, &cache_entry, cache_control, response_callback)
                .await?)
        } else {
            // If we lack PEP 658 support, try using HTTP range requests to read only the
            // `.dist-info/METADATA` file from the zip, and if that also fails, download the whole wheel
            // into the cache and read from there
            self.wheel_metadata_no_pep658(
                &filename,
                url,
                Some(index),
                WheelCache::Index(index),
                capabilities,
            )
            .await
        }
    }

    /// Get the wheel metadata if it isn't available in an index through PEP 658
    async fn wheel_metadata_no_pep658<'data>(
        &self,
        filename: &'data WheelFilename,
        url: &'data DisplaySafeUrl,
        index: Option<&'data IndexUrl>,
        cache_shard: WheelCache<'data>,
        capabilities: &'data IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::Wheels,
            cache_shard.wheel_dir(filename.name.as_ref()),
            format!("{}.msgpack", filename.cache_key()),
        );
        let cache_control = match self.connectivity {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, Some(&filename.name), None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        // Acquire an advisory lock, to guard against concurrent writes.
        #[cfg(windows)]
        let _lock = {
            let lock_entry = cache_entry.with_file(format!("{}.lock", filename.stem()));
            lock_entry.lock().await.map_err(ErrorKind::CacheWrite)?
        };

        // Attempt to fetch via a range request.
        if index.is_none_or(|index| capabilities.supports_range_requests(index)) {
            let req = self
                .uncached_client(url)
                .head(Url::from(url.clone()))
                .header(
                    "accept-encoding",
                    http::HeaderValue::from_static("identity"),
                )
                .build()
                .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

            // Copy authorization headers from the HEAD request to subsequent requests
            let mut headers = HeaderMap::default();
            if let Some(authorization) = req.headers().get("authorization") {
                headers.append("authorization", authorization.clone());
            }

            // This response callback is special, we actually make a number of subsequent requests to
            // fetch the file from the remote zip.
            let read_metadata_range_request = |response: Response| {
                async {
                    let mut reader = AsyncHttpRangeReader::from_head_response(
                        self.uncached_client(url).clone(),
                        response,
                        Url::from(url.clone()),
                        headers.clone(),
                    )
                    .await
                    .map_err(|err| ErrorKind::AsyncHttpRangeReader(url.clone(), err))?;
                    trace!("Getting metadata for {filename} by range request");
                    let text = wheel_metadata_from_remote_zip(filename, url, &mut reader).await?;
                    let metadata =
                        ResolutionMetadata::parse_metadata(text.as_bytes()).map_err(|err| {
                            Error::from(ErrorKind::MetadataParseError(
                                filename.clone(),
                                url.to_string(),
                                Box::new(err),
                            ))
                        })?;
                    Ok::<ResolutionMetadata, CachedClientError<Error>>(metadata)
                }
                .boxed_local()
                .instrument(info_span!("read_metadata_range_request", wheel = %filename))
            };

            let result = self
                .cached_client()
                .get_serde_with_retry(
                    req,
                    &cache_entry,
                    cache_control,
                    read_metadata_range_request,
                )
                .await
                .map_err(crate::Error::from);

            match result {
                Ok(metadata) => return Ok(metadata),
                Err(err) => {
                    if err.is_http_range_requests_unsupported() {
                        // The range request version failed. Fall back to streaming the file to search
                        // for the METADATA file.
                        warn!("Range requests not supported for {filename}; streaming wheel");

                        // Mark the index as not supporting range requests.
                        if let Some(index) = index {
                            capabilities.set_no_range_requests(index.clone());
                        }
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        // Create a request to stream the file.
        let req = self
            .uncached_client(url)
            .get(Url::from(url.clone()))
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
            .build()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

        // Stream the file, searching for the METADATA.
        let read_metadata_stream = |response: Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                read_metadata_async_stream(filename, url.as_ref(), reader)
                    .await
                    .map_err(|err| ErrorKind::Metadata(url.to_string(), err))
            }
            .instrument(info_span!("read_metadata_stream", wheel = %filename))
        };

        self.cached_client()
            .get_serde_with_retry(req, &cache_entry, cache_control, read_metadata_stream)
            .await
            .map_err(crate::Error::from)
    }

    /// Handle a specific `reqwest` error, and convert it to [`io::Error`].
    fn handle_response_errors(&self, err: reqwest::Error) -> std::io::Error {
        if err.is_timeout() {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",
                    self.timeout().as_secs()
                ),
            )
        } else {
            std::io::Error::other(err)
        }
    }
}

#[derive(Debug)]
pub(crate) enum SimpleMetadataSearchOutcome {
    /// Simple metadata was found
    Found(OwnedArchive<SimpleMetadata>),
    /// Simple metadata was not found
    NotFound,
    /// A status code failure was encountered when searching for
    /// simple metadata and our strategy did not ignore it
    StatusCodeFailure(StatusCode),
}

impl From<IndexStatusCodeDecision> for SimpleMetadataSearchOutcome {
    fn from(item: IndexStatusCodeDecision) -> Self {
        match item {
            IndexStatusCodeDecision::Ignore => Self::NotFound,
            IndexStatusCodeDecision::Fail(status_code) => Self::StatusCodeFailure(status_code),
        }
    }
}

/// A map from [`IndexUrl`] to [`FlatIndexEntry`] entries found at the given URL, indexed by
/// [`PackageName`].
#[derive(Default, Debug, Clone)]
struct FlatIndexCache(FxHashMap<IndexUrl, FxHashMap<PackageName, Vec<FlatIndexEntry>>>);

impl FlatIndexCache {
    /// Get the entries for a given index URL.
    fn get(&self, index: &IndexUrl) -> Option<&FxHashMap<PackageName, Vec<FlatIndexEntry>>> {
        self.0.get(index)
    }

    /// Insert the entries for a given index URL.
    fn insert(
        &mut self,
        index: IndexUrl,
        entries: FxHashMap<PackageName, Vec<FlatIndexEntry>>,
    ) -> Option<FxHashMap<PackageName, Vec<FlatIndexEntry>>> {
        self.0.insert(index, entries)
    }
}

#[derive(Default, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct VersionFiles {
    pub wheels: Vec<VersionWheel>,
    pub source_dists: Vec<VersionSourceDist>,
}

impl VersionFiles {
    fn push(&mut self, filename: DistFilename, file: File) {
        match filename {
            DistFilename::WheelFilename(name) => self.wheels.push(VersionWheel { name, file }),
            DistFilename::SourceDistFilename(name) => {
                self.source_dists.push(VersionSourceDist { name, file });
            }
        }
    }

    pub fn all(self) -> impl Iterator<Item = (DistFilename, File)> {
        self.source_dists
            .into_iter()
            .map(|VersionSourceDist { name, file }| (DistFilename::SourceDistFilename(name), file))
            .chain(
                self.wheels
                    .into_iter()
                    .map(|VersionWheel { name, file }| (DistFilename::WheelFilename(name), file)),
            )
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct VersionWheel {
    pub name: WheelFilename,
    pub file: File,
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct VersionSourceDist {
    pub name: SourceDistFilename,
    pub file: File,
}

#[derive(Default, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct SimpleMetadata(Vec<SimpleMetadatum>);

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct SimpleMetadatum {
    pub version: Version,
    pub files: VersionFiles,
}

impl SimpleMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &SimpleMetadatum> {
        self.0.iter()
    }

    fn from_files(files: Vec<uv_pypi_types::File>, package_name: &PackageName, base: &Url) -> Self {
        let mut map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Convert to a reference-counted string.
        let base = SmallString::from(base.as_str());

        // Group the distributions by version and kind
        for file in files {
            let Some(filename) = DistFilename::try_from_filename(&file.filename, package_name)
            else {
                warn!("Skipping file for {package_name}: {}", file.filename);
                continue;
            };
            let version = match filename {
                DistFilename::SourceDistFilename(ref inner) => &inner.version,
                DistFilename::WheelFilename(ref inner) => &inner.version,
            };
            let file = match File::try_from(file, &base) {
                Ok(file) => file,
                Err(err) => {
                    // Ignore files with unparsable version specifiers.
                    warn!("Skipping file for {package_name}: {err}");
                    continue;
                }
            };
            match map.entry(version.clone()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().push(filename, file);
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let mut files = VersionFiles::default();
                    files.push(filename, file);
                    entry.insert(files);
                }
            }
        }
        Self(
            map.into_iter()
                .map(|(version, files)| SimpleMetadatum { version, files })
                .collect(),
        )
    }

    /// Read the [`SimpleMetadata`] from an HTML index.
    fn from_html(
        text: &str,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
    ) -> Result<Self, Error> {
        let SimpleHtml { base, files } =
            SimpleHtml::parse(text, url).map_err(|err| Error::from_html_err(err, url.clone()))?;

        Ok(SimpleMetadata::from_files(
            files,
            package_name,
            base.as_url(),
        ))
    }
}

impl IntoIterator for SimpleMetadata {
    type Item = SimpleMetadatum;
    type IntoIter = std::vec::IntoIter<SimpleMetadatum>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl ArchivedSimpleMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &rkyv::Archived<SimpleMetadatum>> {
        self.0.iter()
    }

    pub fn datum(&self, i: usize) -> Option<&rkyv::Archived<SimpleMetadatum>> {
        self.0.get(i)
    }
}

#[derive(Debug)]
enum MediaType {
    Json,
    Html,
}

impl MediaType {
    /// Parse a media type from a string, returning `None` if the media type is not supported.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "application/vnd.pypi.simple.v1+json" => Some(Self::Json),
            "application/vnd.pypi.simple.v1+html" | "text/html" => Some(Self::Html),
            _ => None,
        }
    }

    /// Return the `Accept` header value for all supported media types.
    #[inline]
    const fn accepts() -> &'static str {
        // See: https://peps.python.org/pep-0691/#version-format-selection
        "application/vnd.pypi.simple.v1+json, application/vnd.pypi.simple.v1+html;q=0.2, text/html;q=0.01"
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum Connectivity {
    /// Allow access to the network.
    #[default]
    Online,

    /// Do not allow access to the network.
    Offline,
}

impl Connectivity {
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online)
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;
    use uv_pypi_types::{JoinRelativeError, SimpleJson};
    use uv_redacted::DisplaySafeUrl;

    use crate::{SimpleMetadata, SimpleMetadatum, html::SimpleHtml};

    #[test]
    fn ignore_failing_files() {
        // 1.7.7 has an invalid requires-python field (double comma), 1.7.8 is valid
        let response = r#"
    {
        "files": [
        {
            "core-metadata": false,
            "data-dist-info-metadata": false,
            "filename": "pyflyby-1.7.7.tar.gz",
            "hashes": {
            "sha256": "0c4d953f405a7be1300b440dbdbc6917011a07d8401345a97e72cd410d5fb291"
            },
            "requires-python": ">=2.5, !=3.0.*, !=3.1.*, !=3.2.*, !=3.2.*, !=3.3.*, !=3.4.*,, !=3.5.*, !=3.6.*, <4",
            "size": 427200,
            "upload-time": "2022-05-19T09:14:36.591835Z",
            "url": "https://files.pythonhosted.org/packages/61/93/9fec62902d0b4fc2521333eba047bff4adbba41f1723a6382367f84ee522/pyflyby-1.7.7.tar.gz",
            "yanked": false
        },
        {
            "core-metadata": false,
            "data-dist-info-metadata": false,
            "filename": "pyflyby-1.7.8.tar.gz",
            "hashes": {
            "sha256": "1ee37474f6da8f98653dbcc208793f50b7ace1d9066f49e2707750a5ba5d53c6"
            },
            "requires-python": ">=2.5, !=3.0.*, !=3.1.*, !=3.2.*, !=3.2.*, !=3.3.*, !=3.4.*, !=3.5.*, !=3.6.*, <4",
            "size": 424460,
            "upload-time": "2022-08-04T10:42:02.190074Z",
            "url": "https://files.pythonhosted.org/packages/ad/39/17180d9806a1c50197bc63b25d0f1266f745fc3b23f11439fccb3d6baa50/pyflyby-1.7.8.tar.gz",
            "yanked": false
        }
        ]
    }
    "#;
        let data: SimpleJson = serde_json::from_str(response).unwrap();
        let base = DisplaySafeUrl::parse("https://pypi.org/simple/pyflyby/").unwrap();
        let simple_metadata = SimpleMetadata::from_files(
            data.files,
            &PackageName::from_str("pyflyby").unwrap(),
            &base,
        );
        let versions: Vec<String> = simple_metadata
            .iter()
            .map(|SimpleMetadatum { version, .. }| version.to_string())
            .collect();
        assert_eq!(versions, ["1.7.8".to_string()]);
    }

    /// Test for AWS Code Artifact registry
    ///
    /// See: <https://github.com/astral-sh/uv/issues/1388>
    #[test]
    fn relative_urls_code_artifact() -> Result<(), JoinRelativeError> {
        let text = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Links for flask</title>
        </head>
        <body>
            <h1>Links for flask</h1>
            <a href="0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237"  data-gpg-sig="false" >Flask-0.1.tar.gz</a>
            <br/>
            <a href="0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373"  data-gpg-sig="false" >Flask-0.10.1.tar.gz</a>
            <br/>
            <a href="3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403" data-requires-python="&gt;=3.8" data-gpg-sig="false" >flask-3.0.1.tar.gz</a>
            <br/>
        </body>
        </html>
    "#;

        // Note the lack of a trailing `/` here is important for coverage of url-join behavior
        let base = DisplaySafeUrl::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask")
            .unwrap();
        let SimpleHtml { base, files } = SimpleHtml::parse(text, &base).unwrap();

        // Test parsing of the file urls
        let urls = files
            .iter()
            .map(|file| uv_pypi_types::base_url_join_relative(base.as_url().as_str(), &file.url))
            .collect::<Result<Vec<_>, JoinRelativeError>>()?;
        let urls = urls
            .iter()
            .map(DisplaySafeUrl::to_string)
            .collect::<Vec<_>>();
        insta::assert_debug_snapshot!(urls, @r#"
        [
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.1/Flask-0.1.tar.gz",
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.10.1/Flask-0.10.1.tar.gz",
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/3.0.1/flask-3.0.1.tar.gz",
        ]
        "#);

        Ok(())
    }
}
