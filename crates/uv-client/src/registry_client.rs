use std::collections::BTreeMap;
use std::env;
use std::fmt::Debug;
use std::path::Path;
use std::str::FromStr;

use async_http_range_reader::AsyncHttpRangeReader;
use futures::{FutureExt, TryStreamExt};
use http::HeaderMap;
use reqwest::{Client, ClientBuilder, Response, StatusCode};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, trace, warn, Instrument};
use url::Url;

use distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use distribution_types::{BuiltDist, File, FileLocation, IndexUrl, IndexUrls, Name};
use install_wheel_rs::metadata::{find_archive_dist_info, is_metadata_entry};
use pep440_rs::Version;
use pypi_types::{Metadata23, SimpleJson};
use uv_auth::safe_copy_url_auth;
use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_normalize::PackageName;
use uv_version::version;
use uv_warnings::warn_user_once;

use crate::cached_client::CacheControl;
use crate::html::SimpleHtml;
use crate::middleware::{NetrcMiddleware, OfflineMiddleware};
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::rkyvutil::OwnedArchive;
use crate::tls::Roots;
use crate::{tls, CachedClient, CachedClientError, Error, ErrorKind};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder {
    index_urls: IndexUrls,
    native_tls: bool,
    retries: u32,
    connectivity: Connectivity,
    cache: Cache,
    client: Option<Client>,
}

impl RegistryClientBuilder {
    pub fn new(cache: Cache) -> Self {
        Self {
            index_urls: IndexUrls::default(),
            native_tls: false,
            cache,
            connectivity: Connectivity::Online,
            retries: 3,
            client: None,
        }
    }
}

impl RegistryClientBuilder {
    #[must_use]
    pub fn index_urls(mut self, index_urls: IndexUrls) -> Self {
        self.index_urls = index_urls;
        self
    }

    #[must_use]
    pub fn connectivity(mut self, connectivity: Connectivity) -> Self {
        self.connectivity = connectivity;
        self
    }

    #[must_use]
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    #[must_use]
    pub fn native_tls(mut self, native_tls: bool) -> Self {
        self.native_tls = native_tls;
        self
    }

    #[must_use]
    pub fn cache<T>(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }

    #[must_use]
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn build(self) -> RegistryClient {
        // Create user agent.
        let user_agent_string = format!("uv/{}", version());

        // Timeout options, matching https://doc.rust-lang.org/nightly/cargo/reference/config.html#httptimeout
        // `UV_REQUEST_TIMEOUT` is provided for backwards compatibility with v0.1.6
        let default_timeout = 5 * 60;
        let timeout = env::var("UV_HTTP_TIMEOUT")
            .or_else(|_| env::var("UV_REQUEST_TIMEOUT"))
            .or_else(|_| env::var("HTTP_TIMEOUT"))
            .and_then(|value| {
                value.parse::<u64>()
                    .or_else(|_| {
                        // On parse error, warn and use the default timeout
                        warn_user_once!("Ignoring invalid value from environment for UV_HTTP_TIMEOUT. Expected integer number of seconds, got \"{value}\".");
                        Ok(default_timeout)
                    })
            })
            .unwrap_or(default_timeout);
        debug!("Using registry request timeout of {}s", timeout);

        // Initialize the base client.
        let client = self.client.unwrap_or_else(|| {
            // Load the TLS configuration.
            let tls = tls::load(if self.native_tls {
                Roots::Native
            } else {
                Roots::Webpki
            })
            .expect("Failed to load TLS configuration.");

            let client_core = ClientBuilder::new()
                .user_agent(user_agent_string)
                .pool_max_idle_per_host(20)
                .timeout(std::time::Duration::from_secs(timeout))
                .use_preconfigured_tls(tls);

            client_core.build().expect("Failed to build HTTP client.")
        });

        // Wrap in any relevant middleware.
        let client = match self.connectivity {
            Connectivity::Online => {
                let client = reqwest_middleware::ClientBuilder::new(client.clone());

                // Initialize the retry strategy.
                let retry_policy =
                    ExponentialBackoff::builder().build_with_max_retries(self.retries);
                let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);
                let client = client.with(retry_strategy);

                // Initialize the netrc middleware.
                let client = if let Ok(netrc) = NetrcMiddleware::new() {
                    client.with(netrc)
                } else {
                    client
                };

                client.build()
            }
            Connectivity::Offline => reqwest_middleware::ClientBuilder::new(client.clone())
                .with(OfflineMiddleware)
                .build(),
        };

        // Wrap in the cache middleware.
        let client = CachedClient::new(client);

        RegistryClient {
            index_urls: self.index_urls,
            cache: self.cache,
            connectivity: self.connectivity,
            client,
            timeout,
        }
    }
}

/// A client for fetching packages from a `PyPI`-compatible index.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    /// The index URLs to use for fetching packages.
    index_urls: IndexUrls,
    /// The underlying HTTP client.
    client: CachedClient,
    /// Used for the remote wheel METADATA cache.
    cache: Cache,
    /// The connectivity mode to use.
    connectivity: Connectivity,
    /// Configured client timeout, in seconds.
    timeout: u64,
}

impl RegistryClient {
    /// Return the [`CachedClient`] used by this client.
    pub fn cached_client(&self) -> &CachedClient {
        &self.client
    }

    /// Return the [`Connectivity`] mode used by this client.
    pub fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    /// Return the timeout this client is configured with, in seconds.
    pub fn timeout(&self) -> u64 {
        self.timeout
    }

    /// Set the index URLs to use for fetching packages.
    #[must_use]
    pub fn with_index_url(self, index_urls: IndexUrls) -> Self {
        Self { index_urls, ..self }
    }

    /// Fetch a package from the `PyPI` simple API.
    ///
    /// "simple" here refers to [PEP 503 – Simple Repository API](https://peps.python.org/pep-0503/)
    /// and [PEP 691 – JSON-based Simple API for Python Package Indexes](https://peps.python.org/pep-0691/),
    /// which the pypi json api approximately implements.
    #[instrument("simple_api", skip_all, fields(package = % package_name))]
    pub async fn simple(
        &self,
        package_name: &PackageName,
    ) -> Result<(IndexUrl, OwnedArchive<SimpleMetadata>), Error> {
        let mut it = self.index_urls.indexes().peekable();
        if it.peek().is_none() {
            return Err(ErrorKind::NoIndex(package_name.as_ref().to_string()).into());
        }

        for index in it {
            let result = self.simple_single_index(package_name, index).await?;

            return match result {
                Ok(metadata) => Ok((index.clone(), metadata)),
                Err(CachedClientError::Client(err)) => match err.into_kind() {
                    ErrorKind::Offline(_) => continue,
                    ErrorKind::ReqwestError(err) => {
                        if err.status() == Some(StatusCode::NOT_FOUND)
                            || err.status() == Some(StatusCode::FORBIDDEN)
                        {
                            continue;
                        }
                        Err(ErrorKind::from(err).into())
                    }
                    other => Err(other.into()),
                },
                Err(CachedClientError::Callback(err)) => Err(err),
            };
        }

        match self.connectivity {
            Connectivity::Online => {
                Err(ErrorKind::PackageNotFound(package_name.to_string()).into())
            }
            Connectivity::Offline => Err(ErrorKind::Offline(package_name.to_string()).into()),
        }
    }

    async fn simple_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
    ) -> Result<Result<OwnedArchive<SimpleMetadata>, CachedClientError<Error>>, Error> {
        // Format the URL for PyPI.
        let mut url: Url = index.clone().into();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push(package_name.as_ref())
            // The URL *must* end in a trailing slash for proper relative path behavior
            // ref https://github.com/servo/rust-url/issues/333
            .push("");

        trace!("Fetching metadata for {package_name} from {url}");

        let cache_entry = self.cache.entry(
            CacheBucket::Simple,
            Path::new(&match index {
                IndexUrl::Pypi(_) => "pypi".to_string(),
                IndexUrl::Url(url) => cache_key::digest(&cache_key::CanonicalUrl::new(url)),
            }),
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

        let simple_request = self
            .client
            .uncached()
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .header("Accept", MediaType::accepts())
            .build()
            .map_err(ErrorKind::from)?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = safe_copy_url_auth(&url, response.url().clone());

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
                        let bytes = response.bytes().await.map_err(ErrorKind::from)?;
                        let data: SimpleJson = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;

                        SimpleMetadata::from_files(data.files, package_name, &url)
                    }
                    MediaType::Html => {
                        let text = response.text().await.map_err(ErrorKind::from)?;
                        let SimpleHtml { base, files } = SimpleHtml::parse(&text, &url)
                            .map_err(|err| Error::from_html_err(err, url.clone()))?;
                        let base = safe_copy_url_auth(&url, base.into_url());

                        SimpleMetadata::from_files(files, package_name, &base)
                    }
                };
                OwnedArchive::from_unarchived(&unarchived)
            }
            .boxed()
            .instrument(info_span!("parse_simple_api", package = %package_name))
        };
        let result = self
            .client
            .get_cacheable(
                simple_request,
                &cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await;
        Ok(result)
    }

    /// Fetch the metadata for a remote wheel file.
    ///
    /// For a remote wheel, we try the following ways to fetch the metadata:
    /// 1. From a [PEP 658](https://peps.python.org/pep-0658/) data-dist-info-metadata url
    /// 2. From a remote wheel by partial zip reading
    /// 3. From a (temp) download of a remote wheel (this is a fallback, the webserver should support range requests)
    #[instrument(skip_all, fields(% built_dist))]
    pub async fn wheel_metadata(&self, built_dist: &BuiltDist) -> Result<Metadata23, Error> {
        let metadata = match &built_dist {
            BuiltDist::Registry(wheel) => match &wheel.file.url {
                FileLocation::RelativeUrl(base, url) => {
                    let url = pypi_types::base_url_join_relative(base, url)
                        .map_err(ErrorKind::JoinRelativeError)?;
                    self.wheel_metadata_registry(&wheel.index, &wheel.file, &url)
                        .await?
                }
                FileLocation::AbsoluteUrl(url) => {
                    let url = Url::parse(url).map_err(ErrorKind::UrlParseError)?;
                    self.wheel_metadata_registry(&wheel.index, &wheel.file, &url)
                        .await?
                }
                FileLocation::Path(path) => {
                    let file = fs_err::tokio::File::open(&path)
                        .await
                        .map_err(ErrorKind::Io)?;
                    let reader = tokio::io::BufReader::new(file);
                    read_metadata_async_seek(&wheel.filename, built_dist.to_string(), reader)
                        .await?
                }
            },
            BuiltDist::DirectUrl(wheel) => {
                self.wheel_metadata_no_pep658(
                    &wheel.filename,
                    &wheel.url,
                    WheelCache::Url(&wheel.url),
                )
                .await?
            }
            BuiltDist::Path(wheel) => {
                let file = fs_err::tokio::File::open(&wheel.path)
                    .await
                    .map_err(ErrorKind::Io)?;
                let reader = tokio::io::BufReader::new(file);
                read_metadata_async_seek(&wheel.filename, built_dist.to_string(), reader).await?
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
    ) -> Result<Metadata23, Error> {
        // If the metadata file is available at its own url (PEP 658), download it from there.
        let filename = WheelFilename::from_str(&file.filename).map_err(ErrorKind::WheelFilename)?;
        if file
            .dist_info_metadata
            .as_ref()
            .is_some_and(pypi_types::DistInfoMetadata::is_available)
        {
            let mut url = url.clone();
            url.set_path(&format!("{}.metadata", url.path()));

            let cache_entry = self.cache.entry(
                CacheBucket::Wheels,
                WheelCache::Index(index).remote_wheel_dir(filename.name.as_ref()),
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
                let bytes = response.bytes().await.map_err(ErrorKind::from)?;

                info_span!("parse_metadata21")
                    .in_scope(|| Metadata23::parse_metadata(bytes.as_ref()))
                    .map_err(|err| {
                        Error::from(ErrorKind::MetadataParseError(
                            filename,
                            url.to_string(),
                            Box::new(err),
                        ))
                    })
            };
            let req = self
                .client
                .uncached()
                .get(url.clone())
                .build()
                .map_err(ErrorKind::from)?;
            Ok(self
                .client
                .get_serde(req, &cache_entry, cache_control, response_callback)
                .await?)
        } else {
            // If we lack PEP 658 support, try using HTTP range requests to read only the
            // `.dist-info/METADATA` file from the zip, and if that also fails, download the whole wheel
            // into the cache and read from there
            self.wheel_metadata_no_pep658(&filename, url, WheelCache::Index(index))
                .await
        }
    }

    /// Get the wheel metadata if it isn't available in an index through PEP 658
    async fn wheel_metadata_no_pep658<'data>(
        &self,
        filename: &'data WheelFilename,
        url: &'data Url,
        cache_shard: WheelCache<'data>,
    ) -> Result<Metadata23, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::Wheels,
            cache_shard.remote_wheel_dir(filename.name.as_ref()),
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

        let req = self
            .client
            .uncached()
            .head(url.clone())
            .header(
                "accept-encoding",
                http::HeaderValue::from_static("identity"),
            )
            .build()
            .map_err(ErrorKind::from)?;

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
                    self.client.uncached(),
                    response,
                    headers,
                )
                .await
                .map_err(ErrorKind::AsyncHttpRangeReader)?;
                trace!("Getting metadata for {filename} by range request");
                let text = wheel_metadata_from_remote_zip(filename, &mut reader).await?;
                let metadata = Metadata23::parse_metadata(text.as_bytes()).map_err(|err| {
                    Error::from(ErrorKind::MetadataParseError(
                        filename.clone(),
                        url.to_string(),
                        Box::new(err),
                    ))
                })?;
                Ok::<Metadata23, CachedClientError<Error>>(metadata)
            }
            .boxed()
            .instrument(info_span!("read_metadata_range_request", wheel = %filename))
        };

        let result = self
            .client
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
                } else {
                    return Err(err);
                }
            }
        };

        // Create a request to stream the file.
        let req = self
            .client
            .uncached()
            .get(url.clone())
            .header(
                // `reqwest` defaults to accepting compressed responses.
                // Specify identity encoding to get consistent .whl downloading
                // behavior from servers. ref: https://github.com/pypa/pip/pull/1688
                "accept-encoding",
                reqwest::header::HeaderValue::from_static("identity"),
            )
            .build()
            .map_err(ErrorKind::from)?;

        // Stream the file, searching for the METADATA.
        let read_metadata_stream = |response: Response| {
            async {
                let reader = response
                    .bytes_stream()
                    .map_err(|err| self.handle_response_errors(err))
                    .into_async_read();

                read_metadata_async_stream(filename, url.to_string(), reader).await
            }
            .instrument(info_span!("read_metadata_stream", wheel = %filename))
        };

        self.client
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
                    "Failed to download distribution due to network timeout. Try increasing UV_HTTP_TIMEOUT (current value: {}s).",  self.timeout()
                ),
            )
        } else {
            std::io::Error::new(std::io::ErrorKind::Other, err)
        }
    }
}

/// Read a wheel's `METADATA` file from a zip file.
async fn read_metadata_async_seek(
    filename: &WheelFilename,
    debug_source: String,
    reader: impl tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
) -> Result<Metadata23, Error> {
    let mut zip_reader = async_zip::tokio::read::seek::ZipFileReader::with_tokio(reader)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    let (metadata_idx, _dist_info_prefix) = find_archive_dist_info(
        filename,
        zip_reader
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((index, entry.filename().as_str().ok()?))),
    )
    .map_err(ErrorKind::InstallWheel)?;

    // Read the contents of the `METADATA` file.
    let mut contents = Vec::new();
    zip_reader
        .reader_with_entry(metadata_idx)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?
        .read_to_end_checked(&mut contents)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    let metadata = Metadata23::parse_metadata(&contents).map_err(|err| {
        ErrorKind::MetadataParseError(filename.clone(), debug_source, Box::new(err))
    })?;
    Ok(metadata)
}

/// Like [`read_metadata_async_seek`], but doesn't use seek.
async fn read_metadata_async_stream<R: futures::AsyncRead + Unpin>(
    filename: &WheelFilename,
    debug_source: String,
    reader: R,
) -> Result<Metadata23, Error> {
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(reader);

    while let Some(mut entry) = zip
        .next_with_entry()
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?
    {
        // Find the `METADATA` entry.
        let path = entry
            .reader()
            .entry()
            .filename()
            .as_str()
            .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

        if is_metadata_entry(path, filename) {
            let mut reader = entry.reader_mut().compat();
            let mut contents = Vec::new();
            reader.read_to_end(&mut contents).await.unwrap();

            let metadata = Metadata23::parse_metadata(&contents).map_err(|err| {
                ErrorKind::MetadataParseError(filename.clone(), debug_source, Box::new(err))
            })?;
            return Ok(metadata);
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry
            .skip()
            .await
            .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;
    }

    Err(ErrorKind::MetadataNotFound(filename.clone(), debug_source).into())
}

#[derive(
    Default, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct VersionFiles {
    pub wheels: Vec<VersionWheel>,
    pub source_dists: Vec<VersionSourceDist>,
}

impl VersionFiles {
    fn push(&mut self, filename: DistFilename, file: File) {
        match filename {
            DistFilename::WheelFilename(name) => self.wheels.push(VersionWheel { name, file }),
            DistFilename::SourceDistFilename(name) => {
                self.source_dists.push(VersionSourceDist { name, file })
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

#[derive(Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct VersionWheel {
    pub name: WheelFilename,
    pub file: File,
}

#[derive(Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct VersionSourceDist {
    pub name: SourceDistFilename,
    pub file: File,
}

#[derive(
    Default, Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct SimpleMetadata(Vec<SimpleMetadatum>);

#[derive(Debug, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct SimpleMetadatum {
    pub version: Version,
    pub files: VersionFiles,
}

impl SimpleMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &SimpleMetadatum> {
        self.0.iter()
    }

    fn from_files(files: Vec<pypi_types::File>, package_name: &PackageName, base: &Url) -> Self {
        let mut map: BTreeMap<Version, VersionFiles> = BTreeMap::default();

        // Group the distributions by version and kind
        for file in files {
            if let Some(filename) =
                DistFilename::try_from_filename(file.filename.as_str(), package_name)
            {
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
        }
        Self(
            map.into_iter()
                .map(|(version, files)| SimpleMetadatum { version, files })
                .collect(),
        )
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Connectivity {
    /// Allow access to the network.
    Online,

    /// Do not allow access to the network.
    Offline,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use url::Url;

    use pypi_types::{JoinRelativeError, SimpleJson};
    use uv_normalize::PackageName;

    use crate::{html::SimpleHtml, SimpleMetadata, SimpleMetadatum};

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
        let base = Url::parse("https://pypi.org/simple/pyflyby/").unwrap();
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
    /// Regression coverage of https://github.com/astral-sh/uv/issues/1388
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
        let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask")
            .unwrap();
        let SimpleHtml { base, files } = SimpleHtml::parse(text, &base).unwrap();

        // Test parsing of the file urls
        let urls = files
            .iter()
            .map(|file| pypi_types::base_url_join_relative(base.as_url().as_str(), &file.url))
            .collect::<Result<Vec<_>, JoinRelativeError>>()?;
        let urls = urls.iter().map(|url| url.as_str()).collect::<Vec<_>>();
        insta::assert_debug_snapshot!(urls, @r###"
        [
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
            "https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
        ]
        "###);

        Ok(())
    }
}
