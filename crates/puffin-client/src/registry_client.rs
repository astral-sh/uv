use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::Path;
use std::str::FromStr;

use async_http_range_reader::{AsyncHttpRangeReader, AsyncHttpRangeReaderError};
use async_zip::tokio::read::seek::ZipFileReader;
use futures::{FutureExt, TryStreamExt};
use reqwest::{Client, ClientBuilder, Response, StatusCode};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use serde::{Deserialize, Serialize};
use tempfile::tempfile_in;
use tokio::io::BufWriter;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, instrument, trace, warn, Instrument};
use url::Url;

use distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use distribution_types::{BuiltDist, File, FileLocation, IndexUrl, IndexUrls, Name};
use install_wheel_rs::find_dist_info;
use pep440_rs::Version;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_normalize::PackageName;
use pypi_types::{BaseUrl, Metadata21, SimpleJson};

use crate::cached_client::CacheControl;
use crate::html::SimpleHtml;
use crate::remote_metadata::wheel_metadata_from_remote_zip;
use crate::{CachedClient, CachedClientError, Error, ErrorKind};

/// A builder for an [`RegistryClient`].
#[derive(Debug, Clone)]
pub struct RegistryClientBuilder {
    index_urls: IndexUrls,
    retries: u32,
    cache: Cache,
}

impl RegistryClientBuilder {
    pub fn new(cache: Cache) -> Self {
        Self {
            index_urls: IndexUrls::default(),
            cache,
            retries: 3,
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
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    #[must_use]
    pub fn cache<T>(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }

    pub fn build(self) -> RegistryClient {
        let client_raw = {
            let client_core = ClientBuilder::new()
                .user_agent("puffin")
                .pool_max_idle_per_host(20)
                .timeout(std::time::Duration::from_secs(60 * 5));

            client_core.build().expect("Failed to build HTTP client.")
        };

        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(self.retries);
        let retry_strategy = RetryTransientMiddleware::new_with_policy(retry_policy);

        let uncached_client = reqwest_middleware::ClientBuilder::new(client_raw.clone())
            .with(retry_strategy)
            .build();

        let client = CachedClient::new(uncached_client.clone());
        RegistryClient {
            index_urls: self.index_urls,
            client_raw: client_raw.clone(),
            cache: self.cache,
            client,
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
    /// Don't use this client, it only exists because `async_http_range_reader` needs
    /// [`reqwest::Client] instead of [`reqwest_middleware::Client`]
    client_raw: Client,
    /// Used for the remote wheel METADATA cache
    cache: Cache,
}

impl RegistryClient {
    /// Return the [`CachedClient`] used by this client.
    pub fn cached_client(&self) -> &CachedClient {
        &self.client
    }

    /// Fetch a package from the `PyPI` simple API.
    ///
    /// "simple" here refers to [PEP 503 – Simple Repository API](https://peps.python.org/pep-0503/)
    /// and [PEP 691 – JSON-based Simple API for Python Package Indexes](https://peps.python.org/pep-0691/),
    /// which the pypi json api approximately implements.
    #[instrument("simple_api", skip_all, fields(package = %package_name))]
    pub async fn simple(
        &self,
        package_name: &PackageName,
    ) -> Result<(IndexUrl, SimpleMetadata), Error> {
        if self.index_urls.no_index() {
            return Err(ErrorKind::NoIndex(package_name.as_ref().to_string()).into());
        }

        for index in self.index_urls.indexes() {
            let result = self.simple_single_index(package_name, index).await?;

            return match result {
                Ok(metadata) => Ok((index.clone(), metadata)),
                Err(CachedClientError::Client(err)) => match err.into_kind() {
                    ErrorKind::RequestError(err) => {
                        if err.status() == Some(StatusCode::NOT_FOUND) {
                            continue;
                        }
                        Err(ErrorKind::RequestError(err).into())
                    }
                    other => Err(other.into()),
                },
                Err(CachedClientError::Callback(err)) => Err(err),
            };
        }

        Err(ErrorKind::PackageNotFound(package_name.to_string()).into())
    }

    async fn simple_single_index(
        &self,
        package_name: &PackageName,
        index: &IndexUrl,
    ) -> Result<Result<SimpleMetadata, CachedClientError<Error>>, Error> {
        // Format the URL for PyPI.
        let mut url: Url = index.clone().into();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push(package_name.as_ref());

        trace!("Fetching metadata for {package_name} from {url}");

        let cache_entry = self.cache.entry(
            CacheBucket::Simple,
            Path::new(&match index {
                IndexUrl::Pypi => "pypi".to_string(),
                IndexUrl::Url(url) => cache_key::digest(&cache_key::CanonicalUrl::new(url)),
            }),
            format!("{package_name}.msgpack"),
        );
        let cache_control = CacheControl::from(
            self.cache
                .freshness(&cache_entry, Some(package_name))
                .map_err(ErrorKind::Io)?,
        );

        let simple_request = self
            .client
            .uncached()
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .header("Accept", MediaType::accepts())
            .build()
            .map_err(ErrorKind::RequestError)?;
        let parse_simple_response = |response: Response| {
            async {
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

                match media_type {
                    MediaType::Json => {
                        let bytes = response.bytes().await.map_err(ErrorKind::RequestError)?;
                        let data: SimpleJson = serde_json::from_slice(bytes.as_ref())
                            .map_err(|err| Error::from_json_err(err, url.clone()))?;
                        let metadata =
                            SimpleMetadata::from_files(data.files, package_name, url.as_str());
                        Ok(metadata)
                    }
                    MediaType::Html => {
                        let text = response.text().await.map_err(ErrorKind::RequestError)?;
                        let SimpleHtml { base, files } = SimpleHtml::parse(&text, &url)
                            .map_err(|err| Error::from_html_err(err, url.clone()))?;
                        let metadata =
                            SimpleMetadata::from_files(files, package_name, base.as_url().as_str());
                        Ok(metadata)
                    }
                }
            }
            .boxed()
            .instrument(info_span!("parse_simple_api", package = %package_name))
        };
        let result = self
            .client
            .get_cached_with_callback(
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
    #[instrument(skip(self))]
    pub async fn wheel_metadata(&self, built_dist: &BuiltDist) -> Result<Metadata21, Error> {
        let metadata = match &built_dist {
            BuiltDist::Registry(wheel) => match &wheel.file.url {
                FileLocation::RelativeUrl(base, url) => {
                    let base = BaseUrl::from(Url::parse(base).map_err(ErrorKind::UrlParseError)?);
                    let url = base.join_relative(url).map_err(ErrorKind::UrlParseError)?;
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
                    read_metadata_async(&wheel.filename, built_dist.to_string(), reader).await?
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
                read_metadata_async(&wheel.filename, built_dist.to_string(), reader).await?
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
    ) -> Result<Metadata21, Error> {
        // If the metadata file is available at its own url (PEP 658), download it from there.
        let filename = WheelFilename::from_str(&file.filename).map_err(ErrorKind::WheelFilename)?;
        if file
            .dist_info_metadata
            .as_ref()
            .is_some_and(pypi_types::DistInfoMetadata::is_available)
        {
            let url = Url::parse(&format!("{}.metadata", url)).map_err(ErrorKind::UrlParseError)?;

            let cache_entry = self.cache.entry(
                CacheBucket::Wheels,
                WheelCache::Index(index).remote_wheel_dir(filename.name.as_ref()),
                format!("{}.msgpack", filename.stem()),
            );
            let cache_control = CacheControl::from(
                self.cache
                    .freshness(&cache_entry, Some(&filename.name))
                    .map_err(ErrorKind::Io)?,
            );

            let response_callback = |response: Response| async {
                let bytes = response.bytes().await.map_err(ErrorKind::RequestError)?;

                info_span!("parse_metadata21")
                    .in_scope(|| Metadata21::parse(bytes.as_ref()))
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
                .map_err(ErrorKind::RequestError)?;
            Ok(self
                .client
                .get_cached_with_callback(req, &cache_entry, cache_control, response_callback)
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
    ) -> Result<Metadata21, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::Wheels,
            cache_shard.remote_wheel_dir(filename.name.as_ref()),
            format!("{}.msgpack", filename.stem()),
        );
        let cache_control = CacheControl::from(
            self.cache
                .freshness(&cache_entry, Some(&filename.name))
                .map_err(ErrorKind::Io)?,
        );

        // This response callback is special, we actually make a number of subsequent requests to
        // fetch the file from the remote zip.
        let client = self.client_raw.clone();
        let read_metadata_range_request = |response: Response| {
            async {
                let mut reader = AsyncHttpRangeReader::from_head_response(client, response)
                    .await
                    .map_err(ErrorKind::AsyncHttpRangeReader)?;
                trace!("Getting metadata for {filename} by range request");
                let text = wheel_metadata_from_remote_zip(filename, &mut reader).await?;
                let metadata = Metadata21::parse(text.as_bytes()).map_err(|err| {
                    Error::from(ErrorKind::MetadataParseError(
                        filename.clone(),
                        url.to_string(),
                        Box::new(err),
                    ))
                })?;
                Ok::<Metadata21, CachedClientError<Error>>(metadata)
            }
            .boxed()
            .instrument(info_span!("read_metadata_range_request", wheel = %filename))
        };

        let req = self
            .client
            .uncached()
            .head(url.clone())
            .build()
            .map_err(ErrorKind::RequestError)?;
        let result = self
            .client
            .get_cached_with_callback(
                req,
                &cache_entry,
                cache_control,
                read_metadata_range_request,
            )
            .await
            .map_err(crate::Error::from);

        match result {
            Ok(metadata) => return Ok(metadata),
            Err(err) => match err.into_kind() {
                ErrorKind::AsyncHttpRangeReader(
                    AsyncHttpRangeReaderError::HttpRangeRequestUnsupported,
                ) => {}
                kind => return Err(kind.into()),
            },
        }

        // The range request version failed (this is bad, the webserver should support this), fall
        // back to downloading the entire file and the reading the file from the zip the regular way

        debug!("Range requests not supported for {filename}; downloading wheel");
        // TODO(konstin): Download the wheel into a cache shared with the installer instead
        // Note that this branch is only hit when you're not using and the server where
        // you host your wheels for some reasons doesn't support range requests
        // (tbh we should probably warn here and tell users to get a better registry because
        // their current one makes resolution unnecessary slow).
        let temp_download = tempfile_in(self.cache.root()).map_err(ErrorKind::CacheWrite)?;
        let mut writer = BufWriter::new(tokio::fs::File::from_std(temp_download));
        let mut reader = self.stream_external(url).await?.compat();
        tokio::io::copy(&mut reader, &mut writer)
            .await
            .map_err(ErrorKind::CacheWrite)?;
        let reader = writer.into_inner();

        read_metadata_async(filename, url.to_string(), reader).await
    }

    /// Stream a file from an external URL.
    pub async fn stream_external(
        &self,
        url: &Url,
    ) -> Result<Box<dyn futures::AsyncRead + Unpin + Send + Sync>, Error> {
        Ok(Box::new(
            self.client
                .uncached()
                .get(url.to_string())
                .send()
                .await
                .map_err(ErrorKind::RequestMiddlewareError)?
                .error_for_status()
                .map_err(ErrorKind::RequestError)?
                .bytes_stream()
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                .into_async_read(),
        ))
    }
}

/// It doesn't really fit into `puffin_client`, but it avoids cyclical crate dependencies.
pub async fn read_metadata_async(
    filename: &WheelFilename,
    debug_source: String,
    reader: impl tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
) -> Result<Metadata21, Error> {
    let mut zip_reader = ZipFileReader::with_tokio(reader)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    let (metadata_idx, _dist_info_prefix) = find_dist_info(
        filename,
        zip_reader
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| Some((idx, e.filename().as_str().ok()?))),
    )
    .map_err(ErrorKind::InstallWheel)?;

    // Read the contents of the METADATA file
    let mut contents = Vec::new();
    zip_reader
        .reader_with_entry(metadata_idx)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?
        .read_to_end_checked(&mut contents)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    let metadata = Metadata21::parse(&contents).map_err(|err| {
        ErrorKind::MetadataParseError(filename.clone(), debug_source, Box::new(err))
    })?;
    Ok(metadata)
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
pub struct SimpleMetadata(BTreeMap<Version, VersionFiles>);

impl SimpleMetadata {
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&Version, &VersionFiles)> {
        self.0.iter()
    }

    fn from_files(files: Vec<pypi_types::File>, package_name: &PackageName, base: &str) -> Self {
        let mut metadata = Self::default();

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
                        // Ignore files with unparseable version specifiers.
                        warn!("Skipping file for {package_name}: {err}");
                        continue;
                    }
                };
                match metadata.0.entry(version.clone()) {
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

        metadata
    }
}

impl IntoIterator for SimpleMetadata {
    type Item = (Version, VersionFiles);
    type IntoIter = std::collections::btree_map::IntoIter<Version, VersionFiles>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use puffin_normalize::PackageName;
    use pypi_types::SimpleJson;

    use crate::SimpleMetadata;

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
        let base = "https://pypi.org/simple/pyflyby/";
        let simple_metadata = SimpleMetadata::from_files(
            data.files,
            &PackageName::from_str("pyflyby").unwrap(),
            &base,
        );
        let versions: Vec<String> = simple_metadata
            .iter()
            .map(|(version, _)| version.to_string())
            .collect();
        assert_eq!(versions, ["1.7.8".to_string()]);
    }
}
