use async_http_range_reader::AsyncHttpRangeReader;
use futures::{FutureExt, TryStreamExt};
use http::HeaderMap;
use itertools::Either;
use reqwest::{Client, Response, StatusCode};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tracing::{info_span, instrument, trace, warn, Instrument};
use url::Url;

use uv_cache::{Cache, CacheBucket, CacheEntry, WheelCache};
use uv_configuration::KeyringProviderType;
use uv_configuration::{IndexStrategy, TrustedHost};
use uv_distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use uv_distribution_types::{
    BuiltDist, File, FileLocation, Index, IndexCapabilities, IndexUrl, IndexUrls, Name,
};
use uv_metadata::{read_metadata_async_seek, read_metadata_async_stream};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Platform;
use uv_pypi_types::{ResolutionMetadata, SimpleJson};

use crate::base_client::BaseClientBuilder;
use crate::cached_client::CacheControl;
use crate::html::SimpleHtml;
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::rkyvutil::OwnedArchive;
use crate::{CachedClient, CachedClientError, Error, ErrorKind};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder<'a> {
    index_urls: IndexUrls,
    index_strategy: IndexStrategy,
    cache: Cache,
    base_client_builder: BaseClientBuilder<'a>,
}

impl RegistryClientBuilder<'_> {
    pub fn new(cache: Cache) -> Self {
        Self {
            index_urls: IndexUrls::default(),
            index_strategy: IndexStrategy::default(),
            cache,
            base_client_builder: BaseClientBuilder::new(),
        }
    }
}

impl<'a> RegistryClientBuilder<'a> {
    #[must_use]
    pub fn index_urls(mut self, index_urls: IndexUrls) -> Self {
        self.index_urls = index_urls;
        self
    }

    #[must_use]
    pub fn index_strategy(mut self, index_strategy: IndexStrategy) -> Self {
        self.index_strategy = index_strategy;
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
    pub fn client(mut self, client: Client) -> Self {
        self.base_client_builder = self.base_client_builder.client(client);
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
            cache: self.cache,
            connectivity,
            client,
            timeout,
        }
    }
}

impl<'a> TryFrom<BaseClientBuilder<'a>> for RegistryClientBuilder<'a> {
    type Error = std::io::Error;

    fn try_from(value: BaseClientBuilder<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            index_urls: IndexUrls::default(),
            index_strategy: IndexStrategy::default(),
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
    /// The underlying HTTP client.
    client: CachedClient,
    /// Used for the remote wheel METADATA cache.
    cache: Cache,
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: Duration,
}

impl RegistryClient {
    /// Return the [`CachedClient`] used by this client.
    pub fn cached_client(&self) -> &CachedClient {
        &self.client
    }

    /// Return the [`BaseClient`] used by this client.
    pub fn uncached_client(&self, url: &Url) -> &ClientWithMiddleware {
        self.client.uncached().for_host(url)
    }

    /// Return the [`Connectivity`] mode used by this client.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// Return the timeout this client is configured with, in seconds.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Fetch a package from the `PyPI` simple API.
    ///
    /// "simple" here refers to [PEP 503 – Simple Repository API](https://peps.python.org/pep-0503/)
    /// and [PEP 691 – JSON-based Simple API for Python Package Indexes](https://peps.python.org/pep-0691/),
    /// which the pypi json api approximately implements.
    #[instrument("simple_api", skip_all, fields(package = % package_name))]
    pub async fn simple<'index>(
        &'index self,
        package_name: &PackageName,
        index: Option<&'index IndexUrl>,
        capabilities: &IndexCapabilities,
    ) -> Result<Vec<(&'index IndexUrl, OwnedArchive<SimpleMetadata>)>, Error> {
        let indexes = if let Some(index) = index {
            Either::Left(std::iter::once(index))
        } else {
            Either::Right(self.index_urls.indexes().map(Index::url))
        };

        let mut it = indexes.peekable();
        if it.peek().is_none() {
            return Err(ErrorKind::NoIndex(package_name.to_string()).into());
        }

        let mut results = Vec::new();
        for index in it {
            match self.simple_single_index(package_name, index).await {
                Ok(metadata) => {
                    results.push((index, metadata));

                    // If we're only using the first match, we can stop here.
                    if self.index_strategy == IndexStrategy::FirstIndex {
                        break;
                    }
                }
                Err(err) => match err.into_kind() {
                    // The package could not be found in the remote index.
                    ErrorKind::WrappedReqwestError(url, err) => match err.status() {
                        Some(StatusCode::NOT_FOUND) => {}
                        Some(StatusCode::UNAUTHORIZED) => {
                            capabilities.set_unauthorized(index.clone());
                        }
                        Some(StatusCode::FORBIDDEN) => {
                            capabilities.set_forbidden(index.clone());
                        }
                        _ => return Err(ErrorKind::WrappedReqwestError(url, err).into()),
                    },

                    // The package is unavailable due to a lack of connectivity.
                    ErrorKind::Offline(_) => {}

                    // The package could not be found in the local index.
                    ErrorKind::FileNotFound(_) => {}

                    other => return Err(other.into()),
                },
            };
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

    /// Fetch the [`SimpleMetadata`] from a single index for a given package.
    ///
    /// The index can either be a PEP 503-compatible remote repository, or a local directory laid
    /// out in the same format.
    async fn simple_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
    ) -> Result<OwnedArchive<SimpleMetadata>, Error> {
        // Format the URL for PyPI.
        let mut url: Url = index.clone().into();
        url.path_segments_mut()
            .map_err(|()| ErrorKind::CannotBeABase(index.clone().into()))?
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
                    .freshness(&cache_entry, Some(package_name))
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        if matches!(index, IndexUrl::Path(_)) {
            self.fetch_local_index(package_name, &url).await
        } else {
            self.fetch_remote_index(package_name, &url, &cache_entry, cache_control)
                .await
        }
    }

    /// Fetch the [`SimpleMetadata`] from a remote URL, using the PEP 503 Simple Repository API.
    async fn fetch_remote_index(
        &self,
        package_name: &PackageName,
        url: &Url,
        cache_entry: &CacheEntry,
        cache_control: CacheControl,
    ) -> Result<OwnedArchive<SimpleMetadata>, Error> {
        let simple_request = self
            .uncached_client(url)
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .header("Accept", MediaType::accepts())
            .build()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = response.url().clone();

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
            .get_cacheable(
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
        url: &Url,
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
                    Url(Url),
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
                        let url = url.to_url();
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
                let file = fs_err::tokio::File::open(&wheel.install_path)
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
        url: &Url,
        capabilities: &IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        // If the metadata file is available at its own url (PEP 658), download it from there.
        let filename = WheelFilename::from_str(&file.filename).map_err(ErrorKind::WheelFilename)?;
        if file.dist_info_metadata {
            let mut url = url.clone();
            url.set_path(&format!("{}.metadata", url.path()));

            let cache_entry = self.cache.entry(
                CacheBucket::Wheels,
                WheelCache::Index(index).wheel_dir(filename.name.as_ref()),
                format!("{}.msgpack", filename.stem()),
            );
            let cache_control = match self.connectivity {
                Connectivity::Online => CacheControl::from(
                    self.cache
                        .freshness(&cache_entry, Some(&filename.name))
                        .map_err(ErrorKind::Io)?,
                ),
                Connectivity::Offline => CacheControl::AllowStale,
            };

            let response_callback = |response: Response| async {
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;

                info_span!("parse_metadata21")
                    .in_scope(|| ResolutionMetadata::parse_metadata(bytes.as_ref()))
                    .map_err(|err| {
                        Error::from(ErrorKind::MetadataParseError(
                            filename,
                            url.to_string(),
                            Box::new(err),
                        ))
                    })
            };
            let req = self
                .uncached_client(&url)
                .get(url.clone())
                .build()
                .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
            Ok(self
                .cached_client()
                .get_serde(req, &cache_entry, cache_control, response_callback)
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
        url: &'data Url,
        index: Option<&'data IndexUrl>,
        cache_shard: WheelCache<'data>,
        capabilities: &'data IndexCapabilities,
    ) -> Result<ResolutionMetadata, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::Wheels,
            cache_shard.wheel_dir(filename.name.as_ref()),
            format!("{}.msgpack", filename.stem()),
        );
        let cache_control = match self.connectivity {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, Some(&filename.name))
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        // Attempt to fetch via a range request.
        if index.map_or(true, |index| capabilities.supports_range_requests(index)) {
            let req = self
                .uncached_client(url)
                .head(url.clone())
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
                        url.clone(),
                        headers,
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
                .get_serde(
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
            };
        }

        // Create a request to stream the file.
        let req = self
            .uncached_client(url)
            .get(url.clone())
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
            .get_serde(req, &cache_entry, cache_control, read_metadata_stream)
            .await
            .map_err(crate::Error::from)
    }

    /// Handle a specific `reqwest` error, and convert it to [`io::Error`].
    fn handle_response_errors(&self, err: reqwest::Error) -> std::io::Error {
        if err.is_timeout() {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).", self.timeout().as_secs()
                ),
            )
        } else {
            std::io::Error::new(std::io::ErrorKind::Other, err)
        }
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
        self.wheels
            .into_iter()
            .map(|VersionWheel { name, file }| (DistFilename::WheelFilename(name), file))
            .chain(
                self.source_dists
                    .into_iter()
                    .map(|VersionSourceDist { name, file }| {
                        (DistFilename::SourceDistFilename(name), file)
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

        // Group the distributions by version and kind
        for file in files {
            let Some(filename) =
                DistFilename::try_from_filename(file.filename.as_str(), package_name)
            else {
                warn!("Skipping file for {package_name}: {}", file.filename);
                continue;
            };
            let version = match filename {
                DistFilename::SourceDistFilename(ref inner) => &inner.version,
                DistFilename::WheelFilename(ref inner) => &inner.version,
            };
            let file = match File::try_from(file, base) {
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
    fn from_html(text: &str, package_name: &PackageName, url: &Url) -> Result<Self, Error> {
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
mod tests;
