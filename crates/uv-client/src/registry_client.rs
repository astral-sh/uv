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
use rustc_hash::FxHashMap;
use tokio::sync::{Mutex, Semaphore};
use tracing::{Instrument, debug, info_span, instrument, trace, warn};
use url::Url;

use uv_auth::{Indexes, PyxTokenStore};
use uv_cache::{Cache, CacheBucket, CacheEntry, WheelCache};
use uv_configuration::IndexStrategy;
use uv_configuration::KeyringProviderType;
use uv_distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use uv_distribution_types::{
    BuiltDist, File, IndexCapabilities, IndexEntryFilename, IndexFormat, IndexLocations,
    IndexMetadataRef, IndexStatusCodeDecision, IndexStatusCodeStrategy, IndexUrl, IndexUrls, Name,
    RegistryVariantsJson, VariantsJson,
};
use uv_metadata::{read_metadata_async_seek, read_metadata_async_stream};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_pypi_types::{PypiSimpleDetail, PyxSimpleDetail, ResolutionMetadata};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;
use uv_torch::TorchStrategy;
use uv_variants::variants_json::VariantsJsonContent;

use crate::base_client::{BaseClientBuilder, ExtraMiddleware, RedirectPolicy};
use crate::cached_client::CacheControl;
use crate::flat_index::FlatIndexEntry;
use crate::html::SimpleHtml;
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::rkyvutil::OwnedArchive;
use crate::{
    BaseClient, CachedClient, Error, ErrorKind, FlatIndexClient, FlatIndexEntries,
    RedirectClientWithMiddleware,
};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder<'a> {
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchStrategy>,
    cache: Cache,
    base_client_builder: BaseClientBuilder<'a>,
}

impl<'a> RegistryClientBuilder<'a> {
    pub fn new(base_client_builder: BaseClientBuilder<'a>, cache: Cache) -> Self {
        Self {
            index_locations: IndexLocations::default(),
            index_strategy: IndexStrategy::default(),
            torch_backend: None,
            cache,
            base_client_builder,
        }
    }

    #[must_use]
    pub fn with_reqwest_client(mut self, client: reqwest::Client) -> Self {
        self.base_client_builder = self.base_client_builder.custom_client(client);
        self
    }

    #[must_use]
    pub fn index_locations(mut self, index_locations: IndexLocations) -> Self {
        self.index_locations = index_locations;
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
    pub fn built_in_root_certs(mut self, built_in_root_certs: bool) -> Self {
        self.base_client_builder = self
            .base_client_builder
            .built_in_root_certs(built_in_root_certs);
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

    /// Allows credentials to be propagated on cross-origin redirects.
    ///
    /// WARNING: This should only be available for tests. In production code, propagating credentials
    /// during cross-origin redirects can lead to security vulnerabilities including credential
    /// leakage to untrusted domains.
    #[cfg(test)]
    #[must_use]
    pub fn allow_cross_origin_credentials(mut self) -> Self {
        self.base_client_builder = self.base_client_builder.allow_cross_origin_credentials();
        self
    }

    pub fn build(self) -> RegistryClient {
        self.index_locations.cache_index_credentials();
        let index_urls = self.index_locations.index_urls();

        // Build a base client
        let builder = self
            .base_client_builder
            .indexes(Indexes::from(&self.index_locations))
            .redirect(RedirectPolicy::RetriggerMiddleware);

        let client = builder.build();

        let timeout = client.timeout();
        let connectivity = client.connectivity();

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        RegistryClient {
            index_urls,
            index_strategy: self.index_strategy,
            torch_backend: self.torch_backend,
            cache: self.cache,
            connectivity,
            client,
            timeout,
            flat_indexes: Arc::default(),
            pyx_token_store: PyxTokenStore::from_settings().ok(),
        }
    }

    /// Share the underlying client between two different middleware configurations.
    pub fn wrap_existing(self, existing: &BaseClient) -> RegistryClient {
        self.index_locations.cache_index_credentials();
        let index_urls = self.index_locations.index_urls();

        // Wrap in any relevant middleware and handle connectivity.
        let client = self
            .base_client_builder
            .indexes(Indexes::from(&self.index_locations))
            .wrap_existing(existing);

        let timeout = client.timeout();
        let connectivity = client.connectivity();

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        RegistryClient {
            index_urls,
            index_strategy: self.index_strategy,
            torch_backend: self.torch_backend,
            cache: self.cache,
            connectivity,
            client,
            timeout,
            flat_indexes: Arc::default(),
            pyx_token_store: PyxTokenStore::from_settings().ok(),
        }
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
    /// The pyx token store to use for persistent credentials.
    // TODO(charlie): The token store is only needed for `is_known_url`; can we avoid storing it here?
    pyx_token_store: Option<PyxTokenStore>,
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
    pub fn uncached_client(&self, url: &DisplaySafeUrl) -> &RedirectClientWithMiddleware {
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
    fn index_urls_for(
        &self,
        package_name: &PackageName,
    ) -> impl Iterator<Item = IndexMetadataRef<'_>> {
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
            Connectivity::Online => {
                if let Some(header) = self.index_urls.simple_api_cache_control_for(index) {
                    CacheControl::Override(header)
                } else {
                    CacheControl::from(
                        self.cache
                            .freshness(&cache_entry, Some(package_name), None)
                            .map_err(ErrorKind::Io)?,
                    )
                }
            }
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
            self.fetch_remote_index(package_name, &url, index, &cache_entry, cache_control)
                .await
        };

        match result {
            Ok(metadata) => Ok(SimpleMetadataSearchOutcome::Found(metadata)),
            Err(err) => match err.kind() {
                // The package could not be found in the remote index.
                ErrorKind::WrappedReqwestError(.., reqwest_err) => {
                    let Some(status_code) = reqwest_err.status() else {
                        return Err(err);
                    };
                    let decision =
                        status_code_strategy.handle_status_code(status_code, index, capabilities);
                    if let IndexStatusCodeDecision::Fail(status_code) = decision {
                        if !matches!(
                            status_code,
                            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                        ) {
                            return Err(err);
                        }
                    }
                    Ok(SimpleMetadataSearchOutcome::from(decision))
                }

                // The package is unavailable due to a lack of connectivity.
                ErrorKind::Offline(_) => Ok(SimpleMetadataSearchOutcome::NotFound),

                // The package could not be found in the local index.
                ErrorKind::FileNotFound(_) => Ok(SimpleMetadataSearchOutcome::NotFound),

                _ => Err(err),
            },
        }
    }

    /// Fetch the [`SimpleMetadata`] from a remote URL, using the PEP 503 Simple Repository API.
    async fn fetch_remote_index(
        &self,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
        index: &IndexUrl,
        cache_entry: &CacheEntry,
        cache_control: CacheControl<'_>,
    ) -> Result<OwnedArchive<SimpleMetadata>, Error> {
        // In theory, we should be able to pass `MediaType::all()` to all registries, and as
        // unsupported media types should be ignored by the server. For now, we implement this
        // defensively to avoid issues with misconfigured servers.
        let accept = if self
            .pyx_token_store
            .as_ref()
            .is_some_and(|token_store| token_store.is_known_url(index.url()))
        {
            MediaType::all()
        } else {
            MediaType::pypi()
        };
        let simple_request = self
            .uncached_client(url)
            .get(Url::from(url.clone()))
            .header("Accept-Encoding", "gzip, deflate, zstd")
            .header("Accept", accept)
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
                    MediaType::PyxV1Msgpack => {
                        let bytes = response
                            .bytes()
                            .await
                            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
                        let data: PyxSimpleDetail = rmp_serde::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_msgpack_err(err, url.clone()))?;

                        SimpleMetadata::from_pyx_files(
                            data.files,
                            data.core_metadata,
                            package_name,
                            &url,
                        )
                    }
                    MediaType::PyxV1Json => {
                        let bytes = response
                            .bytes()
                            .await
                            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
                        let data: PyxSimpleDetail = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleMetadata::from_pyx_files(
                            data.files,
                            data.core_metadata,
                            package_name,
                            &url,
                        )
                    }
                    MediaType::PypiV1Json => {
                        let bytes = response
                            .bytes()
                            .await
                            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

                        let data: PypiSimpleDetail = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleMetadata::from_pypi_files(data.files, package_name, &url)
                    }
                    MediaType::PypiV1Html | MediaType::TextHtml => {
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
        let simple = self
            .cached_client()
            .get_cacheable_with_retry(
                simple_request,
                cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await?;
        Ok(simple)
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

    /// Fetch the variants.json contents from a remote index (cached) a local index.
    pub async fn fetch_variants_json(
        &self,
        variants_json: &RegistryVariantsJson,
    ) -> Result<VariantsJsonContent, Error> {
        let url = variants_json
            .file
            .url
            .to_url()
            .map_err(ErrorKind::InvalidUrl)?;

        // If the URL is a file URL, load the variants directly from the file system.
        let variants_json = if url.scheme() == "file" {
            let path = url
                .to_file_path()
                .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?;
            let bytes = match fs_err::tokio::read(&path).await {
                Ok(text) => text,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Err(Error::from(ErrorKind::FileNotFound(
                        variants_json.filename.to_string(),
                    )));
                }
                Err(err) => {
                    return Err(Error::from(ErrorKind::Io(err)));
                }
            };
            info_span!("parse_variants_json")
                .in_scope(|| serde_json::from_slice::<VariantsJsonContent>(&bytes))
                .map_err(|err| ErrorKind::VariantsJsonFormat(url, err))?
        } else {
            let cache_entry = self.cache.entry(
                CacheBucket::Wheels,
                WheelCache::Index(&variants_json.index)
                    .wheel_dir(variants_json.filename.name.as_ref()),
                format!("variants-{}.msgpack", variants_json.filename.cache_key()),
            );

            let cache_control = match self.connectivity {
                Connectivity::Online => {
                    if let Some(header) = self
                        .index_urls
                        .artifact_cache_control_for(&variants_json.index)
                    {
                        CacheControl::Override(header)
                    } else {
                        CacheControl::from(
                            self.cache
                                .freshness(&cache_entry, Some(&variants_json.filename.name), None)
                                .map_err(ErrorKind::Io)?,
                        )
                    }
                }
                Connectivity::Offline => CacheControl::AllowStale,
            };

            let response_callback = async |response: Response| {
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

                info_span!("parse_variants_json")
                    .in_scope(|| serde_json::from_slice::<VariantsJsonContent>(&bytes))
                    .map_err(|err| Error::from(ErrorKind::VariantsJsonFormat(url.clone(), err)))
            };

            let req = self
                .uncached_client(&url)
                .get(Url::from(url.clone()))
                .build()
                .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
            self.cached_client()
                .get_serde_with_retry(req, &cache_entry, cache_control, response_callback)
                .await?
        };
        Ok(variants_json)
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

                let url = wheel.file.url.to_url().map_err(ErrorKind::InvalidUrl)?;
                let location = if url.scheme() == "file" {
                    let path = url
                        .to_file_path()
                        .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?;
                    WheelLocation::Path(path)
                } else {
                    WheelLocation::Url(url)
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
                Connectivity::Online => {
                    if let Some(header) = self.index_urls.artifact_cache_control_for(index) {
                        CacheControl::Override(header)
                    } else {
                        CacheControl::from(
                            self.cache
                                .freshness(&cache_entry, Some(&filename.name), None)
                                .map_err(ErrorKind::Io)?,
                        )
                    }
                }
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
            Connectivity::Online => {
                if let Some(index) = index {
                    if let Some(header) = self.index_urls.artifact_cache_control_for(index) {
                        CacheControl::Override(header)
                    } else {
                        CacheControl::from(
                            self.cache
                                .freshness(&cache_entry, Some(&filename.name), None)
                                .map_err(ErrorKind::Io)?,
                        )
                    }
                } else {
                    CacheControl::from(
                        self.cache
                            .freshness(&cache_entry, Some(&filename.name), None)
                            .map_err(ErrorKind::Io)?,
                    )
                }
            }
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
                    ResolutionMetadata::parse_metadata(text.as_bytes()).map_err(|err| {
                        Error::from(ErrorKind::MetadataParseError(
                            filename.clone(),
                            url.to_string(),
                            Box::new(err),
                        ))
                    })
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
    pub variant_jsons: Vec<VersionVariantJson>,
}

impl VersionFiles {
    fn push(&mut self, filename: IndexEntryFilename, file: File) {
        match filename {
            IndexEntryFilename::DistFilename(DistFilename::WheelFilename(name)) => {
                self.wheels.push(VersionWheel { name, file });
            }
            IndexEntryFilename::DistFilename(DistFilename::SourceDistFilename(name)) => {
                self.source_dists.push(VersionSourceDist { name, file });
            }
            IndexEntryFilename::VariantJson(variants_json) => {
                self.variant_jsons.push(VersionVariantJson {
                    name: variants_json,
                    file,
                });
            }
        }
    }

    pub fn dists(self) -> impl Iterator<Item = (DistFilename, File)> {
        self.source_dists
            .into_iter()
            .map(|VersionSourceDist { name, file }| (DistFilename::SourceDistFilename(name), file))
            .chain(
                self.wheels
                    .into_iter()
                    .map(|VersionWheel { name, file }| (DistFilename::WheelFilename(name), file)),
            )
    }

    pub fn all(self) -> impl Iterator<Item = (IndexEntryFilename, File)> {
        self.source_dists
            .into_iter()
            .map(|VersionSourceDist { name, file }| {
                (
                    IndexEntryFilename::DistFilename(DistFilename::SourceDistFilename(name)),
                    file,
                )
            })
            .chain(self.wheels.into_iter().map(|VersionWheel { name, file }| {
                (
                    IndexEntryFilename::DistFilename(DistFilename::WheelFilename(name)),
                    file,
                )
            }))
            .chain(
                self.variant_jsons
                    .into_iter()
                    .map(|VersionVariantJson { name, file }| {
                        (IndexEntryFilename::VariantJson(name), file)
                    }),
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

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct VersionVariantJson {
    pub name: VariantsJson,
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
    pub metadata: Option<ResolutionMetadata>,
}

impl SimpleMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &SimpleMetadatum> {
        self.0.iter()
    }

    fn from_pypi_files(
        files: Vec<uv_pypi_types::PypiFile>,
        package_name: &PackageName,
        base: &Url,
    ) -> Self {
        let mut version_map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Convert to a reference-counted string.
        let base = SmallString::from(base.as_str());

        // Group the distributions by version and kind
        for file in files {
            let Some(filename) =
                IndexEntryFilename::try_from_filename(&file.filename, package_name)
            else {
                warn!("Skipping file for {package_name}: {}", file.filename);
                continue;
            };
            let file = match File::try_from_pypi(file, &base) {
                Ok(file) => file,
                Err(err) => {
                    // Ignore files with unparsable version specifiers.
                    warn!("Skipping file for {package_name}: {err}");
                    continue;
                }
            };
            match version_map.entry(filename.version().clone()) {
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
            version_map
                .into_iter()
                .map(|(version, files)| SimpleMetadatum {
                    version,
                    files,
                    metadata: None,
                })
                .collect(),
        )
    }

    fn from_pyx_files(
        files: Vec<uv_pypi_types::PyxFile>,
        mut core_metadata: FxHashMap<Version, uv_pypi_types::CoreMetadatum>,
        package_name: &PackageName,
        base: &Url,
    ) -> Self {
        let mut version_map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Convert to a reference-counted string.
        let base = SmallString::from(base.as_str());

        // Group the distributions by version and kind
        for file in files {
            let file = match File::try_from_pyx(file, &base) {
                Ok(file) => file,
                Err(err) => {
                    // Ignore files with unparsable version specifiers.
                    warn!("Skipping file for {package_name}: {err}");
                    continue;
                }
            };
            let Some(filename) =
                IndexEntryFilename::try_from_filename(&file.filename, package_name)
            else {
                warn!("Skipping file for {package_name}: {}", file.filename);
                continue;
            };
            match version_map.entry(filename.version().clone()) {
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
            version_map
                .into_iter()
                .map(|(version, files)| {
                    let metadata =
                        core_metadata
                            .remove(&version)
                            .map(|metadata| ResolutionMetadata {
                                name: package_name.clone(),
                                version: version.clone(),
                                requires_dist: metadata.requires_dist,
                                requires_python: metadata.requires_python,
                                provides_extras: metadata.provides_extras,
                                dynamic: false,
                            });
                    SimpleMetadatum {
                        version,
                        files,
                        metadata,
                    }
                })
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

        Ok(Self::from_pypi_files(files, package_name, base.as_url()))
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
    PyxV1Msgpack,
    PyxV1Json,
    PypiV1Json,
    PypiV1Html,
    TextHtml,
}

impl MediaType {
    /// Parse a media type from a string, returning `None` if the media type is not supported.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "application/vnd.pyx.simple.v1+msgpack" => Some(Self::PyxV1Msgpack),
            "application/vnd.pyx.simple.v1+json" => Some(Self::PyxV1Json),
            "application/vnd.pypi.simple.v1+json" => Some(Self::PypiV1Json),
            "application/vnd.pypi.simple.v1+html" => Some(Self::PypiV1Html),
            "text/html" => Some(Self::TextHtml),
            _ => None,
        }
    }

    /// Return the `Accept` header value for all PyPI media types.
    #[inline]
    const fn pypi() -> &'static str {
        // See: https://peps.python.org/pep-0691/#version-format-selection
        "application/vnd.pypi.simple.v1+json, application/vnd.pypi.simple.v1+html;q=0.2, text/html;q=0.01"
    }

    /// Return the `Accept` header value for all supported media types.
    #[inline]
    const fn all() -> &'static str {
        // See: https://peps.python.org/pep-0691/#version-format-selection
        "application/vnd.pyx.simple.v1+msgpack, application/vnd.pyx.simple.v1+json;q=0.9, application/vnd.pypi.simple.v1+json;q=0.8, application/vnd.pypi.simple.v1+html;q=0.2, text/html;q=0.01"
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PyxV1Msgpack => write!(f, "application/vnd.pyx.simple.v1+msgpack"),
            Self::PyxV1Json => write!(f, "application/vnd.pyx.simple.v1+json"),
            Self::PypiV1Json => write!(f, "application/vnd.pypi.simple.v1+json"),
            Self::PypiV1Html => write!(f, "application/vnd.pypi.simple.v1+html"),
            Self::TextHtml => write!(f, "text/html"),
        }
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

    use url::Url;
    use uv_normalize::PackageName;
    use uv_pypi_types::PypiSimpleDetail;
    use uv_redacted::DisplaySafeUrl;

    use crate::{BaseClientBuilder, SimpleMetadata, SimpleMetadatum, html::SimpleHtml};

    use crate::RegistryClientBuilder;
    use uv_cache::Cache;
    use uv_distribution_types::{FileLocation, ToUrlError};
    use uv_small_str::SmallString;
    use wiremock::matchers::{basic_auth, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    type Error = Box<dyn std::error::Error>;

    async fn start_test_server(username: &'static str, password: &'static str) -> MockServer {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        server
    }

    #[tokio::test]
    async fn test_redirect_to_server_with_credentials() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let auth_server = start_test_server(username, password).await;
        let auth_base_url = DisplaySafeUrl::parse(&auth_server.uri())?;

        let redirect_server = MockServer::start().await;

        // Configure the redirect server to respond with a 302 to the auth server
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("Location", format!("{auth_base_url}")),
            )
            .mount(&redirect_server)
            .await;

        let redirect_server_url = DisplaySafeUrl::parse(&redirect_server.uri())?;

        let cache = Cache::temp()?;
        let registry_client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache)
            .allow_cross_origin_credentials()
            .build();
        let client = registry_client.cached_client().uncached();

        assert_eq!(
            client
                .for_host(&redirect_server_url)
                .get(redirect_server.uri())
                .send()
                .await?
                .status(),
            401,
            "Requests should fail if credentials are missing"
        );

        let mut url = redirect_server_url.clone();
        let _ = url.set_username(username);
        let _ = url.set_password(Some(password));

        assert_eq!(
            client
                .for_host(&redirect_server_url)
                .get(Url::from(url))
                .send()
                .await?
                .status(),
            200,
            "Requests should succeed if credentials are present"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_root_relative_url() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let redirect_server = MockServer::start().await;

        // Configure the redirect server to respond with a 307 with a relative URL.
        Mock::given(method("GET"))
            .and(path_regex("/foo/"))
            .respond_with(
                ResponseTemplate::new(307).insert_header("Location", "/bar/baz/".to_string()),
            )
            .mount(&redirect_server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex("/bar/baz/"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200))
            .mount(&redirect_server)
            .await;

        let redirect_server_url = DisplaySafeUrl::parse(&redirect_server.uri())?.join("foo/")?;

        let cache = Cache::temp()?;
        let registry_client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache)
            .allow_cross_origin_credentials()
            .build();
        let client = registry_client.cached_client().uncached();

        let mut url = redirect_server_url.clone();
        let _ = url.set_username(username);
        let _ = url.set_password(Some(password));

        assert_eq!(
            client
                .for_host(&url)
                .get(Url::from(url))
                .send()
                .await?
                .status(),
            200,
            "Requests should succeed for relative URL"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_redirect_relative_url() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let redirect_server = MockServer::start().await;

        // Configure the redirect server to respond with a 307 with a relative URL.
        Mock::given(method("GET"))
            .and(path_regex("/foo/bar/baz/"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200))
            .mount(&redirect_server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex("/foo/"))
            .and(basic_auth(username, password))
            .respond_with(
                ResponseTemplate::new(307).insert_header("Location", "bar/baz/".to_string()),
            )
            .mount(&redirect_server)
            .await;

        let cache = Cache::temp()?;
        let registry_client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache)
            .allow_cross_origin_credentials()
            .build();
        let client = registry_client.cached_client().uncached();

        let redirect_server_url = DisplaySafeUrl::parse(&redirect_server.uri())?.join("foo/")?;
        let mut url = redirect_server_url.clone();
        let _ = url.set_username(username);
        let _ = url.set_password(Some(password));

        assert_eq!(
            client
                .for_host(&url)
                .get(Url::from(url))
                .send()
                .await?
                .status(),
            200,
            "Requests should succeed for relative URL"
        );

        Ok(())
    }

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
        let data: PypiSimpleDetail = serde_json::from_str(response).unwrap();
        let base = DisplaySafeUrl::parse("https://pypi.org/simple/pyflyby/").unwrap();
        let simple_metadata = SimpleMetadata::from_pypi_files(
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
    fn relative_urls_code_artifact() -> Result<(), ToUrlError> {
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
        let base = SmallString::from(base.as_str());

        // Test parsing of the file urls
        let urls = files
            .into_iter()
            .map(|file| FileLocation::new(file.url, &base).to_url())
            .collect::<Result<Vec<_>, _>>()?;
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
