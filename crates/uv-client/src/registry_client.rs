use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Debug, Formatter};
use std::path::PathBuf;
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

use uv_auth::{CredentialsCache, Indexes, PyxTokenStore};
use uv_cache::{Cache, CacheBucket, CacheEntry, WheelCache};
use uv_configuration::IndexStrategy;
use uv_configuration::KeyringProviderType;
use uv_distribution_filename::{DistFilename, WheelFilename};
use uv_distribution_types::{
    BuiltDist, File, FileLocation, IndexCapabilities, IndexFormat, IndexLocations,
    IndexMetadataRef, IndexStatusCodeDecision, IndexStatusCodeStrategy, IndexUrl, Name,
    RegistryBuiltWheel, UrlString, Zstd,
};
use uv_git::{GIT_LFS, GitError, GitHttpSettings, GitResolver, Reporter};
use uv_metadata::{read_metadata_async_seek, read_metadata_async_stream};
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{MarkerEnvironment, split_scheme};
use uv_platform_tags::Platform;
use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests, ProjectStatus, Yanked};
use uv_pypi_types::{
    PypiSimpleDetail, PypiSimpleIndex, PyxSimpleDetail, PyxSimpleIndex, ResolutionMetadata,
};
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;
use uv_torch::TorchStrategy;

use crate::base_client::{BaseClientBuilder, ClientBuildError, ExtraMiddleware, RedirectPolicy};
use crate::cached_client::CacheControl;
use crate::flat_index::FlatIndexEntry;
use crate::html::SimpleDetailHTML;
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::rkyvutil::OwnedArchive;
use crate::{
    BaseClient, CachedClient, Error, ErrorKind, FlatIndexClient, RedirectClientWithMiddleware,
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
            base_client_builder: base_client_builder.redirect(RedirectPolicy::RetriggerMiddleware),
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
    fn allow_cross_origin_credentials(mut self) -> Self {
        self.base_client_builder = self.base_client_builder.allow_cross_origin_credentials();
        self
    }

    /// Add all authenticated sources to the cache.
    fn cache_index_credentials(&mut self) -> Result<(), ClientBuildError> {
        for index in self.index_locations.known_indexes() {
            if let Some(credentials) = index.credentials()? {
                trace!(
                    "Read credentials for index {}",
                    index
                        .name
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| index.url.to_string())
                );
                if let Some(root_url) = index.root_url() {
                    self.base_client_builder
                        .store_credentials(&root_url, credentials.clone());
                }
                self.base_client_builder
                    .store_credentials(index.raw_url(), credentials);
            }
        }
        Ok(())
    }

    pub fn build(self) -> Result<RegistryClient, ClientBuildError> {
        self.build_inner(None)
    }

    /// Share the underlying client between two different middleware configurations.
    pub fn wrap_existing(self, existing: &BaseClient) -> Result<RegistryClient, ClientBuildError> {
        self.build_inner(Some(existing))
    }

    fn build_inner(
        mut self,
        existing: Option<&BaseClient>,
    ) -> Result<RegistryClient, ClientBuildError> {
        self.cache_index_credentials()?;

        // Wrap in any relevant middleware and handle connectivity.
        let builder = self
            .base_client_builder
            .indexes(Indexes::from(&self.index_locations));
        let client = if let Some(existing) = existing {
            builder.wrap_existing(existing)
        } else {
            builder.build()?
        };

        let read_timeout = client.read_timeout();
        let connectivity = client.connectivity();

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        Ok(RegistryClient {
            indexes: self.index_locations,
            index_strategy: self.index_strategy,
            torch_backend: self.torch_backend,
            cache: self.cache,
            connectivity,
            client,
            read_timeout,
            flat_indexes: Arc::default(),
            pyx_token_store: PyxTokenStore::from_settings().ok(),
        })
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// The indexes to use for fetching packages.
    indexes: IndexLocations,
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
    /// Client HTTP read timeout.
    read_timeout: Duration,
    /// The flat index entries for each `--find-links`-style index URL, with one slot per index.
    flat_indexes: Arc<Mutex<FlatIndexCache>>,
    /// The pyx token store to use for persistent credentials.
    // TODO(charlie): The token store is only needed for `is_known_url`; can we avoid storing it here?
    pyx_token_store: Option<PyxTokenStore>,
}

/// The format of the package metadata returned by querying an index.
#[derive(Debug)]
pub enum MetadataFormat {
    /// The metadata adheres to the Simple Repository API format.
    Simple(OwnedArchive<SimpleDetailMetadata>),
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

    /// Return the [`GitHttpSettings`] for fetching from the given URL.
    pub fn git_http_settings(&self, url: &DisplaySafeUrl) -> GitHttpSettings {
        self.client.uncached().git_http_settings(url)
    }

    /// Return the [`Connectivity`] mode used by this client.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// Return the timeout this client is configured with, in seconds.
    pub fn read_timeout(&self) -> Duration {
        self.read_timeout
    }

    pub fn credentials_cache(&self) -> &CredentialsCache {
        self.client.uncached().credentials_cache()
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
            .unwrap_or_else(|| {
                Either::Right(self.indexes.fetch_indexes().map(IndexMetadataRef::from))
            })
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
    pub async fn simple_detail<'index>(
        &'index self,
        package_name: &PackageName,
        index: Option<IndexMetadataRef<'index>>,
        capabilities: &IndexCapabilities,
        download_concurrency: &Semaphore,
    ) -> Result<Vec<(&'index IndexUrl, MetadataFormat)>, Error> {
        // If `--no-index` is specified, avoid fetching regardless of whether the index is implicit,
        // explicit, etc.
        if self.indexes.no_index() {
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
                                self.indexes.status_code_strategy_for(index.url);
                            match self
                                .simple_detail_single_index(
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
                                    .simple_detail_single_index(
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
                    Err(ErrorKind::RemotePackageNotFound(package_name.clone()).into())
                }
                Connectivity::Offline => Err(ErrorKind::Offline(package_name.to_string()).into()),
            };
        }

        Ok(results)
    }

    /// Fetch and combine entries for a package from the configured legacy `--find-links` locations.
    #[instrument(skip_all, fields(package = % package_name))]
    pub async fn find_links_entries(
        &self,
        package_name: &PackageName,
        download_concurrency: &Semaphore,
    ) -> Result<Vec<FlatIndexEntry>, Error> {
        Ok(futures::stream::iter(self.indexes.flat_indexes())
            .map(async |index| {
                let _permit = download_concurrency.acquire().await;
                self.flat_single_index(package_name, index.url()).await
            })
            .buffered(8)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>())
    }

    /// Fetch the [`FlatIndexEntry`] entries for a given package from a single `--find-links` index.
    async fn flat_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
    ) -> Result<Vec<FlatIndexEntry>, Error> {
        // Each flat index gets its own slot, so lookups for the same index share a fetch while
        // unrelated indexes can proceed concurrently.
        let flat_index_slot = {
            let mut cache = self.flat_indexes.lock().await;
            cache.get_or_insert(index.clone())
        };
        let mut flat_index = flat_index_slot.lock().await;

        if let Some(entries) = flat_index.as_ref() {
            return Ok(entries.get(package_name).cloned().unwrap_or_default());
        }

        let client = FlatIndexClient::new(self.cached_client(), self.connectivity, &self.cache);

        // Fetch the entries for the index.
        let (entries, _) = client
            .fetch_index(index)
            .await
            .map_err(ErrorKind::Flat)?
            .into_parts();

        // Index by package name.
        let mut entries_by_package: FxHashMap<PackageName, Vec<FlatIndexEntry>> =
            FxHashMap::default();
        for entry in entries {
            entries_by_package
                .entry(entry.filename().name().clone())
                .or_default()
                .push(entry);
        }
        let package_entries = entries_by_package
            .get(package_name)
            .cloned()
            .unwrap_or_default();

        // Write to the cache.
        *flat_index = Some(entries_by_package);

        Ok(package_entries)
    }

    /// Fetch the [`SimpleDetailMetadata`] from a single index for a given package.
    ///
    /// The index can either be a PEP 503-compatible remote repository, or a local directory laid
    /// out in the same format.
    async fn simple_detail_single_index(
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
            Connectivity::Online
                if let Some(header) = self.indexes.simple_api_cache_control_for(index) =>
            {
                CacheControl::Override(header)
            }
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
            lock_entry.lock().await.map_err(ErrorKind::CacheLock)?
        };

        let result = if matches!(index, IndexUrl::Path(_)) {
            self.fetch_local_simple_detail(package_name, &url).await
        } else {
            self.fetch_remote_simple_detail(package_name, &url, index, &cache_entry, cache_control)
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
                ErrorKind::LocalPackageNotFound(_) => Ok(SimpleMetadataSearchOutcome::NotFound),

                _ => Err(err),
            },
        }
    }

    /// Fetch the [`SimpleDetailMetadata`] from a remote URL, using the PEP 503 Simple Repository API.
    async fn fetch_remote_simple_detail(
        &self,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
        index: &IndexUrl,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
    ) -> Result<OwnedArchive<SimpleDetailMetadata>, Error> {
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
            .map_err(|err| {
                ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
            })?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = DisplaySafeUrl::from_url(response.url().clone());

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
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        let data: PyxSimpleDetail = rmp_serde::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_msgpack_err(err, url.clone()))?;

                        SimpleDetailMetadata::from_pyx_files(
                            data.files,
                            data.core_metadata,
                            package_name,
                            data.project_status,
                            &url,
                        )
                    }
                    MediaType::PyxV1Json => {
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        let data: PyxSimpleDetail = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleDetailMetadata::from_pyx_files(
                            data.files,
                            data.core_metadata,
                            package_name,
                            data.project_status,
                            &url,
                        )
                    }
                    MediaType::PypiV1Json => {
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;

                        let data: PypiSimpleDetail = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleDetailMetadata::from_pypi_files(
                            data.files,
                            package_name,
                            data.project_status,
                            &url,
                        )
                    }
                    MediaType::PypiV1Html | MediaType::TextHtml => {
                        let text = response.text().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        SimpleDetailMetadata::from_html(&text, package_name, &url)?
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

    /// Fetch the [`SimpleDetailMetadata`] from a local file, using a PEP 503-compatible directory
    /// structure.
    async fn fetch_local_simple_detail(
        &self,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
    ) -> Result<OwnedArchive<SimpleDetailMetadata>, Error> {
        let path = url
            .to_file_path()
            .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?
            .join("index.html");
        let text = match fs_err::tokio::read_to_string(&path).await {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::from(ErrorKind::LocalPackageNotFound(
                    package_name.clone(),
                )));
            }
            Err(err) => {
                return Err(Error::from(ErrorKind::Io(err)));
            }
        };
        let metadata = SimpleDetailMetadata::from_html(&text, package_name, url)?;
        OwnedArchive::from_unarchived(&metadata)
    }

    /// Fetch the list of projects from a Simple API index at a remote URL.
    ///
    /// This fetches the root of a Simple API index (e.g., `https://pypi.org/simple/`)
    /// which returns a list of all available projects.
    pub async fn fetch_simple_index(
        &self,
        index_url: &IndexUrl,
    ) -> Result<SimpleIndexMetadata, Error> {
        // Format the URL for PyPI.
        let mut url = index_url.url().clone();
        url.path_segments_mut()
            .map_err(|()| ErrorKind::CannotBeABase(index_url.url().clone()))?
            .pop_if_empty()
            // The URL *must* end in a trailing slash for proper relative path behavior
            // ref https://github.com/servo/rust-url/issues/333
            .push("");

        if url.scheme() == "file" {
            let archived = self.fetch_local_simple_index(&url).await?;
            Ok(OwnedArchive::deserialize(&archived))
        } else {
            let archived = self.fetch_remote_simple_index(&url, index_url).await?;
            Ok(OwnedArchive::deserialize(&archived))
        }
    }

    /// Fetch the list of projects from a remote Simple API index.
    async fn fetch_remote_simple_index(
        &self,
        url: &DisplaySafeUrl,
        index: &IndexUrl,
    ) -> Result<OwnedArchive<SimpleIndexMetadata>, Error> {
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

        let cache_entry = self.cache.entry(
            CacheBucket::Simple,
            WheelCache::Index(index).root(),
            "index.html.rkyv",
        );
        let cache_control = match self.connectivity {
            Connectivity::Online
                if let Some(header) = self.indexes.simple_api_cache_control_for(index) =>
            {
                CacheControl::Override(header)
            }
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, None, None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = DisplaySafeUrl::from_url(response.url().clone());

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

                let metadata = match media_type {
                    MediaType::PyxV1Msgpack => {
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        let data: PyxSimpleIndex = rmp_serde::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_msgpack_err(err, url.clone()))?;
                        SimpleIndexMetadata::from_pyx_index(data)
                    }
                    MediaType::PyxV1Json => {
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        let data: PyxSimpleIndex = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;
                        SimpleIndexMetadata::from_pyx_index(data)
                    }
                    MediaType::PypiV1Json => {
                        let bytes = response.bytes().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        let data: PypiSimpleIndex = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;
                        SimpleIndexMetadata::from_pypi_index(data)
                    }
                    MediaType::PypiV1Html | MediaType::TextHtml => {
                        let text = response.text().await.map_err(|err| {
                            ErrorKind::from_reqwest(
                                url.clone(),
                                err,
                                self.client.certificate_source(),
                            )
                        })?;
                        SimpleIndexMetadata::from_html(&text, &url)?
                    }
                };

                OwnedArchive::from_unarchived(&metadata)
            }
        };

        let simple_request = self
            .uncached_client(url)
            .get(Url::from(url.clone()))
            .header("Accept-Encoding", "gzip, deflate, zstd")
            .header("Accept", accept)
            .build()
            .map_err(|err| {
                ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
            })?;

        let index = self
            .cached_client()
            .get_cacheable_with_retry(
                simple_request,
                &cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await?;

        Ok(index)
    }

    /// Fetch the list of projects from a local Simple API index.
    async fn fetch_local_simple_index(
        &self,
        url: &DisplaySafeUrl,
    ) -> Result<OwnedArchive<SimpleIndexMetadata>, Error> {
        let path = url
            .to_file_path()
            .map_err(|()| ErrorKind::NonFileUrl(url.clone()))?
            .join("index.html");
        let text = match fs_err::tokio::read_to_string(&path).await {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::from(ErrorKind::LocalIndexNotFound(path)));
            }
            Err(err) => {
                return Err(Error::from(ErrorKind::Io(err)));
            }
        };
        let metadata = SimpleIndexMetadata::from_html(&text, url)?;
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
        git: &GitResolver,
        capabilities: &IndexCapabilities,
        reporter: Option<Arc<dyn Reporter>>,
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
                        self.wheel_metadata_registry(wheel, &url, capabilities)
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
            BuiltDist::GitPath(wheel) => {
                // Fetch the Git repository.
                let fetch = git
                    .fetch(
                        &wheel.git,
                        self.git_http_settings(wheel.git.url()),
                        self.cache.bucket(CacheBucket::Git),
                        reporter,
                    )
                    .await
                    .map_err(ErrorKind::Git)?;

                if wheel.git.lfs().enabled() && !fetch.lfs_ready() {
                    if GIT_LFS.is_err() {
                        return Err(ErrorKind::MissingWheelGitLfsArtifacts(
                            wheel.url.to_url(),
                            GitError::GitLfsNotFound,
                        )
                        .into());
                    }
                    return Err(ErrorKind::MissingWheelGitLfsArtifacts(
                        wheel.url.to_url(),
                        GitError::GitLfsNotConfigured,
                    )
                    .into());
                }

                // Read the metadata.
                let file = fs_err::tokio::File::open(fetch.path().join(&wheel.install_path))
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
        wheel: &RegistryBuiltWheel,
        url: &DisplaySafeUrl,
        capabilities: &IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        let RegistryBuiltWheel {
            filename,
            file,
            index,
        } = wheel;

        // If the metadata file is available at its own url (PEP 658), download it from there.
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
                Connectivity::Online
                    if let Some(header) = self.indexes.artifact_cache_control_for(index) =>
                {
                    CacheControl::Override(header)
                }
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
                lock_entry.lock().await.map_err(ErrorKind::CacheLock)?
            };

            let response_callback = async |response: Response| {
                let bytes = response.bytes().await.map_err(|err| {
                    ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
                })?;

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
                .map_err(|err| {
                    ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
                })?;
            Ok(self
                .cached_client()
                .get_serde_with_retry(req, &cache_entry, cache_control, response_callback)
                .await?)
        } else {
            // If we lack PEP 658 support, try using HTTP range requests to read only the
            // `.dist-info/METADATA` file from the zip, and if that also fails, download the whole wheel
            // into the cache and read from there
            self.wheel_metadata_no_pep658(
                filename,
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
            Connectivity::Online
                if let Some(index) = index
                    && let Some(header) = self.indexes.artifact_cache_control_for(index) =>
            {
                CacheControl::Override(header)
            }
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
            lock_entry.lock().await.map_err(ErrorKind::CacheLock)?
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
                .map_err(|err| {
                    ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
                })?;

            // Copy authorization headers from the HEAD request to subsequent requests
            let mut headers = HeaderMap::default();
            if let Some(authorization) = req.headers().get("authorization") {
                headers.append("authorization", authorization.clone());
            }
            // These range requests need the bytes from the wheel archive itself.
            // After `reqwest` moved decompression to tower-http[1], this path could receive
            // transparently decompressed responses. That breaks the byte offsets used by
            // `AsyncHttpRangeReader` and results in us incorrectly trying to double-decompress gzip streams[2].
            // We request with `Accept: identity` so that the range reader always sees the compressed wheel bytes.
            //
            // [1]: https://github.com/seanmonstar/reqwest/pull/2840
            // [2]: https://github.com/astral-sh/async_http_range_reader/pull/3#discussion_r2700194798
            headers.insert(
                reqwest::header::ACCEPT_ENCODING,
                reqwest::header::HeaderValue::from_static("identity"),
            );
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
                    cache_control.clone(),
                    read_metadata_range_request,
                )
                .await
                .map_err(crate::Error::from);

            match result {
                Ok(metadata) => return Ok(metadata),
                Err(err) => {
                    if err.is_http_range_requests_unsupported(url, index) {
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
            .map_err(|err| {
                ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
            })?;

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
            // Assumption: The connect timeout with the 10s default is not the culprit.
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",
                    self.read_timeout().as_secs()
                ),
            )
        } else {
            std::io::Error::other(err)
        }
    }
}

#[derive(Debug)]
enum SimpleMetadataSearchOutcome {
    /// Simple metadata was found
    Found(OwnedArchive<SimpleDetailMetadata>),
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
#[derive(Default, Debug)]
struct FlatIndexCache(FxHashMap<IndexUrl, FlatIndexSlot>);

impl FlatIndexCache {
    /// Return the per-index slot for this flat index, creating it on first access.
    fn get_or_insert(&mut self, index: IndexUrl) -> FlatIndexSlot {
        self.0
            .entry(index)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }
}

type FlatIndexEntriesByPackage = FxHashMap<PackageName, Vec<FlatIndexEntry>>;
type FlatIndexSlot = Arc<Mutex<Option<FlatIndexEntriesByPackage>>>;

#[derive(Default, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct VersionFiles {
    pub wheels: Vec<CachedFile>,
    pub source_dists: Vec<CachedFile>,
}

impl VersionFiles {
    fn push(&mut self, filename: &DistFilename, file: File, url_prefix: &str) {
        let file = CachedFile::from_file(file, url_prefix);
        match filename {
            DistFilename::WheelFilename(_) => self.wheels.push(file),
            DistFilename::SourceDistFilename(_) => self.source_dists.push(file),
        }
    }
}

impl ArchivedVersionFiles {
    /// Materialize the archived files without allocating intermediate cached-file values.
    pub fn all<'a>(
        &'a self,
        package_name: &'a PackageName,
        url_prefix: &'a str,
    ) -> impl Iterator<Item = (DistFilename, File)> + 'a {
        let mut pool = rkyv::de::Pool::new();
        self.source_dists
            .iter()
            .chain(self.wheels.iter())
            .filter_map(move |file| {
                let file = file.to_file(url_prefix, &mut pool);
                let filename = DistFilename::try_from_filename(&file.filename, package_name)?;
                Some((filename, file))
            })
    }
}

/// A compact, cache-local representation of a registry file from the Simple API.
///
/// Filenames recoverable from the URL and false `yanked` markers are omitted, while optional
/// scalar values use presence bits. Converting back to [`File`] restores equivalent Simple API
/// metadata.
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct CachedFile {
    size: u64,
    upload_time_utc_ms: i64,
    hashes: CachedHashDigests,
    url: CachedFileLocation,
    requires_python: Option<Arc<VersionSpecifiers>>,
    #[rkyv(with = rkyv::with::Niche)]
    filename: Option<Box<SmallString>>,
    #[rkyv(with = rkyv::with::Niche)]
    yanked: Option<Box<Yanked>>,
    #[rkyv(with = rkyv::with::Niche)]
    zstd: Option<Box<Zstd>>,
    dist_info_metadata: bool,
    has_size: bool,
    has_upload_time: bool,
}

impl ArchivedCachedFile {
    /// Returns the upload time in UTC milliseconds, if it was present in the index metadata.
    pub fn upload_time_utc_ms(&self) -> Option<i64> {
        self.has_upload_time
            .then_some(self.upload_time_utc_ms.to_native())
    }

    fn to_file(&self, url_prefix: &str, pool: &mut rkyv::de::Pool) -> File {
        let filename = self
            .filename
            .as_ref()
            .map_or_else(|| self.url.raw_filename(), |filename| filename.as_str());
        let hashes = rkyv::api::deserialize_using::<CachedHashDigests, _, rkyv::rancor::Error>(
            &self.hashes,
            pool,
        )
        .expect("archived hashes always deserialize");
        let requires_python = rkyv::api::deserialize_using::<
            Option<Arc<VersionSpecifiers>>,
            _,
            rkyv::rancor::Error,
        >(&self.requires_python, pool)
        .expect("archived requires-python always deserializes");
        let yanked = self.yanked.as_deref().map(|yanked| {
            Box::new(
                rkyv::deserialize::<Yanked, rkyv::rancor::Error>(yanked)
                    .expect("archived yanked marker always deserializes"),
            )
        });
        let zstd = self.zstd.as_deref().map(|zstd| {
            Box::new(
                rkyv::deserialize::<Zstd, rkyv::rancor::Error>(zstd)
                    .expect("archived zstd metadata always deserializes"),
            )
        });
        File {
            dist_info_metadata: self.dist_info_metadata,
            filename: SmallString::from(filename),
            hashes: HashDigests::from(hashes),
            requires_python,
            size: self.has_size.then_some(self.size.to_native()),
            upload_time_utc_ms: self
                .has_upload_time
                .then_some(self.upload_time_utc_ms.to_native()),
            url: self.url.to_location(url_prefix),
            yanked,
            zstd,
        }
    }
}

impl CachedFile {
    /// Returns the stored filename or reconstructs it from the file URL.
    pub fn filename(&self) -> &str {
        self.filename
            .as_deref()
            .map_or_else(|| self.url.raw_filename(), SmallString::as_ref)
    }

    /// Reconstructs the file's hash digests from their compact cache representation.
    pub fn hashes(&self) -> HashDigests {
        HashDigests::from(&self.hashes)
    }

    fn from_file(file: File, url_prefix: &str) -> Self {
        let filename =
            (file.url.raw_filename() != file.filename.as_ref()).then(|| Box::new(file.filename));
        let has_size = file.size.is_some();
        let has_upload_time = file.upload_time_utc_ms.is_some();
        Self {
            dist_info_metadata: file.dist_info_metadata,
            filename,
            hashes: CachedHashDigests::from(file.hashes),
            requires_python: file.requires_python,
            size: file.size.unwrap_or_default(),
            upload_time_utc_ms: file.upload_time_utc_ms.unwrap_or_default(),
            has_size,
            has_upload_time,
            url: CachedFileLocation::from_location(file.url, url_prefix),
            yanked: file.yanked.filter(|yanked| yanked.is_yanked()),
            zstd: file.zstd,
        }
    }
}

/// A cache-local file location that stores a shared absolute URL prefix only once per project.
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
enum CachedFileLocation {
    RelativeUrl(SmallString, SmallString),
    AbsoluteUrl(SmallString),
    Warehouse(Box<CachedWarehouseLocation>),
}

/// A packed Warehouse artifact path of the form `<2 hex>/<2 hex>/<60 hex>/<tail>`.
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
struct CachedWarehouseLocation {
    digest: [u8; 32],
    tail: SmallString,
}

impl CachedFileLocation {
    fn from_location(location: FileLocation, url_prefix: &str) -> Self {
        match location {
            FileLocation::RelativeUrl(base, path) => Self::RelativeUrl(base, path),
            FileLocation::AbsoluteUrl(url) => {
                let suffix = url
                    .as_ref()
                    .strip_prefix(url_prefix)
                    .unwrap_or(url.as_ref());
                let bytes = suffix.as_bytes();
                if bytes.len() > 67
                    && bytes[2] == b'/'
                    && bytes[5] == b'/'
                    && bytes[66] == b'/'
                    && bytes[..2]
                        .iter()
                        .chain(&bytes[3..5])
                        .chain(&bytes[6..66])
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                {
                    let mut encoded = [0; 64];
                    encoded[..2].copy_from_slice(&bytes[..2]);
                    encoded[2..4].copy_from_slice(&bytes[3..5]);
                    encoded[4..].copy_from_slice(&bytes[6..66]);
                    let mut digest = [0; 32];
                    if hex::decode_to_slice(encoded, &mut digest).is_ok() {
                        return Self::Warehouse(Box::new(CachedWarehouseLocation {
                            digest,
                            tail: SmallString::from(&suffix[67..]),
                        }));
                    }
                }
                Self::AbsoluteUrl(SmallString::from(suffix))
            }
        }
    }

    fn raw_filename(&self) -> &str {
        let path = match self {
            Self::RelativeUrl(_, path) | Self::AbsoluteUrl(path) => path.as_ref(),
            Self::Warehouse(location) => location.tail.as_ref(),
        };
        let path = path.split_once(['?', '#']).map_or(path, |(path, _)| path);
        path.rsplit_once('/').map_or(path, |(_, filename)| filename)
    }
}

impl ArchivedCachedFileLocation {
    fn raw_filename(&self) -> &str {
        let path = match self {
            Self::RelativeUrl(_, path) | Self::AbsoluteUrl(path) => path.as_str(),
            Self::Warehouse(location) => location.tail.as_str(),
        };
        let path = path.split_once(['?', '#']).map_or(path, |(path, _)| path);
        path.rsplit_once('/').map_or(path, |(_, filename)| filename)
    }

    fn to_location(&self, url_prefix: &str) -> FileLocation {
        match self {
            Self::RelativeUrl(base, path) => FileLocation::RelativeUrl(
                SmallString::from(base.as_str()),
                SmallString::from(path.as_str()),
            ),
            Self::AbsoluteUrl(suffix) => {
                FileLocation::AbsoluteUrl(UrlString::new(SmallString::concat(&[
                    url_prefix,
                    suffix.as_str(),
                ])))
            }
            Self::Warehouse(location) => {
                let mut encoded = [0; 64];
                let digest = if hex::encode_to_slice(location.digest, &mut encoded).is_ok() {
                    String::from_utf8_lossy(&encoded)
                } else {
                    Cow::Owned(hex::encode(location.digest))
                };
                FileLocation::AbsoluteUrl(UrlString::new(SmallString::concat(&[
                    url_prefix,
                    &digest[..2],
                    "/",
                    &digest[2..4],
                    "/",
                    &digest[4..],
                    "/",
                    location.tail.as_str(),
                ])))
            }
        }
    }
}

/// A compact representation of a single, canonical hash digest.
///
/// Only lowercase hexadecimal digests of the expected length use the packed variants. Multiple
/// hashes and non-canonical spellings remain in [`Self::Other`] so conversion back to
/// [`HashDigests`] is lossless. The larger digests are boxed to keep the common archived layout
/// small.
#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
enum CachedHashDigests {
    Sha256([u8; 32]),
    Md5([u8; 16]),
    Blake2b([u8; 32]),
    Sha384(Box<[u8; 48]>),
    Sha512(Box<[u8; 64]>),
    Other(HashDigests),
}

impl Debug for CachedHashDigests {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (name, digest) = match self {
            Self::Md5(digest) => ("Md5", digest.as_slice()),
            Self::Sha256(digest) => ("Sha256", digest.as_slice()),
            Self::Blake2b(digest) => ("Blake2b", digest.as_slice()),
            Self::Sha384(digest) => ("Sha384", digest.as_slice()),
            Self::Sha512(digest) => ("Sha512", digest.as_slice()),
            Self::Other(hashes) => return f.debug_tuple("Other").field(hashes).finish(),
        };
        f.debug_tuple(name).field(&hex::encode(digest)).finish()
    }
}

impl From<HashDigests> for CachedHashDigests {
    fn from(hashes: HashDigests) -> Self {
        let [hash] = hashes.as_slice() else {
            return Self::Other(hashes);
        };
        let cached = match hash.algorithm {
            HashAlgorithm::Md5 => decode_digest(hash).map(Self::Md5),
            HashAlgorithm::Sha256 => decode_digest(hash).map(Self::Sha256),
            HashAlgorithm::Blake2b => decode_digest(hash).map(Self::Blake2b),
            HashAlgorithm::Sha384 => {
                decode_digest(hash).map(|digest| Self::Sha384(Box::new(digest)))
            }
            HashAlgorithm::Sha512 => {
                decode_digest(hash).map(|digest| Self::Sha512(Box::new(digest)))
            }
        };
        let Some(cached) = cached else {
            return Self::Other(hashes);
        };
        cached
    }
}

impl From<CachedHashDigests> for HashDigests {
    fn from(hashes: CachedHashDigests) -> Self {
        match hashes {
            CachedHashDigests::Other(hashes) => hashes,
            hashes => Self::from(&hashes),
        }
    }
}

impl From<&CachedHashDigests> for HashDigests {
    fn from(hashes: &CachedHashDigests) -> Self {
        match hashes {
            CachedHashDigests::Md5(digest) => Self::from(hash_digest(HashAlgorithm::Md5, digest)),
            CachedHashDigests::Sha256(digest) => {
                Self::from(hash_digest(HashAlgorithm::Sha256, digest))
            }
            CachedHashDigests::Blake2b(digest) => {
                Self::from(hash_digest(HashAlgorithm::Blake2b, digest))
            }
            CachedHashDigests::Sha384(digest) => {
                Self::from(hash_digest(HashAlgorithm::Sha384, digest.as_slice()))
            }
            CachedHashDigests::Sha512(digest) => {
                Self::from(hash_digest(HashAlgorithm::Sha512, digest.as_slice()))
            }
            CachedHashDigests::Other(hashes) => hashes.clone(),
        }
    }
}

/// Decodes a lowercase hexadecimal digest of exactly `N` bytes.
///
/// Rejecting non-canonical spellings lets [`CachedHashDigests::Other`] preserve their original
/// text.
fn decode_digest<const N: usize>(hash: &HashDigest) -> Option<[u8; N]> {
    if hash.digest.len() != N * 2
        || !hash
            .digest
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return None;
    }
    let mut digest = [0; N];
    hex::decode_to_slice(hash.digest.as_bytes(), &mut digest).ok()?;
    Some(digest)
}

/// Reconstructs the canonical lowercase spelling of a packed digest.
fn hash_digest(algorithm: HashAlgorithm, digest: &[u8]) -> HashDigest {
    let mut encoded = [0; 128];
    let length = digest.len() * 2;
    let digest = if let Some(encoded) = encoded.get_mut(..length)
        && hex::encode_to_slice(digest, &mut *encoded).is_ok()
    {
        SmallString::from(String::from_utf8_lossy(encoded))
    } else {
        SmallString::from(hex::encode(digest))
    };
    HashDigest { algorithm, digest }
}

/// The list of projects available in a Simple API index.
#[derive(Default, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct SimpleIndexMetadata {
    /// The list of project names available in the index.
    projects: Vec<PackageName>,
}

impl SimpleIndexMetadata {
    /// Iterate over the projects in the index.
    pub fn iter(&self) -> impl Iterator<Item = &PackageName> {
        self.projects.iter()
    }

    /// Create a [`SimpleIndexMetadata`] from a [`PypiSimpleIndex`].
    fn from_pypi_index(index: PypiSimpleIndex) -> Self {
        Self {
            projects: index.into_project_names(),
        }
    }

    /// Create a [`SimpleIndexMetadata`] from a [`PyxSimpleIndex`].
    fn from_pyx_index(index: PyxSimpleIndex) -> Self {
        Self {
            projects: index.into_project_names(),
        }
    }

    /// Create a [`SimpleIndexMetadata`] from HTML content.
    fn from_html(text: &str, url: &DisplaySafeUrl) -> Result<Self, Error> {
        let html = crate::html::SimpleIndexHtml::parse(text).map_err(|err| {
            Error::from(ErrorKind::BadHtml {
                source: err,
                url: url.clone(),
            })
        })?;
        Ok(Self {
            projects: html.projects,
        })
    }
}

/// Detail response for a Python package from a Simple API index.
///
/// Abstracts over both HTML and JSON index formats.
#[derive(Default, Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct SimpleDetailMetadata {
    project_status: ProjectStatus,
    versions: Vec<SimpleDetailMetadatum>,
    url_prefix: String,
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct SimpleDetailMetadatum {
    pub version: Version,
    pub files: VersionFiles,
    #[rkyv(with = rkyv::with::Niche)]
    pub metadata: Option<Box<ResolutionMetadata>>,
}

impl SimpleDetailMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &SimpleDetailMetadatum> {
        self.versions.iter()
    }

    fn from_pypi_files(
        files: Vec<uv_pypi_types::PypiFile>,
        package_name: &PackageName,
        project_status: ProjectStatus,
        base: &Url,
    ) -> Self {
        let url_prefix = common_url_prefix(files.iter().map(|file| file.url.as_ref()));
        let mut version_map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Convert to a reference-counted string.
        let base = SmallString::from(base.as_str());

        // Group the distributions by version and kind
        for file in files {
            let filename =
                match DistFilename::try_from_filename_with_reason(&file.filename, package_name) {
                    Ok(filename) => filename,
                    Err(err) => {
                        debug!(
                            "Skipping file for {package_name}: {:?} ({err})",
                            file.filename
                        );
                        continue;
                    }
                };
            let file = match File::try_from_pypi(file, &base) {
                Ok(file) => file,
                Err(err) => {
                    // Ignore files with unparsable version specifiers.
                    debug!("Skipping file for {package_name}: {err}");
                    continue;
                }
            };
            match version_map.entry(filename.version().clone()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().push(&filename, file, &url_prefix);
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let mut files = VersionFiles::default();
                    files.push(&filename, file, &url_prefix);
                    entry.insert(files);
                }
            }
        }

        // Keep file ordering deterministic without sorting the complete Simple API response.
        for files in version_map.values_mut() {
            files
                .wheels
                .sort_unstable_by(|left, right| left.filename().cmp(right.filename()));
            files
                .source_dists
                .sort_unstable_by(|left, right| left.filename().cmp(right.filename()));
        }

        Self {
            versions: version_map
                .into_iter()
                .map(|(version, files)| SimpleDetailMetadatum {
                    version,
                    files,
                    metadata: None,
                })
                .collect(),
            project_status,
            url_prefix,
        }
    }

    fn from_pyx_files(
        files: Vec<uv_pypi_types::PyxFile>,
        mut core_metadata: FxHashMap<Version, uv_pypi_types::CoreMetadatum>,
        package_name: &PackageName,
        project_status: ProjectStatus,
        base: &Url,
    ) -> Self {
        let url_prefix = common_url_prefix(files.iter().map(|file| file.url.as_ref()));
        let mut version_map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Convert to a reference-counted string.
        let base = SmallString::from(base.as_str());

        // Group the distributions by version and kind
        for file in files {
            let file = match File::try_from_pyx(file, &base) {
                Ok(file) => file,
                Err(err) => {
                    // Ignore files with unparsable version specifiers.
                    debug!("Skipping file for {package_name}: {err}");
                    continue;
                }
            };
            let filename =
                match DistFilename::try_from_filename_with_reason(&file.filename, package_name) {
                    Ok(filename) => filename,
                    Err(err) => {
                        debug!(
                            "Skipping file for {package_name}: {:?} ({err})",
                            file.filename
                        );
                        continue;
                    }
                };
            match version_map.entry(filename.version().clone()) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().push(&filename, file, &url_prefix);
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let mut files = VersionFiles::default();
                    files.push(&filename, file, &url_prefix);
                    entry.insert(files);
                }
            }
        }

        Self {
            versions: version_map
                .into_iter()
                .map(|(version, files)| {
                    let metadata = core_metadata.remove(&version).map(|metadata| {
                        Box::new(ResolutionMetadata {
                            name: package_name.clone(),
                            version: version.clone(),
                            requires_dist: metadata.requires_dist,
                            requires_python: metadata.requires_python,
                            provides_extra: metadata.provides_extra,
                            dynamic: false,
                        })
                    });
                    SimpleDetailMetadatum {
                        version,
                        files,
                        metadata,
                    }
                })
                .collect(),
            project_status,
            url_prefix,
        }
    }

    /// Read the [`SimpleDetailMetadata`] from an HTML index.
    fn from_html(
        text: &str,
        package_name: &PackageName,
        url: &DisplaySafeUrl,
    ) -> Result<Self, Error> {
        let SimpleDetailHTML {
            project_status,
            base,
            files,
        } = SimpleDetailHTML::parse(text, url)
            .map_err(|err| Error::from_html_err(err, url.clone()))?;

        Ok(Self::from_pypi_files(
            files,
            package_name,
            project_status,
            base.as_url(),
        ))
    }
}

/// Return the longest shared, slash-terminated prefix of the absolute artifact URLs.
fn common_url_prefix<'a>(urls: impl Iterator<Item = &'a str>) -> String {
    let mut urls = urls.filter(|url| split_scheme(url).is_some());
    let Some(first) = urls.next() else {
        return String::new();
    };
    let mut length = first.len();
    for url in urls {
        length = first
            .as_bytes()
            .iter()
            .zip(url.as_bytes())
            .take(length)
            .take_while(|(left, right)| left == right)
            .count();
    }
    let Some(slash) = first.as_bytes()[..length]
        .iter()
        .rposition(|byte| *byte == b'/')
    else {
        return String::new();
    };
    first[..=slash].to_string()
}

impl IntoIterator for SimpleDetailMetadata {
    type Item = SimpleDetailMetadatum;
    type IntoIter = std::vec::IntoIter<SimpleDetailMetadatum>;

    fn into_iter(self) -> Self::IntoIter {
        self.versions.into_iter()
    }
}

impl ArchivedSimpleDetailMetadata {
    /// Return the common prefix of the absolute artifact URLs for this project.
    pub fn url_prefix(&self) -> &str {
        self.url_prefix.as_str()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &rkyv::Archived<SimpleDetailMetadatum>> {
        self.versions.iter()
    }

    pub fn datum(&self, i: usize) -> Option<&rkyv::Archived<SimpleDetailMetadatum>> {
        self.versions.get(i)
    }

    /// Return the project-level [PEP 792] status marker for this package.
    ///
    /// [PEP 792]: https://peps.python.org/pep-0792/
    pub fn project_status(&self) -> &rkyv::Archived<ProjectStatus> {
        &self.project_status
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
    use std::sync::Arc;

    use tokio::sync::Semaphore;
    use url::Url;
    use uv_normalize::PackageName;
    use uv_pypi_types::{PypiSimpleDetail, Yanked};
    use uv_redacted::DisplaySafeUrl;
    use uv_torch::{TorchBackend, TorchSource, TorchStrategy};

    use crate::{
        BaseClientBuilder, Connectivity, RegistryClient, RegistryClientBuilder,
        SimpleDetailMetadata, SimpleDetailMetadatum, html::SimpleDetailHTML,
    };
    use uv_cache::Cache;
    use uv_distribution_types::{
        FileLocation, Index, IndexCapabilities, IndexFormat, IndexLocations, IndexMetadataRef,
        IndexUrl, ToUrlError,
    };
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

    fn no_index_client(flat_indexes: Vec<Index>) -> Result<RegistryClient, Error> {
        Ok(
            RegistryClientBuilder::new(BaseClientBuilder::default(), Cache::temp()?)
                .index_locations(IndexLocations::new(vec![], flat_indexes, true))
                .build()?,
        )
    }

    async fn assert_no_index(
        client: &RegistryClient,
        package: &str,
        index: Option<IndexMetadataRef<'_>>,
    ) -> Result<(), Error> {
        let error = client
            .simple_detail(
                &PackageName::from_str(package)?,
                index,
                &IndexCapabilities::default(),
                &Semaphore::new(1),
            )
            .await
            .expect_err("index lookup should be disabled");

        assert!(matches!(
            error.kind(),
            crate::ErrorKind::NoIndex(error_package) if error_package == package
        ));
        Ok(())
    }

    async fn assert_no_requests(server: &MockServer) {
        assert!(
            server
                .received_requests()
                .await
                .expect("request recording should be enabled")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn no_index_disables_explicit_simple_index() -> Result<(), Error> {
        let server = MockServer::start().await;
        let explicit_index = IndexUrl::from_str(&format!("{}/simple", server.uri()))?;
        let flat_index = Index::from_find_links(IndexUrl::from_str("https://example.com/flat")?);
        let registry_client = no_index_client(vec![flat_index])?;

        assert_no_index(
            &registry_client,
            "validation",
            Some(IndexMetadataRef {
                url: &explicit_index,
                format: IndexFormat::Simple,
            }),
        )
        .await?;
        assert_no_requests(&server).await;
        Ok(())
    }

    #[tokio::test]
    async fn no_index_disables_explicit_flat_index() -> Result<(), Error> {
        let server = MockServer::start().await;
        let explicit_index = IndexUrl::from_str(&server.uri())?;
        let registry_client = no_index_client(vec![])?;

        assert_no_index(
            &registry_client,
            "validation",
            Some(IndexMetadataRef {
                url: &explicit_index,
                format: IndexFormat::Flat,
            }),
        )
        .await?;
        assert_no_requests(&server).await;
        Ok(())
    }

    #[tokio::test]
    async fn no_index_disables_torch_simple_index() -> Result<(), Error> {
        let flat_index_dir = tempfile::tempdir()?;
        let flat_index = Index::from_find_links(IndexUrl::parse(
            flat_index_dir.path().to_string_lossy().as_ref(),
            None,
        )?);
        let registry_client = RegistryClientBuilder::new(
            BaseClientBuilder::default().connectivity(Connectivity::Offline),
            Cache::temp()?,
        )
        .index_locations(IndexLocations::new(vec![], vec![flat_index], true))
        .torch_backend(Some(TorchStrategy::Backend {
            backend: TorchBackend::Cpu,
            source: TorchSource::PyTorch,
        }))
        .build()?;

        assert_no_index(&registry_client, "torch", None).await?;
        Ok(())
    }

    #[tokio::test]
    async fn simple_detail_does_not_fetch_legacy_find_links() -> Result<(), Error> {
        let server = MockServer::start().await;
        let flat_index = Index::from_find_links(IndexUrl::from_str(&server.uri())?);
        let registry_client = no_index_client(vec![flat_index])?;

        assert_no_index(&registry_client, "validation", None).await?;
        assert_no_requests(&server).await;
        Ok(())
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
            .build()
            .expect("failed to build registry client");
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
            .build()
            .expect("failed to build registry client");
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
            .build()
            .expect("failed to build registry client");
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
        let simple_metadata = SimpleDetailMetadata::from_pypi_files(
            data.files,
            &PackageName::from_str("pyflyby").unwrap(),
            data.project_status,
            &base,
        );
        let versions: Vec<String> = simple_metadata
            .iter()
            .map(|SimpleDetailMetadatum { version, .. }| version.to_string())
            .collect();
        assert_eq!(versions, ["1.7.8".to_string()]);
    }

    #[test]
    fn distribution_files_round_trip() -> Result<(), Error> {
        let response = r#"
        {
            "files": [
                {
                    "filename": "example_1-1.0.0-py3-none-any.whl",
                    "hashes": {"sha256": "1ee37474f6da8f98653dbcc208793f50b7ace1d9066f49e2707750a5ba5d53c6"},
                    "requires-python": ">=3.8",
                    "url": "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example_1-1.0.0-py3-none-any.whl"
                },
                {
                    "filename": "example-1-1.0.0.tar.gz",
                    "hashes": {"sha256": "0c4d953f405a7be1300b440dbdbc6917011a07d8401345a97e72cd410d5fb291"},
                    "requires-python": ">=3.8",
                    "yanked": "broken source archive",
                    "url": "https://files.pythonhosted.org/packages/ab/cd/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example-1-1.0.0.tar.gz"
                }
            ]
        }
        "#;
        let package_name = PackageName::from_str("example-1")?;
        let data: PypiSimpleDetail = serde_json::from_str(response)?;
        let base = DisplaySafeUrl::parse("https://pypi.org/simple/example-1/")?;
        let simple_metadata = SimpleDetailMetadata::from_pypi_files(
            data.files,
            &package_name,
            data.project_status,
            &base,
        );
        let archived = super::OwnedArchive::from_unarchived(&simple_metadata)?;
        let url_prefix = archived.url_prefix();
        assert_eq!(url_prefix, "https://files.pythonhosted.org/packages/");
        let files: Vec<_> = archived
            .iter()
            .flat_map(|datum| datum.files.all(&package_name, url_prefix))
            .collect();
        assert_eq!(
            files
                .iter()
                .map(|(filename, file)| (filename.to_string(), file.url.to_string()))
                .collect::<Vec<_>>(),
            [
                (
                    "example_1-1.0.0.tar.gz".to_string(),
                    "https://files.pythonhosted.org/packages/ab/cd/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example-1-1.0.0.tar.gz".to_string(),
                ),
                (
                    "example_1-1.0.0-py3-none-any.whl".to_string(),
                    "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example_1-1.0.0-py3-none-any.whl".to_string(),
                ),
            ]
        );
        assert_eq!(
            files
                .iter()
                .map(|(_, file)| file.hashes.first().map(ToString::to_string))
                .collect::<Vec<_>>(),
            [
                Some(
                    "sha256:0c4d953f405a7be1300b440dbdbc6917011a07d8401345a97e72cd410d5fb291"
                        .to_string()
                ),
                Some(
                    "sha256:1ee37474f6da8f98653dbcc208793f50b7ace1d9066f49e2707750a5ba5d53c6"
                        .to_string()
                ),
            ]
        );
        assert!(matches!(
            files[0].1.yanked.as_deref(),
            Some(Yanked::Reason(reason)) if reason.as_ref() == "broken source archive"
        ));
        assert!(
            files[0]
                .1
                .requires_python
                .as_ref()
                .zip(files[1].1.requires_python.as_ref())
                .is_some_and(|(left, right)| Arc::ptr_eq(left, right))
        );

        Ok(())
    }

    #[test]
    fn cached_file_locations_round_trip() -> Result<(), Error> {
        let base = SmallString::from("https://pypi.org/simple/example/");
        let locations = [
            FileLocation::new(
                SmallString::from(
                    "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example-1.0.0-py3-none-any.whl?download=1",
                ),
                &base,
            ),
            FileLocation::new(
                SmallString::from(
                    "https://files.pythonhosted.org/packages/ab/cd/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example%20name-1.0.0.tar.gz#sha256=abc",
                ),
                &base,
            ),
            FileLocation::new(
                SmallString::from("../../packages/example-1.0.0.tar.gz"),
                &base,
            ),
        ];
        let prefix = super::common_url_prefix(locations.iter().map(|location| match location {
            FileLocation::RelativeUrl(_, path) => path.as_ref(),
            FileLocation::AbsoluteUrl(url) => url.as_ref(),
        }));
        assert_eq!(prefix, "https://files.pythonhosted.org/packages/");

        for location in locations {
            let cached = super::CachedFileLocation::from_location(location.clone(), &prefix);
            if matches!(location, FileLocation::AbsoluteUrl(_)) {
                assert!(matches!(cached, super::CachedFileLocation::Warehouse(_)));
            }
            let archived = super::OwnedArchive::from_unarchived(&cached)?;
            let round_trip = archived.to_location(&prefix);
            assert_eq!(round_trip, location);
        }

        let mixed = [
            "https://files.pythonhosted.org/packages/a/example.whl",
            "http://mirror.example/packages/b/example.whl",
        ];
        assert!(super::common_url_prefix(mixed.into_iter()).is_empty());

        let noncanonical = FileLocation::new(
            SmallString::from(
                "https://files.pythonhosted.org/packages/AB/cd/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/example.whl",
            ),
            &base,
        );
        let cached = super::CachedFileLocation::from_location(noncanonical.clone(), &prefix);
        assert!(matches!(cached, super::CachedFileLocation::AbsoluteUrl(_)));
        let archived = super::OwnedArchive::from_unarchived(&cached)?;
        assert_eq!(archived.to_location(&prefix), noncanonical);

        assert_eq!(
            SmallString::concat(&["", "unicode-", "λ", ""]).as_ref(),
            "unicode-λ"
        );

        Ok(())
    }

    /// Test for project statuses from PyPI's JSON detail response.
    #[test]
    fn project_status_pypi_json() {
        // Minimized from https://pypi.org/simple/pepy/
        let json = r#"
        {
          "alternate-locations": [],
          "files": [
            {
              "core-metadata": false,
              "data-dist-info-metadata": false,
              "filename": "pepy-2.1.1.tar.gz",
              "hashes": {
                "sha256": "cec463c444b71d1664229121897b22df753dc91fabb2113d1c89992638c90829"
              },
              "provenance": null,
              "requires-python": ">=3.7",
              "size": 15399,
              "upload-time": "2022-11-14T17:14:53.935145Z",
              "url": "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/pepy-2.1.1.tar.gz",
              "yanked": false
            }
          ],
          "meta": {
            "_last-serial": 15765070,
            "api-version": "1.4"
          },
          "name": "pepy",
          "project-status": {
            "status": "archived"
          },
          "versions": [
            "2.1.1"
          ]
        }
        "#;

        let data: PypiSimpleDetail = serde_json::from_str(json).unwrap();
        let base = DisplaySafeUrl::parse("https://pypi.org/simple/pepy/").unwrap();
        let simple_metadata = SimpleDetailMetadata::from_pypi_files(
            data.files,
            &PackageName::from_str("pepy").unwrap(),
            data.project_status,
            &base,
        );

        insta::assert_debug_snapshot!(simple_metadata, @r#"
        SimpleDetailMetadata {
            project_status: ProjectStatus {
                status: Archived,
                reason: None,
            },
            versions: [
                SimpleDetailMetadatum {
                    version: "2.1.1",
                    files: VersionFiles {
                        wheels: [],
                        source_dists: [
                            CachedFile {
                                size: 15399,
                                upload_time_utc_ms: 1668446093935,
                                hashes: Sha256(
                                    "cec463c444b71d1664229121897b22df753dc91fabb2113d1c89992638c90829",
                                ),
                                url: AbsoluteUrl(
                                    "pepy-2.1.1.tar.gz",
                                ),
                                requires_python: Some(
                                    VersionSpecifiers(
                                        [
                                            VersionSpecifier {
                                                operator: GreaterThanEqual,
                                                version: "3.7",
                                            },
                                        ],
                                    ),
                                ),
                                filename: None,
                                yanked: None,
                                zstd: None,
                                dist_info_metadata: false,
                                has_size: true,
                                has_upload_time: true,
                            },
                        ],
                    },
                    metadata: None,
                },
            ],
            url_prefix: "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/",
        }
        "#);
    }

    /// Test for project statuses from PyPI's HTML detail response.
    #[test]
    fn project_status_pypi_html() {
        // Minimized from https://pypi.org/simple/pepy/
        let html = r#"
        <!DOCTYPE html>
        <html lang="en">
          <head>
            <meta name="pypi:repository-version" content="1.4">
        <meta name="pypi:project-status" content="archived">    <title>Links for pepy</title>
          </head>
          <body>
            <h1>Links for pepy</h1>
        <a href="https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/pepy-2.1.1.tar.gz#sha256=cec463c444b71d1664229121897b22df753dc91fabb2113d1c89992638c90829" data-requires-python="&gt;=3.7" >pepy-2.1.1.tar.gz</a><br />
        </body>
        </html>
        <!--SERIAL 15765070-->
        "#;

        let base = DisplaySafeUrl::parse("https://pypi.org/simple/pepy/").unwrap();
        let simple_metadata =
            SimpleDetailMetadata::from_html(html, &PackageName::from_str("pepy").unwrap(), &base)
                .unwrap();
        insta::assert_debug_snapshot!(simple_metadata, @r#"
        SimpleDetailMetadata {
            project_status: ProjectStatus {
                status: Archived,
                reason: None,
            },
            versions: [
                SimpleDetailMetadatum {
                    version: "2.1.1",
                    files: VersionFiles {
                        wheels: [],
                        source_dists: [
                            CachedFile {
                                size: 0,
                                upload_time_utc_ms: 0,
                                hashes: Sha256(
                                    "cec463c444b71d1664229121897b22df753dc91fabb2113d1c89992638c90829",
                                ),
                                url: AbsoluteUrl(
                                    "pepy-2.1.1.tar.gz",
                                ),
                                requires_python: Some(
                                    VersionSpecifiers(
                                        [
                                            VersionSpecifier {
                                                operator: GreaterThanEqual,
                                                version: "3.7",
                                            },
                                        ],
                                    ),
                                ),
                                filename: None,
                                yanked: None,
                                zstd: None,
                                dist_info_metadata: false,
                                has_size: false,
                                has_upload_time: false,
                            },
                        ],
                    },
                    metadata: None,
                },
            ],
            url_prefix: "https://files.pythonhosted.org/packages/78/7e/123d89ce0e999e957e53f0b985f734565c93b9a698af53586fc2a1be0dbf/",
        }
        "#);
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
        let SimpleDetailHTML {
            project_status: _,
            base,
            files,
        } = SimpleDetailHTML::parse(text, &base).unwrap();
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
