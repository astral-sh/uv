//! Discover available Python downloads and fetch their distributions.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTimeError};
use std::{env, io};
use uv_python_types::{PythonDownloadRequest, PythonDownloadRequestError};

use futures::TryStreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use reqwest::Response;
use reqwest_retry::RetryError;
use reqwest_retry::policies::ExponentialBackoff;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWriteExt, BufWriter, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::either::Either;
use tracing::{debug, instrument};
use url::Url;
use zstd::stream::read::Decoder;

use uv_cache::{Cache, CacheBucket};
use uv_cache_key::cache_digest;
use uv_client::{
    BaseClient, BaseClientBuilder, CacheControl, CachedClient, CachedClientError, ClientBuildError,
    Connectivity, RetriableError, RetryState, WrappedReqwestError, fetch_with_url_fallback,
    retryable_on_request_failure,
};
use uv_distribution_filename::{ExtensionError, SourceDistExtension};
use uv_extract::hash::Hasher;
use uv_fs::{Simplified, rename_with_retry};
use uv_macros::DebugNoInline;
use uv_platform::{Arch, Libc, Os, Platform};
use uv_pypi_types::{Digest, HashAlgorithm, HashDigest};
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_static::{
    EnvVars, astral_mirror_base_url, astral_mirror_url_from_env, custom_astral_mirror_url,
};

use uv_python_types::{
    ImplementationName, LenientImplementationName, PythonDownloadMirrors, PythonInstallationKey,
    PythonVariant, PythonVersion,
};

#[derive(Error, DebugNoInline)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Request(#[from] PythonDownloadRequestError),
    #[error("Expected download URL (`{0}`) to end in a supported file extension: {1}")]
    MissingExtension(String, ExtensionError),
    #[error("Failed to download `{0}`")]
    NetworkError(DisplaySafeUrl, #[source] WrappedReqwestError),
    #[error(
        "Request failed after {retries} {subject} in {duration:.1}s",
        subject = if *retries > 1 { "retries" } else { "retry" },
        duration = duration.as_secs_f32()
    )]
    NetworkErrorWithRetries {
        #[source]
        err: Box<Self>,
        retries: u32,
        duration: Duration,
    },
    #[error("Failed to download `{0}`")]
    NetworkMiddlewareError(DisplaySafeUrl, #[source] anyhow::Error),
    #[error("Failed to extract archive: {0}")]
    ExtractError(String, #[source] uv_extract::Error),
    #[error("Failed to hash installation")]
    HashExhaustion(#[source] io::Error),
    #[error("Hash mismatch for `{installation}`\n\nExpected:\n{expected}\n\nComputed:\n{actual}")]
    HashMismatch {
        installation: String,
        expected: String,
        actual: String,
    },
    #[error("Invalid download URL")]
    InvalidUrl(#[from] DisplaySafeUrlError),
    #[error("Invalid download URL: {0}")]
    InvalidUrlFormat(DisplaySafeUrl),
    #[error("Invalid path in file URL: {0}")]
    InvalidFileUrl(String),
    #[error("Failed to create download directory")]
    DownloadDirError(#[source] io::Error),
    #[error("Failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to read managed Python installation directory: {0}", dir.user_display())]
    ReadError {
        dir: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("No download found for request: {}", _0.green())]
    NoDownloadFound(PythonDownloadRequest),
    #[error("A mirror was provided via `{0}`, but the URL does not match the expected format: {1}")]
    Mirror(&'static str, DisplaySafeUrl),
    #[error("Unable to parse the JSON Python download list at `{0}`")]
    InvalidPythonDownloadsJSON(String, #[source] serde_json::Error),
    #[error("This version of uv is too old to support the JSON Python download list at `{0}`")]
    UnsupportedPythonDownloadsJSON(String),
    #[error("Error while fetching remote python downloads json from `{0}`")]
    FetchingPythonDownloadsJSONError(String, #[source] Box<Self>),
    #[error(transparent)]
    RemotePythonDownloadsJSONClient(Box<uv_client::Error>),
    #[error(transparent)]
    ClientBuild(Box<ClientBuildError>),
    #[error("An offline Python installation was requested, but `{file}` (from `{url}`) is missing in `{}`", python_builds_dir.user_display())]
    OfflinePythonMissing {
        file: Box<PythonInstallationKey>,
        url: Box<DisplaySafeUrl>,
        python_builds_dir: PathBuf,
    },
    #[error("No download URL found for Python")]
    NoPythonDownloadUrlFound,
    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

impl RetriableError for Error {
    // Return the number of retries that were made to complete this request before this error was
    // returned.
    //
    // Note that e.g. 3 retries equates to 4 attempts.
    fn retries(&self) -> u32 {
        // Unfortunately different variants of `Error` track retry counts in different ways. We
        // could consider unifying the variants we handle here in `Error::from_reqwest_middleware`
        // instead, but both approaches will be fragile as new variants get added over time.
        if let Self::NetworkErrorWithRetries { retries, .. } = self {
            return *retries;
        }
        if let Self::NetworkMiddlewareError(_, anyhow_error) = self
            && let Some(RetryError::WithRetries { retries, .. }) =
                anyhow_error.downcast_ref::<RetryError>()
        {
            return *retries;
        }
        0
    }

    /// Returns `true` if trying an alternative URL makes sense after this error.
    ///
    /// HTTP-level failures (4xx, 5xx) and connection-level failures return `true`.
    /// Hash mismatches, extraction failures, and similar post-download errors return `false`
    /// because switching to a different host would not fix them.
    fn should_try_next_url(&self) -> bool {
        match self {
            // There are two primary reasons to try an alternative URL:
            // - HTTP/DNS/TCP/etc errors due to a mirror being blocked at various layers
            // - HTTP 404s from the mirror, which may mean the next URL still works
            // So we catch all network-level errors here.
            Self::NetworkError(..)
            | Self::NetworkMiddlewareError(..)
            | Self::NetworkErrorWithRetries { .. } => true,
            // `Io` uses `#[error(transparent)]`, so `source()` delegates to the inner error's
            // own source rather than returning the `io::Error` itself. We must unwrap it
            // explicitly so that `retryable_on_request_failure` can inspect the io error kind.
            Self::Io(err) => retryable_on_request_failure(err).is_some(),
            _ => false,
        }
    }

    fn into_retried(self, retries: u32, duration: Duration) -> Self {
        Self::NetworkErrorWithRetries {
            err: Box::new(self),
            retries,
            duration,
        }
    }
}

/// The URL prefix used by `python-build-standalone` releases on GitHub.
const CPYTHON_DOWNLOADS_URL_PREFIX: &str =
    "https://github.com/astral-sh/python-build-standalone/releases/download/";

/// The suffix appended to the Astral mirror base for `python-build-standalone` releases.
const CPYTHON_MIRROR_SUFFIX: &str = "/github/python-build-standalone/releases/download/";

/// Return the Astral mirror base URL for CPython downloads.
fn effective_cpython_mirror(astral_mirror_url: Option<&str>) -> String {
    format!(
        "{}{CPYTHON_MIRROR_SUFFIX}",
        astral_mirror_base_url(astral_mirror_url)
    )
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct ManagedPythonDownload {
    key: PythonInstallationKey,
    url: Cow<'static, str>,
    sha256: Option<Digest<32>>,
    build: Option<&'static str>,
}

const BUILTIN_PYTHON_DOWNLOADS_ZSTD: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/download-metadata.json.zst"));

pub struct ManagedPythonDownloadList {
    downloads: Vec<ManagedPythonDownload>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct JsonPythonDownload {
    name: String,
    arch: JsonArch,
    os: String,
    libc: String,
    major: u8,
    minor: u8,
    patch: u8,
    prerelease: Option<String>,
    url: String,
    sha256: Option<Digest<32>>,
    variant: Option<String>,
    build: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct JsonArch {
    family: String,
    variant: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DownloadResult {
    AlreadyAvailable(PathBuf),
    Fetched(PathBuf),
}

impl ManagedPythonDownloadList {
    /// Iterate over all [`ManagedPythonDownload`]s.
    fn iter_all(&self) -> impl Iterator<Item = &ManagedPythonDownload> {
        self.downloads.iter()
    }

    /// Iterate over all [`ManagedPythonDownload`]s that match the request.
    pub fn iter_matching(
        &self,
        request: &PythonDownloadRequest,
    ) -> impl Iterator<Item = &ManagedPythonDownload> {
        self.iter_all()
            .filter(move |download| download.matches_request(request))
    }

    /// Return the first [`ManagedPythonDownload`] matching a request, if any.
    ///
    /// If there is no stable version matching the request, a compatible pre-release version will
    /// be searched for — even if a pre-release was not explicitly requested.
    pub fn find(&self, request: &PythonDownloadRequest) -> Result<&ManagedPythonDownload, Error> {
        if let Some(download) = self.iter_matching(request).next() {
            return Ok(download);
        }

        if !request.allows_prereleases()
            && let Some(download) = self
                .iter_matching(&request.clone().with_prereleases(true))
                .next()
        {
            return Ok(download);
        }

        Err(Error::NoDownloadFound(request.clone()))
    }

    /// Load available Python distributions from a provided source or the compiled-in list.
    ///
    /// Returns an error if the provided list could not be opened, if the JSON is invalid, or if it
    /// does not parse into the expected data structure.
    pub async fn new(
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        python_downloads_json_url: Option<&str>,
    ) -> Result<Self, Error> {
        // file:// URLs are converted to local file reads, and we also support parsing bare
        // filenames like "/tmp/py.json", not just "file:///tmp/py.json". Note that
        // "C:\Temp\py.json" should be considered a filename, even though Url::parse would
        // successfully misparse it as a URL with scheme "C".
        enum Source<'a> {
            BuiltIn,
            Path(Cow<'a, Path>),
            Http(DisplaySafeUrl),
        }

        let json_source = if let Some(url_or_path) = python_downloads_json_url {
            if let Ok(url) = DisplaySafeUrl::parse(url_or_path) {
                match url.scheme() {
                    "http" | "https" => Source::Http(url),
                    "file" => Source::Path(Cow::Owned(
                        url.to_file_path().or(Err(Error::InvalidUrlFormat(url)))?,
                    )),
                    _ => Source::Path(Cow::Borrowed(Path::new(url_or_path))),
                }
            } else {
                Source::Path(Cow::Borrowed(Path::new(url_or_path)))
            }
        } else {
            Source::BuiltIn
        };

        let json_downloads = match json_source {
            Source::BuiltIn => parse_builtin_downloads()?,
            Source::Path(ref path) => parse_downloads_json(
                &fs_err::read(path.as_ref())?,
                path.to_string_lossy().to_string(),
            )?,
            Source::Http(ref url) => {
                let client = CachedClient::new(
                    client_builder
                        .build()
                        .map_err(|err| Error::ClientBuild(Box::new(err)))?,
                );
                fetch_downloads_from_url(&client, cache, url)
                    .await
                    .map_err(|e| match e {
                        e @ (Error::InvalidPythonDownloadsJSON(..)
                        | Error::UnsupportedPythonDownloadsJSON(..)) => e,
                        e => Error::FetchingPythonDownloadsJSONError(url.to_string(), Box::new(e)),
                    })?
            }
        };

        let downloads = parse_json_downloads(json_downloads);
        Ok(Self { downloads })
    }

    /// Load available Python distributions from the compiled-in list only.
    /// for testing purposes.
    pub fn new_only_embedded() -> Result<Self, Error> {
        let json_downloads = parse_builtin_downloads()?;
        let result = parse_json_downloads(json_downloads);
        Ok(Self { downloads: result })
    }
}

/// Decompress and parse the embedded Python download catalog.
fn parse_builtin_downloads() -> Result<HashMap<String, JsonPythonDownload>, Error> {
    let mut json = Vec::new();
    Decoder::with_buffer(BUILTIN_PYTHON_DOWNLOADS_ZSTD)?.read_to_end(&mut json)?;
    parse_downloads_json(&json, "EMBEDDED IN THE BINARY".to_owned())
}

/// Parse the downloads JSON.
///
/// `source` is where the JSON came from for error reporting.
fn parse_downloads_json(
    buf: &[u8],
    source: String,
) -> Result<HashMap<String, JsonPythonDownload>, Error> {
    match serde_json::from_slice(buf) {
        Ok(data) => Ok(data),
        Err(e) => {
            // As an explicit compatibility mechanism, if there's a top-level "version" key, it
            // means it's a newer format than we know how to deal with. Before reporting a
            // parse error about the format of JsonPythonDownload, check for that key. We can do
            // this by parsing into a Map<String, IgnoredAny> which allows any valid JSON on the
            // value side. (Because it's zero-sized, Clippy suggests Set<String>, but that won't
            // have the same parsing effect.)
            #[expect(clippy::zero_sized_map_values)]
            if let Ok(keys) = serde_json::from_slice::<HashMap<String, serde::de::IgnoredAny>>(buf)
                && keys.contains_key("version")
            {
                Err(Error::UnsupportedPythonDownloadsJSON(source))
            } else {
                Err(Error::InvalidPythonDownloadsJSON(source, e))
            }
        }
    }
}

async fn fetch_downloads_from_url(
    client: &CachedClient,
    cache: &Cache,
    url: &DisplaySafeUrl,
) -> Result<HashMap<String, JsonPythonDownload>, Error> {
    let cache_entry = cache.entry(
        CacheBucket::Python,
        "downloads-json",
        format!("{}.msgpack", cache_digest(&url.as_str())),
    );
    let cache_control = match client.uncached().connectivity() {
        Connectivity::Online => CacheControl::from(cache.freshness(&cache_entry, None, None)?),
        Connectivity::Offline => CacheControl::AllowStale,
    };

    let request = client
        .uncached()
        .for_host(url)
        .get(Url::from(url.clone()))
        .build()
        .map_err(|err| Error::NetworkError(url.clone(), WrappedReqwestError::from(err)))?;

    let response_callback = async |response: Response, _: &mut RetryState| {
        let bytes = response
            .bytes()
            .await
            .map_err(|err| Error::NetworkError(url.clone(), WrappedReqwestError::from(err)))?;
        parse_downloads_json(&bytes, url.to_string())
    };

    client
        .get_serde_with_retry(request, &cache_entry, cache_control, response_callback)
        .await
        .map_err(|err| match err {
            CachedClientError::Client(err) => Error::RemotePythonDownloadsJSONClient(Box::new(err)),
            CachedClientError::Callback {
                err,
                retries,
                duration,
            } => match err {
                // Avoid double-wrapping errors.
                err @ (Error::InvalidPythonDownloadsJSON(..)
                | Error::UnsupportedPythonDownloadsJSON(..)) => err,
                err if retries > 0 => err.into_retried(retries, duration),
                err => err,
            },
        })
}

impl ManagedPythonDownload {
    pub(crate) fn url(&self) -> &Cow<'static, str> {
        &self.url
    }

    pub fn key(&self) -> &PythonInstallationKey {
        &self.key
    }

    fn os(&self) -> &Os {
        self.key.os()
    }

    pub(crate) fn sha256(&self) -> Option<&Digest<32>> {
        self.sha256.as_ref()
    }

    pub fn build(&self) -> Option<&'static str> {
        self.build
    }

    /// Download and extract a Python distribution, retrying on failure.
    ///
    /// For CPython without a user-configured mirror, the default Astral mirror is tried first.
    /// Each attempt tries all URLs in sequence without backoff between them; backoff is only
    /// applied after all URLs have been exhausted.
    #[instrument(skip_all, fields(download = % self.key()))]
    pub async fn fetch_with_retry(
        &self,
        client: &BaseClient,
        retry_policy: &ExponentialBackoff,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        mirrors: PythonDownloadMirrors<'_>,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let urls = self.download_urls(mirrors)?;
        if urls.is_empty() {
            return Err(Error::NoPythonDownloadUrlFound);
        }
        fetch_with_url_fallback(&urls, *retry_policy, &format!("`{}`", self.key()), |url| {
            self.fetch_from_url(
                url,
                client,
                installation_dir,
                scratch_dir,
                reinstall,
                reporter,
            )
        })
        .await
    }

    /// Download and extract a Python distribution from the given URL.
    async fn fetch_from_url(
        &self,
        url: DisplaySafeUrl,
        client: &BaseClient,
        installation_dir: &Path,
        scratch_dir: &Path,
        reinstall: bool,
        reporter: Option<&dyn Reporter>,
    ) -> Result<DownloadResult, Error> {
        let path = installation_dir.join(self.key().to_string());

        // If it is not a reinstall and the dir already exists, return it.
        if !reinstall && path.is_dir() {
            return Ok(DownloadResult::AlreadyAvailable(path));
        }

        // We improve filesystem compatibility by using neither the URL-encoded `%2B` nor the `+` it
        // decodes to.
        let filename = url
            .path_segments()
            .ok_or_else(|| Error::InvalidUrlFormat(url.clone()))?
            .next_back()
            .ok_or_else(|| Error::InvalidUrlFormat(url.clone()))?
            .replace("%2B", "-");
        debug_assert!(
            filename
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
            "Unexpected char in filename: {filename}"
        );
        let ext = SourceDistExtension::from_path(&filename)
            .map_err(|err| Error::MissingExtension(url.to_string(), err))?;

        let temp_dir = tempfile::tempdir_in(scratch_dir).map_err(Error::DownloadDirError)?;

        let temp_dir = if let Some(python_builds_dir) =
            env::var_os(EnvVars::UV_PYTHON_CACHE_DIR).filter(|s| !s.is_empty())
        {
            let python_builds_dir = PathBuf::from(python_builds_dir);
            fs_err::create_dir_all(&python_builds_dir)?;
            let hash_prefix = match self.sha256.as_ref() {
                Some(digest) => {
                    // Shorten the hash to avoid too-long-filename errors
                    &digest.as_str()[..9]
                }
                None => "none",
            };
            let target_cache_file = python_builds_dir.join(format!("{hash_prefix}-{filename}"));

            // Download the archive to the cache, or return a reader if we have it in cache.
            // TODO(konsti): We should "tee" the write so we can do the download-to-cache and unpacking
            // in one step.
            let (reader, size): (Box<dyn AsyncRead + Unpin>, Option<u64>) =
                match fs_err::tokio::File::open(&target_cache_file).await {
                    Ok(file) => {
                        debug!(
                            "Extracting existing `{}`",
                            target_cache_file.simplified_display()
                        );
                        let size = file.metadata().await?.len();
                        let reader = Box::new(tokio::io::BufReader::new(file));
                        (reader, Some(size))
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {
                        // Point the user to which file is missing where and where to download it
                        if client.connectivity().is_offline() {
                            return Err(Error::OfflinePythonMissing {
                                file: Box::new(self.key().clone()),
                                url: Box::new(url.clone()),
                                python_builds_dir,
                            });
                        }

                        self.download_archive(
                            &url,
                            client,
                            reporter,
                            &python_builds_dir,
                            &target_cache_file,
                        )
                        .await?;

                        debug!("Extracting `{}`", target_cache_file.simplified_display());
                        let file = fs_err::tokio::File::open(&target_cache_file).await?;
                        let size = file.metadata().await?.len();
                        let reader = Box::new(tokio::io::BufReader::new(file));
                        (reader, Some(size))
                    }
                    Err(err) => return Err(err.into()),
                };

            // Extract the downloaded archive into a temporary directory.
            self.extract_reader(
                reader,
                temp_dir,
                &filename,
                ext,
                size,
                reporter,
                Direction::Extract,
            )
            .await?
        } else {
            // Avoid overlong log lines
            debug!("Downloading `{url}`");
            debug!(
                "Extracting `{filename}` to temporary location `{}`",
                temp_dir.path().simplified_display()
            );

            let (reader, size) = read_url(&url, client).await?;
            self.extract_reader(
                reader,
                temp_dir,
                &filename,
                ext,
                size,
                reporter,
                Direction::Download,
            )
            .await?
        };

        // Extract the top-level directory.
        let mut extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.path().to_path_buf(),
            Err(err) => return Err(Error::ExtractError(filename, err)),
        };

        // If the distribution is a `full` archive, the Python installation is in the `install` directory.
        if extracted.join("install").is_dir() {
            extracted = extracted.join("install");
        // If the distribution is a Pyodide archive, the Python installation is in the `pyodide-root/dist` directory.
        } else if self.os().is_emscripten() {
            extracted = extracted.join("pyodide-root").join("dist");
        }

        #[cfg(unix)]
        {
            // Pyodide distributions require all of the supporting files to be alongside the Python
            // executable, so they don't have a `bin` directory. We create it and link
            // `bin/pythonX.Y` to `dist/python`.
            if self.os().is_emscripten() {
                fs_err::create_dir_all(extracted.join("bin"))?;
                fs_err::os::unix::fs::symlink(
                    "../python",
                    extracted
                        .join("bin")
                        .join(format!("python{}.{}", self.key.major, self.key.minor)),
                )?;
            }

            // If the distribution is missing a `python` -> `pythonX.Y` symlink, add it.
            //
            // We skip for Windows distributions, allowing cross-installs from Unix.
            //
            // Pyodide releases never contain this link by default.
            //
            // PEP 394 permits it, and python-build-standalone releases after `20240726` include it,
            // but releases prior to that date do not.
            if !self.os().is_windows() {
                match fs_err::os::unix::fs::symlink(
                    format!("python{}.{}", self.key.major, self.key.minor),
                    extracted.join("bin").join("python"),
                ) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }

        // Remove the target if it already exists.
        if path.is_dir() {
            debug!("Removing existing directory: {}", path.user_display());
            fs_err::tokio::remove_dir_all(&path).await?;
        }

        // Persist it to the target.
        debug!(
            "Moving `{}` to `{}`",
            extracted.display(),
            path.user_display()
        );
        rename_with_retry(extracted, &path)
            .await
            .map_err(|err| Error::CopyError {
                to: path.clone(),
                err,
            })?;

        Ok(DownloadResult::Fetched(path))
    }

    /// Download the managed Python archive into the cache directory.
    async fn download_archive(
        &self,
        url: &DisplaySafeUrl,
        client: &BaseClient,
        reporter: Option<&dyn Reporter>,
        python_builds_dir: &Path,
        target_cache_file: &Path,
    ) -> Result<(), Error> {
        debug!(
            "Downloading `{}` to `{}`",
            url,
            target_cache_file.simplified_display()
        );

        let (mut reader, size) = read_url(url, client).await?;
        let temp_dir = tempfile::tempdir_in(python_builds_dir)?;
        let temp_file = temp_dir.path().join("download");

        // Download to a temporary file. We verify the hash when unpacking the file.
        {
            let mut archive_writer = BufWriter::new(fs_err::tokio::File::create(&temp_file).await?);

            // Download with or without progress bar.
            if let Some(reporter) = reporter {
                let key = reporter.on_request_start(Direction::Download, &self.key, size);
                tokio::io::copy(
                    &mut ProgressReader::new(reader, key, reporter),
                    &mut archive_writer,
                )
                .await?;
                reporter.on_request_complete(Direction::Download, key);
            } else {
                tokio::io::copy(&mut reader, &mut archive_writer).await?;
            }

            archive_writer.flush().await?;
        }
        // Move the completed file into place, invalidating the `File` instance.
        match rename_with_retry(&temp_file, target_cache_file).await {
            Ok(()) => {}
            Err(_) if target_cache_file.is_file() => {}
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }

    /// Extract a Python interpreter archive into a (temporary) directory, either from a file or
    /// from a download stream.
    async fn extract_reader(
        &self,
        reader: impl AsyncRead + Unpin,
        target: TempDir,
        filename: &String,
        ext: SourceDistExtension,
        size: Option<u64>,
        reporter: Option<&dyn Reporter>,
        direction: Direction,
    ) -> Result<TempDir, Error> {
        let mut hashers = self
            .sha256
            .as_ref()
            .map(|_| Hasher::from(HashAlgorithm::Sha256));
        let mut hasher = uv_extract::hash::HashReader::new(reader, hashers.as_mut_slice());

        let target = if let Some(reporter) = reporter {
            let progress_key = reporter.on_request_start(direction, &self.key, size);
            let mut reader = ProgressReader::new(&mut hasher, progress_key, reporter);
            let (target, _) = uv_extract::stream::archive(&mut reader, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_owned(), err))?;
            reporter.on_request_complete(direction, progress_key);
            target
        } else {
            let (target, _) = uv_extract::stream::archive(&mut hasher, ext, target)
                .await
                .map_err(|err| Error::ExtractError(filename.to_owned(), err))?;
            target
        };
        hasher.finish().await.map_err(Error::HashExhaustion)?;

        // Check the hash
        if let Some((expected, hasher)) = self.sha256.as_ref().zip(hashers) {
            let actual = HashDigest::from(hasher);
            if actual.digest() != expected.as_str() {
                return Err(Error::HashMismatch {
                    installation: self.key.to_string(),
                    expected: expected.as_str().to_string(),
                    actual: actual.digest().to_string(),
                });
            }
        }

        Ok(target)
    }

    #[cfg(test)]
    fn python_version(&self) -> PythonVersion {
        self.key.version()
    }

    /// Return the ordered list of [`Url`]s to try when downloading the distribution.
    ///
    /// For CPython without a user-configured mirror, the default Astral mirror is listed first,
    /// followed by the canonical GitHub URL as a fallback.
    ///
    /// For all other cases (user mirror explicitly set, PyPy, GraalPy, Pyodide), a single URL
    /// is returned with no fallback.
    pub fn download_urls(
        &self,
        mirrors: PythonDownloadMirrors<'_>,
    ) -> Result<Vec<DisplaySafeUrl>, Error> {
        let custom_astral_mirror = astral_mirror_url_from_env();
        self.download_urls_with_astral_mirror(mirrors, custom_astral_mirror.as_deref())
    }

    fn download_urls_with_astral_mirror(
        &self,
        mirrors: PythonDownloadMirrors<'_>,
        astral_mirror_url: Option<&str>,
    ) -> Result<Vec<DisplaySafeUrl>, Error> {
        let astral_mirror_url = custom_astral_mirror_url(astral_mirror_url);
        match self.key.implementation().as_ref() {
            LenientImplementationName::Known(ImplementationName::CPython) => {
                if let Some(mirror) = mirrors.cpython {
                    // User-configured mirror: use it exclusively, no automatic fallback.
                    let Some(suffix) = self.url.strip_prefix(CPYTHON_DOWNLOADS_URL_PREFIX) else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYTHON_INSTALL_MIRROR,
                            DisplaySafeUrl::parse(&self.url)?,
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
                // No user mirror: try the default/custom Astral mirror first.
                if let Some(suffix) = self.url.strip_prefix(CPYTHON_DOWNLOADS_URL_PREFIX) {
                    let effective_mirror = effective_cpython_mirror(astral_mirror_url);
                    let mirror_url = DisplaySafeUrl::parse(
                        format!("{}/{}", effective_mirror.trim_end_matches('/'), suffix).as_str(),
                    )?;
                    // When a custom Astral mirror is set, use it exclusively.
                    if astral_mirror_url.is_some() {
                        return Ok(vec![mirror_url]);
                    }
                    // Otherwise fall back to the canonical GitHub URL.
                    let canonical_url = DisplaySafeUrl::parse(&self.url)?;
                    return Ok(vec![mirror_url, canonical_url]);
                }
            }

            LenientImplementationName::Known(ImplementationName::PyPy) => {
                if let Some(mirror) = mirrors.pypy {
                    let Some(suffix) = self.url.strip_prefix("https://downloads.python.org/pypy/")
                    else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYPY_INSTALL_MIRROR,
                            DisplaySafeUrl::parse(&self.url)?,
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
            }

            LenientImplementationName::Known(ImplementationName::GraalPy) => {
                if let Some(mirror) = mirrors.graalpy {
                    let Some(suffix) = self
                        .url
                        .strip_prefix("https://github.com/oracle/graalpython/releases/download/")
                    else {
                        return Err(Error::Mirror(
                            EnvVars::UV_GRAALPY_INSTALL_MIRROR,
                            DisplaySafeUrl::parse(&self.url)?,
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
            }

            LenientImplementationName::Known(ImplementationName::Pyodide) => {
                if let Some(mirror) = mirrors.pyodide {
                    let Some(suffix) = self
                        .url
                        .strip_prefix("https://github.com/pyodide/pyodide/releases/download/")
                    else {
                        return Err(Error::Mirror(
                            EnvVars::UV_PYODIDE_INSTALL_MIRROR,
                            DisplaySafeUrl::parse(&self.url)?,
                        ));
                    };
                    return Ok(vec![DisplaySafeUrl::parse(
                        format!("{}/{}", mirror.trim_end_matches('/'), suffix).as_str(),
                    )?]);
                }
            }

            _ => {}
        }

        Ok(vec![DisplaySafeUrl::parse(&self.url)?])
    }

    /// Whether this download satisfies the requested Python and build.
    fn matches_request(&self, request: &PythonDownloadRequest) -> bool {
        // First check the key
        if !request.satisfied_by_key(self.key()) {
            return false;
        }

        // Then check the build if specified
        if let Some(ref requested_build) = request.build {
            let Some(download_build) = self.build() else {
                debug!(
                    "Skipping download `{}`: a build version was requested but is not available for this download",
                    self
                );
                return false;
            };

            if download_build != requested_build {
                debug!(
                    "Skipping download `{}`: requested build version `{}` does not match download build version `{}`",
                    self, requested_build, download_build
                );
                return false;
            }
        }

        true
    }
}

fn parse_json_downloads(
    json_downloads: HashMap<String, JsonPythonDownload>,
) -> Vec<ManagedPythonDownload> {
    json_downloads
        .into_iter()
        .filter_map(|(key, entry)| {
            let implementation = match entry.name.as_str() {
                "cpython" => LenientImplementationName::Known(ImplementationName::CPython),
                "pypy" => LenientImplementationName::Known(ImplementationName::PyPy),
                "graalpy" => LenientImplementationName::Known(ImplementationName::GraalPy),
                _ => LenientImplementationName::Unknown(entry.name.clone()),
            };

            let arch_str = match entry.arch.family.as_str() {
                "armv5tel" => Cow::Borrowed("armv5te"),
                // The `gc` variant of riscv64 is the common base instruction set and
                // is the target in `python-build-standalone`
                // See https://github.com/astral-sh/python-build-standalone/issues/504
                "riscv64" => Cow::Borrowed("riscv64gc"),
                value => Cow::Borrowed(value),
            };

            let arch_str = if let Some(variant) = entry.arch.variant {
                Cow::Owned(format!("{arch_str}_{variant}"))
            } else {
                arch_str
            };

            let arch = match Arch::from_str(&arch_str) {
                Ok(arch) => arch,
                Err(e) => {
                    debug!("Skipping entry {key}: Invalid arch '{arch_str}' - {e}");
                    return None;
                }
            };

            let os = match Os::from_str(&entry.os) {
                Ok(os) => os,
                Err(e) => {
                    debug!("Skipping entry {}: Invalid OS '{}' - {}", key, entry.os, e);
                    return None;
                }
            };

            let libc = match Libc::from_str(&entry.libc) {
                Ok(libc) => libc,
                Err(e) => {
                    debug!(
                        "Skipping entry {}: Invalid libc '{}' - {}",
                        key, entry.libc, e
                    );
                    return None;
                }
            };

            let variant = match entry
                .variant
                .as_deref()
                .map(PythonVariant::from_str)
                .transpose()
            {
                Ok(Some(variant)) => variant,
                Ok(None) => PythonVariant::default(),
                Err(()) => {
                    debug!(
                        "Skipping entry {key}: Unknown python variant - {}",
                        entry.variant.unwrap_or_default()
                    );
                    return None;
                }
            };

            let version_str = format!(
                "{}.{}.{}{}",
                entry.major,
                entry.minor,
                entry.patch,
                entry.prerelease.as_deref().unwrap_or_default()
            );

            let version = match PythonVersion::from_str(&version_str) {
                Ok(version) => version,
                Err(e) => {
                    debug!("Skipping entry {key}: Invalid version '{version_str}' - {e}");
                    return None;
                }
            };

            let url = Cow::Owned(entry.url);
            let sha256 = entry.sha256;
            let build = entry
                .build
                .map(|s| Box::leak(s.into_boxed_str()) as &'static str);

            Some(ManagedPythonDownload {
                key: PythonInstallationKey::new_from_version(
                    implementation,
                    &version,
                    Platform::new(os, arch, libc),
                    variant,
                ),
                url,
                sha256,
                build,
            })
        })
        .sorted_by(|a, b| Ord::cmp(&b.key, &a.key))
        .collect()
}

impl Error {
    fn from_reqwest(
        url: DisplaySafeUrl,
        err: reqwest::Error,
        retries: Option<u32>,
        start: Instant,
    ) -> Self {
        let err = Self::NetworkError(url, WrappedReqwestError::from(err));
        if let Some(retries) = retries {
            Self::NetworkErrorWithRetries {
                err: Box::new(err),
                retries,
                duration: start.elapsed(),
            }
        } else {
            err
        }
    }

    fn from_reqwest_middleware(url: DisplaySafeUrl, err: reqwest_middleware::Error) -> Self {
        match err {
            reqwest_middleware::Error::Middleware(error) => {
                Self::NetworkMiddlewareError(url, error)
            }
            reqwest_middleware::Error::Reqwest(error) => {
                Self::NetworkError(url, WrappedReqwestError::from(error))
            }
        }
    }
}

impl Display for ManagedPythonDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Download,
    Extract,
}

impl Direction {
    fn as_str(&self) -> &str {
        match self {
            Self::Download => "download",
            Self::Extract => "extract",
        }
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub trait Reporter: Send + Sync {
    fn on_request_start(
        &self,
        direction: Direction,
        name: &PythonInstallationKey,
        size: Option<u64>,
    ) -> usize;
    fn on_request_progress(&self, id: usize, inc: u64);
    fn on_request_complete(&self, direction: Direction, id: usize);
}

/// An asynchronous reader that reports progress as bytes are read.
struct ProgressReader<'a, R> {
    reader: R,
    index: usize,
    reporter: &'a dyn Reporter,
}

impl<'a, R> ProgressReader<'a, R> {
    /// Create a new [`ProgressReader`] that wraps another reader.
    fn new(reader: R, index: usize, reporter: &'a dyn Reporter) -> Self {
        Self {
            reader,
            index,
            reporter,
        }
    }
}

impl<R> AsyncRead for ProgressReader<'_, R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.as_mut().reader)
            .poll_read(cx, buf)
            .map_ok(|()| {
                self.reporter
                    .on_request_progress(self.index, buf.filled().len() as u64);
            })
    }
}

/// Convert a [`Url`] into an [`AsyncRead`] stream.
async fn read_url(
    url: &DisplaySafeUrl,
    client: &BaseClient,
) -> Result<(impl AsyncRead + Unpin, Option<u64>), Error> {
    if url.scheme() == "file" {
        // Loads downloaded distribution from the given `file://` URL.
        let path = url
            .to_file_path()
            .map_err(|()| Error::InvalidFileUrl(url.to_string()))?;

        let size = fs_err::tokio::metadata(&path).await?.len();
        let reader = fs_err::tokio::File::open(&path).await?;

        Ok((Either::Left(reader), Some(size)))
    } else {
        let start = Instant::now();
        let response = client
            .for_host(url)
            .get(Url::from(url.clone()))
            .send()
            .await
            .map_err(|err| Error::from_reqwest_middleware(url.clone(), err))?;

        let retry_count = response
            .extensions()
            .get::<reqwest_retry::RetryCount>()
            .map(|retries| retries.value());

        // Check the status code.
        let response = response
            .error_for_status()
            .map_err(|err| Error::from_reqwest(url.clone(), err, retry_count, start))?;

        let size = response.content_length();
        let stream = response
            .bytes_stream()
            .map_err(io::Error::other)
            .into_async_read();

        Ok((Either::Right(stream.compat()), size))
    }
}

#[cfg(test)]
mod tests {
    use uv_python_types::VersionRequest;

    use std::collections::HashSet;

    use uv_platform::{Arch, Libc, Os, Platform};
    use uv_python_types::{LenientImplementationName, PythonInstallationKey};

    use super::*;

    #[test]
    fn test_download_error_debug() {
        let errors = [
            Error::Request(PythonDownloadRequestError::EmptyRequest),
            Error::Mirror(
                "UV_PYTHON_INSTALL_MIRROR",
                DisplaySafeUrl::parse("file:///mirror").expect("mirror URL should be valid"),
            ),
            Error::NetworkErrorWithRetries {
                err: Box::new(Error::Request(
                    PythonDownloadRequestError::InvalidPythonVersion("3.x".to_owned()),
                )),
                retries: 2,
                duration: Duration::from_secs(3),
            },
            Error::HashMismatch {
                installation: "cpython-3.12.0-linux-x86_64-gnu".to_owned(),
                expected: "abc".to_owned(),
                actual: "def".to_owned(),
            },
        ];

        insta::assert_debug_snapshot!(errors, @r#"
        [
            Request(
                EmptyRequest,
            ),
            Mirror(
                "UV_PYTHON_INSTALL_MIRROR",
                DisplaySafeUrl {
                    scheme: "file",
                    cannot_be_a_base: false,
                    username: "",
                    password: None,
                    host: None,
                    port: None,
                    path: "/mirror",
                    query: None,
                    fragment: None,
                },
            ),
            NetworkErrorWithRetries {
                err: Request(
                    InvalidPythonVersion(
                        "3.x",
                    ),
                ),
                retries: 2,
                duration: 3s,
            },
            HashMismatch {
                installation: "cpython-3.12.0-linux-x86_64-gnu",
                expected: "abc",
                actual: "def",
            },
        ]
        "#);
    }

    /// Test that build filtering works correctly
    #[tokio::test]
    async fn test_python_download_request_build_filtering() {
        let mut request = PythonDownloadRequest::default()
            .with_version(VersionRequest::from_str("3.12").unwrap())
            .with_implementation(ImplementationName::CPython);
        request.build = Some("20240814".to_string());

        let client_builder = uv_client::BaseClientBuilder::default();
        let cache = uv_cache::Cache::temp().expect("failed to create temp cache");
        let download_list = ManagedPythonDownloadList::new(&client_builder, &cache, None)
            .await
            .unwrap();

        let downloads: Vec<_> = download_list
            .iter_all()
            .filter(|d| d.matches_request(&request))
            .collect();

        assert!(
            !downloads.is_empty(),
            "Should find at least one matching download"
        );
        for download in downloads {
            assert_eq!(download.build(), Some("20240814"));
        }
    }

    /// Test that an invalid build results in no matches
    #[tokio::test]
    async fn test_python_download_request_invalid_build() {
        // Create a request with a non-existent build
        let mut request = PythonDownloadRequest::default()
            .with_version(VersionRequest::from_str("3.12").unwrap())
            .with_implementation(ImplementationName::CPython);
        request.build = Some("99999999".to_string());

        let client_builder = uv_client::BaseClientBuilder::default();
        let cache = uv_cache::Cache::temp().expect("failed to create temp cache");
        let download_list = ManagedPythonDownloadList::new(&client_builder, &cache, None)
            .await
            .unwrap();

        // Should find no matching downloads
        let downloads: Vec<_> = download_list
            .iter_all()
            .filter(|d| d.matches_request(&request))
            .collect();

        assert_eq!(downloads.len(), 0);
    }

    fn cpython_download_for_url(url: &'static str) -> ManagedPythonDownload {
        let key = PythonInstallationKey::new(
            LenientImplementationName::Known(uv_python_types::ImplementationName::CPython),
            3,
            12,
            4,
            None,
            Platform::new(
                Os::from_str("linux").unwrap(),
                Arch::from_str("x86_64").unwrap(),
                Libc::from_str("gnu").unwrap(),
            ),
            uv_python_types::PythonVariant::default(),
        );

        ManagedPythonDownload {
            key,
            url: Cow::Borrowed(url),
            sha256: Some(Digest::from_bytes([0xab; 32])),
            build: Some("20240713"),
        }
    }

    #[test]
    fn test_cpython_mirror_unexpected_download_url_redacts_credentials() {
        let download = cpython_download_for_url(
            "https://user:password@example.com/cpython.tar.gz?X-Amz-Signature=signature&safe=value",
        );

        let error = download
            .download_urls_with_astral_mirror(
                PythonDownloadMirrors {
                    cpython: Some("https://python-mirror.example.com/releases/"),
                    ..PythonDownloadMirrors::default()
                },
                None,
            )
            .expect_err("the download URL should not match the CPython mirror format");

        insta::assert_snapshot!(error, @"A mirror was provided via `UV_PYTHON_INSTALL_MIRROR`, but the URL does not match the expected format: https://user:****@example.com/cpython.tar.gz?X-Amz-Signature=****&safe=value");
    }

    #[test]
    fn test_cpython_mirror_invalid_download_url() {
        let download = cpython_download_for_url("not a URL");

        let error = download
            .download_urls_with_astral_mirror(
                PythonDownloadMirrors {
                    cpython: Some("https://python-mirror.example.com/releases/"),
                    ..PythonDownloadMirrors::default()
                },
                None,
            )
            .expect_err("the download URL should be invalid");

        insta::assert_snapshot!(error, @"Invalid download URL");
    }

    #[test]
    fn test_cpython_download_urls_custom_astral_mirror() {
        let download = cpython_download_for_url(
            "https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-x86_64-unknown-linux-gnu-install_only.tar.gz",
        );

        let urls = download
            .download_urls_with_astral_mirror(
                PythonDownloadMirrors::default(),
                Some("https://nexus.example.com/repository/releases.astral.sh/"),
            )
            .expect("download URLs should be valid");
        let urls = urls
            .into_iter()
            .map(|url| url.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![
                "https://nexus.example.com/repository/releases.astral.sh/github/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-x86_64-unknown-linux-gnu-install_only.tar.gz"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn test_cpython_specific_mirror_takes_precedence_over_astral_mirror() {
        let download = cpython_download_for_url(
            "https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-x86_64-unknown-linux-gnu-install_only.tar.gz",
        );

        let urls = download
            .download_urls_with_astral_mirror(
                PythonDownloadMirrors {
                    cpython: Some("https://python-mirror.example.com/releases/"),
                    ..PythonDownloadMirrors::default()
                },
                Some("https://nexus.example.com/repository/releases.astral.sh/"),
            )
            .expect("download URLs should be valid");
        let urls = urls
            .into_iter()
            .map(|url| url.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![
                "https://python-mirror.example.com/releases/20240713/cpython-3.12.4%2B20240713-x86_64-unknown-linux-gnu-install_only.tar.gz"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn test_cpython_download_urls_empty_astral_mirror_uses_default() {
        let download = cpython_download_for_url(
            "https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-x86_64-unknown-linux-gnu-install_only.tar.gz",
        );

        let default_urls = download
            .download_urls_with_astral_mirror(PythonDownloadMirrors::default(), None)
            .expect("download URLs should be valid");
        let empty_urls = download
            .download_urls_with_astral_mirror(PythonDownloadMirrors::default(), Some(""))
            .expect("download URLs should be valid");

        assert_eq!(default_urls, empty_urls);
    }

    /// A hash mismatch is a post-download integrity failure — retrying a different URL cannot fix
    /// it, so it should not trigger a fallback.
    #[test]
    fn test_should_try_next_url_hash_mismatch() {
        let err = Error::HashMismatch {
            installation: "cpython-3.12.0".to_string(),
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(!err.should_try_next_url());
    }

    /// A local filesystem error during extraction (e.g. permission denied writing to disk) is not
    /// a network failure — a different URL would produce the same outcome.
    #[test]
    fn test_should_try_next_url_extract_error_filesystem() {
        let err = Error::ExtractError(
            "archive.tar.gz".to_string(),
            uv_extract::Error::Io(io::Error::new(io::ErrorKind::PermissionDenied, "")),
        );
        assert!(!err.should_try_next_url());
    }

    /// A generic IO error from a local filesystem operation (e.g. permission denied on cache
    /// directory) should not trigger a fallback to a different URL.
    #[test]
    fn test_should_try_next_url_io_error_filesystem() {
        let err = Error::Io(io::Error::new(io::ErrorKind::PermissionDenied, ""));
        assert!(!err.should_try_next_url());
    }

    /// A network IO error (e.g. connection reset mid-download) surfaces as `Error::Io` from
    /// `download_archive`. It should trigger a fallback because a different mirror may succeed.
    #[test]
    fn test_should_try_next_url_io_error_network() {
        let err = Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, ""));
        assert!(err.should_try_next_url());
    }

    /// A 404 HTTP response from the mirror becomes `Error::NetworkError` — it should trigger a
    /// URL fallback, because a 404 on the mirror does not mean the file is absent from GitHub.
    #[test]
    fn test_should_try_next_url_network_error_404() {
        let url =
            DisplaySafeUrl::from_str("https://releases.astral.sh/python/cpython-3.12.0.tar.gz")
                .unwrap();
        // `NetworkError` wraps a `WrappedReqwestError`; we use a middleware error as a
        // stand-in because `should_try_next_url` only inspects the variant, not the contents.
        let wrapped = WrappedReqwestError::with_problem_details(
            reqwest_middleware::Error::Middleware(anyhow::anyhow!("404 Not Found")),
            None,
        );
        let err = Error::NetworkError(url, wrapped);
        assert!(err.should_try_next_url());
    }

    /// Every [`PythonVersion`] in the embedded download metadata must be convertible
    /// to a [`VersionRequest`] to avoid runtime panics.
    #[test]
    fn embedded_download_versions_convert_to_version_requests() {
        let downloads = ManagedPythonDownloadList::new_only_embedded()
            .expect("embedded download metadata should load");

        let unique_versions: HashSet<PythonVersion> = downloads
            .iter_all()
            .map(ManagedPythonDownload::python_version)
            .collect();

        for version in &unique_versions {
            let _ = VersionRequest::from(version);
        }
    }
}
