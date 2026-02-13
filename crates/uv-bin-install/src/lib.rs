//! Binary download and installation utilities for uv.
//!
//! These utilities are specifically for consuming distributions that are _not_ Python packages,
//! e.g., `ruff` (which does have a Python package, but also has standalone binaries on GitHub).

use std::fmt;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use futures::{StreamExt, TryStreamExt};
use reqwest_retry::policies::ExponentialBackoff;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;
use uv_distribution_filename::SourceDistExtension;

use uv_cache::{Cache, CacheBucket, CacheEntry, Error as CacheError};
use uv_client::{BaseClient, RetryState};
use uv_extract::{Error as ExtractError, stream};
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_platform::Platform;
use uv_redacted::DisplaySafeUrl;

/// Binary tools that can be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binary {
    Ruff,
}

impl Binary {
    /// Get the default version constraints for this binary.
    ///
    /// Returns a version range constraint (e.g., `>=0.15,<0.16`) rather than a pinned version,
    /// allowing patch version updates without requiring a uv release.
    pub fn default_constraints(&self) -> VersionSpecifiers {
        match self {
            // TODO(zanieb): Figure out a nice way to automate updating this
            Self::Ruff => [
                VersionSpecifier::greater_than_equal_version(Version::new([0, 15])),
                VersionSpecifier::less_than_version(Version::new([0, 16])),
            ]
            .into_iter()
            .collect(),
        }
    }

    /// The name of the binary.
    ///
    /// See [`Binary::executable`] for the platform-specific executable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ruff => "ruff",
        }
    }

    /// Get the download URL for a specific version and platform.
    pub fn download_url(
        &self,
        version: &Version,
        platform: &str,
        format: ArchiveFormat,
    ) -> Result<Url, Error> {
        match self {
            Self::Ruff => {
                let url = format!(
                    "https://github.com/astral-sh/ruff/releases/download/{version}/ruff-{platform}.{}",
                    format.extension()
                );
                Url::parse(&url).map_err(|err| Error::UrlParse { url, source: err })
            }
        }
    }

    /// Get the executable name
    pub fn executable(&self) -> String {
        format!("{}{}", self.name(), std::env::consts::EXE_SUFFIX)
    }
}

impl fmt::Display for Binary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Archive formats for binary downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    /// Get the file extension for this archive format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

impl From<ArchiveFormat> for SourceDistExtension {
    fn from(val: ArchiveFormat) -> Self {
        match val {
            ArchiveFormat::Zip => Self::Zip,
            ArchiveFormat::TarGz => Self::TarGz,
        }
    }
}

/// Specifies which version of a binary to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinVersion {
    /// Use the binary's default pinned version.
    Default,
    /// Fetch the latest version from the manifest.
    Latest,
    /// Use a specific pinned version.
    Pinned(Version),
    /// Find the best version matching the given constraints.
    Constraint(uv_pep440::VersionSpecifiers),
}

impl FromStr for BinVersion {
    type Err = uv_pep440::VersionSpecifiersParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("latest") {
            return Ok(Self::Latest);
        }
        // Try parsing as an exact version first
        if let Ok(version) = Version::from_str(s) {
            return Ok(Self::Pinned(version));
        }
        // Otherwise parse as version specifiers
        let specifiers = uv_pep440::VersionSpecifiers::from_str(s)?;
        Ok(Self::Constraint(specifiers))
    }
}

impl fmt::Display for BinVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("default"),
            Self::Latest => f.write_str("latest"),
            Self::Pinned(version) => write!(f, "{version}"),
            Self::Constraint(specifiers) => write!(f, "{specifiers}"),
        }
    }
}

/// Base URL for the versions manifest.
const VERSIONS_MANIFEST_URL: &str = "https://raw.githubusercontent.com/astral-sh/versions/main/v1";

/// Binary version information from the versions manifest.
#[derive(Debug, Deserialize)]
struct BinVersionInfo {
    #[serde(deserialize_with = "deserialize_version")]
    version: Version,
    date: jiff::Timestamp,
    artifacts: Vec<BinArtifact>,
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<Version, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Version::from_str(&s).map_err(serde::de::Error::custom)
}

/// Binary artifact information.
#[derive(Debug, Deserialize)]
struct BinArtifact {
    platform: String,
    url: String,
    archive_format: String,
}

/// A resolved version with its artifact information.
#[derive(Debug)]
pub struct ResolvedVersion {
    /// The version number.
    pub version: Version,
    /// The download URL for this version and current platform.
    pub artifact_url: Url,
    /// The archive format.
    pub archive_format: ArchiveFormat,
}

impl ResolvedVersion {
    /// Construct a [`ResolvedVersion`] from a [`Binary`] and a [`Version`] by inferring the
    /// download URL and archive format from the current platform.
    pub fn from_version(binary: Binary, version: Version) -> Result<Self, Error> {
        let platform = Platform::from_env()?;
        let platform_name = platform.as_cargo_dist_triple();
        let archive_format = if platform.os.is_windows() {
            ArchiveFormat::Zip
        } else {
            ArchiveFormat::TarGz
        };
        let artifact_url = binary.download_url(&version, &platform_name, archive_format)?;
        Ok(Self {
            version,
            artifact_url,
            archive_format,
        })
    }
}

/// Errors that can occur during binary download and installation.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to download from: {url}")]
    Download {
        url: DisplaySafeUrl,
        #[source]
        source: reqwest_middleware::Error,
    },

    #[error("Failed to parse URL: {url}")]
    UrlParse {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error("Failed to extract archive")]
    Extract {
        #[source]
        source: ExtractError,
    },

    #[error("Binary not found in archive at expected location: {expected}")]
    BinaryNotFound { expected: PathBuf },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Cache(#[from] CacheError),

    #[error("Failed to detect platform")]
    Platform(#[from] uv_platform::Error),

    #[error("Request failed after {retries} {subject}", subject = if *retries > 1 { "retries" } else { "retry" })]
    RetriedError {
        #[source]
        err: Box<Self>,
        retries: u32,
    },

    #[error("Failed to fetch version manifest from: {url}")]
    ManifestFetch {
        url: String,
        #[source]
        source: reqwest_middleware::Error,
    },

    #[error("Failed to parse version manifest")]
    ManifestParse(#[from] serde_json::Error),

    #[error("Invalid UTF-8 in version manifest")]
    ManifestUtf8(#[from] std::str::Utf8Error),

    #[error("No version of {binary} found matching `{constraints}` for platform `{platform}`")]
    NoMatchingVersion {
        binary: Binary,
        constraints: uv_pep440::VersionSpecifiers,
        platform: String,
    },

    #[error("No version of {binary} found for platform `{platform}`")]
    NoVersionForPlatform { binary: Binary, platform: String },

    #[error("No artifact found for {binary} {version} on platform {platform}")]
    NoArtifactForPlatform {
        binary: Binary,
        version: String,
        platform: String,
    },

    #[error("Unsupported archive format: {0}")]
    UnsupportedArchiveFormat(String),
}

impl Error {
    /// Return the number of retries that were made to complete this request before this error was
    /// returned.
    ///
    /// Note that e.g. 3 retries equates to 4 attempts.
    fn retries(&self) -> u32 {
        if let Self::RetriedError { retries, .. } = self {
            return *retries;
        }
        0
    }
}

/// Find a version of a binary that matches the given constraints.
///
/// This streams the NDJSON manifest line-by-line, returning the first version
/// that matches the constraints (versions are sorted newest-first).
///
/// If no constraints are provided, returns the latest version.
///
/// If `exclude_newer` is provided, versions with a release date newer than the
/// given timestamp will be skipped.
pub async fn find_matching_version(
    binary: Binary,
    constraints: Option<&uv_pep440::VersionSpecifiers>,
    exclude_newer: Option<jiff::Timestamp>,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
) -> Result<ResolvedVersion, Error> {
    let platform = Platform::from_env()?;
    let platform_name = platform.as_cargo_dist_triple();

    let manifest_url = format!("{}/{}.ndjson", VERSIONS_MANIFEST_URL, binary.name());
    let manifest_url_parsed = Url::parse(&manifest_url).map_err(|source| Error::UrlParse {
        url: manifest_url.clone(),
        source,
    })?;

    let mut retry_state = RetryState::start(*retry_policy, manifest_url_parsed.clone());

    loop {
        let result = fetch_and_find_matching_version(
            binary,
            constraints,
            exclude_newer,
            &platform_name,
            &manifest_url,
            &manifest_url_parsed,
            client,
        )
        .await;

        match result {
            Ok(resolved) => return Ok(resolved),
            Err(err) => {
                if let Some(backoff) = retry_state.should_retry(&err, err.retries()) {
                    retry_state.sleep_backoff(backoff).await;
                    continue;
                }
                return if retry_state.total_retries() > 0 {
                    Err(Error::RetriedError {
                        err: Box::new(err),
                        retries: retry_state.total_retries(),
                    })
                } else {
                    Err(err)
                };
            }
        }
    }
}

/// Inner function that fetches the manifest and finds a matching version.
///
/// This is separated from [`find_matching_version`] to allow retry logic to wrap
/// the entire streaming operation.
async fn fetch_and_find_matching_version(
    binary: Binary,
    constraints: Option<&uv_pep440::VersionSpecifiers>,
    exclude_newer: Option<jiff::Timestamp>,
    platform_name: &str,
    manifest_url: &str,
    manifest_url_parsed: &Url,
    client: &BaseClient,
) -> Result<ResolvedVersion, Error> {
    let response = client
        .for_host(&DisplaySafeUrl::from_url(manifest_url_parsed.clone()))
        .get(manifest_url_parsed.clone())
        .send()
        .await
        .map_err(|source| Error::ManifestFetch {
            url: manifest_url.to_string(),
            source,
        })?;

    let response = response
        .error_for_status()
        .map_err(|err| Error::ManifestFetch {
            url: manifest_url.to_string(),
            source: reqwest_middleware::Error::Reqwest(err),
        })?;

    // Parse a single JSON line and check if it matches the constraints and platform.
    let parse_and_check = |line: &[u8]| -> Result<Option<ResolvedVersion>, Error> {
        let line_str = std::str::from_utf8(line)?;
        if line_str.trim().is_empty() {
            return Ok(None);
        }
        let version_info: BinVersionInfo = serde_json::from_str(line_str)?;
        Ok(check_version_match(
            &version_info,
            constraints,
            exclude_newer,
            platform_name,
        ))
    };

    // Stream the response line by line
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| Error::ManifestFetch {
            url: manifest_url.to_string(),
            source: reqwest_middleware::Error::Reqwest(err),
        })?;
        buffer.extend_from_slice(&chunk);

        // Process complete lines
        while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
            let line = &buffer[..newline_pos];
            let result = parse_and_check(line)?;
            buffer.drain(..=newline_pos);

            if let Some(resolved) = result {
                return Ok(resolved);
            }
        }
    }

    // Process any remaining data in buffer (in case there's no trailing newline)
    if let Some(resolved) = parse_and_check(&buffer)? {
        return Ok(resolved);
    }

    // No matching version found
    match constraints {
        Some(constraints) => Err(Error::NoMatchingVersion {
            binary,
            constraints: constraints.clone(),
            platform: platform_name.to_string(),
        }),
        None => Err(Error::NoVersionForPlatform {
            binary,
            platform: platform_name.to_string(),
        }),
    }
}

/// Check if a version matches the constraints and find the artifact for the platform.
///
/// Returns `Some(resolved)` if the version matches and an artifact is found,
/// `None` if the version doesn't match or no artifact is available for the platform.
fn check_version_match(
    version_info: &BinVersionInfo,
    constraints: Option<&uv_pep440::VersionSpecifiers>,
    exclude_newer: Option<jiff::Timestamp>,
    platform_name: &str,
) -> Option<ResolvedVersion> {
    // Skip versions newer than the exclude_newer cutoff
    if let Some(cutoff) = exclude_newer
        && version_info.date > cutoff
    {
        return None;
    }

    // Skip versions that don't match the constraints
    if let Some(constraints) = constraints
        && !constraints.contains(&version_info.version)
    {
        return None;
    }

    // Find an artifact matching the platform, trusting whichever archive format the
    // manifest reports.
    for artifact in &version_info.artifacts {
        if artifact.platform != platform_name {
            continue;
        }

        let Ok(artifact_url) = Url::parse(&artifact.url) else {
            continue;
        };

        let archive_format = match artifact.archive_format.as_str() {
            "tar.gz" => ArchiveFormat::TarGz,
            "zip" => ArchiveFormat::Zip,
            _ => continue,
        };

        return Some(ResolvedVersion {
            version: version_info.version.clone(),
            artifact_url,
            archive_format,
        });
    }

    None
}

/// Install the given binary from a [`ResolvedVersion`].
pub async fn bin_install(
    binary: Binary,
    resolved: &ResolvedVersion,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
    cache: &Cache,
    reporter: &dyn Reporter,
) -> Result<PathBuf, Error> {
    let platform = Platform::from_env()?;
    let platform_name = platform.as_cargo_dist_triple();

    bin_install_from_url(
        binary,
        &resolved.version,
        &resolved.artifact_url,
        resolved.archive_format,
        &platform_name,
        client,
        retry_policy,
        cache,
        reporter,
    )
    .await
}

/// Install a binary from a specific URL.
async fn bin_install_from_url(
    binary: Binary,
    version: &Version,
    download_url: &Url,
    format: ArchiveFormat,
    platform_name: &str,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
    cache: &Cache,
    reporter: &dyn Reporter,
) -> Result<PathBuf, Error> {
    let download_url = DisplaySafeUrl::from_url(download_url.clone());
    let cache_entry = CacheEntry::new(
        cache
            .bucket(CacheBucket::Binaries)
            .join(binary.name())
            .join(version.to_string())
            .join(platform_name),
        binary.executable(),
    );

    // Lock the directory to prevent racing installs
    let _lock = cache_entry.with_file(".lock").lock().await?;
    if cache_entry.path().exists() {
        return Ok(cache_entry.into_path_buf());
    }

    let cache_dir = cache_entry.dir();
    fs_err::tokio::create_dir_all(&cache_dir).await?;

    let path = download_and_unpack_with_retry(
        binary,
        version,
        client,
        retry_policy,
        cache,
        reporter,
        platform_name,
        format,
        &download_url,
        &cache_entry,
    )
    .await?;

    // Add executable bit
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs_err::tokio::metadata(&path).await?.permissions();
        if permissions.mode() & 0o111 != 0o111 {
            fs_err::tokio::set_permissions(
                &path,
                Permissions::from_mode(permissions.mode() | 0o111),
            )
            .await?;
        }
    }

    Ok(path)
}

/// Download and unpack a binary with retry on stream failures.
async fn download_and_unpack_with_retry(
    binary: Binary,
    version: &Version,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
    cache: &Cache,
    reporter: &dyn Reporter,
    platform_name: &str,
    format: ArchiveFormat,
    download_url: &DisplaySafeUrl,
    cache_entry: &CacheEntry,
) -> Result<PathBuf, Error> {
    let mut retry_state = RetryState::start(*retry_policy, download_url.clone());

    loop {
        let result = download_and_unpack(
            binary,
            version,
            client,
            cache,
            reporter,
            platform_name,
            format,
            download_url,
            cache_entry,
        )
        .await;

        match result {
            Ok(path) => return Ok(path),
            Err(err) => {
                if let Some(backoff) = retry_state.should_retry(&err, err.retries()) {
                    retry_state.sleep_backoff(backoff).await;
                    continue;
                }
                return if retry_state.total_retries() > 0 {
                    Err(Error::RetriedError {
                        err: Box::new(err),
                        retries: retry_state.total_retries(),
                    })
                } else {
                    Err(err)
                };
            }
        }
    }
}

/// Download and unpackage a binary,
///
/// NOTE [`download_and_unpack_with_retry`] should be used instead.
async fn download_and_unpack(
    binary: Binary,
    version: &Version,
    client: &BaseClient,
    cache: &Cache,
    reporter: &dyn Reporter,
    platform_name: &str,
    format: ArchiveFormat,
    download_url: &DisplaySafeUrl,
    cache_entry: &CacheEntry,
) -> Result<PathBuf, Error> {
    // Create a temporary directory for extraction
    let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::Binaries))?;

    let response = client
        .for_host(download_url)
        .get(Url::from(download_url.clone()))
        .send()
        .await
        .map_err(|err| Error::Download {
            url: download_url.clone(),
            source: err,
        })?;

    let inner_retries = response
        .extensions()
        .get::<reqwest_retry::RetryCount>()
        .map(|retries| retries.value());

    if let Err(status_error) = response.error_for_status_ref() {
        let err = Error::Download {
            url: download_url.clone(),
            source: reqwest_middleware::Error::from(status_error),
        };
        if let Some(retries) = inner_retries {
            return Err(Error::RetriedError {
                err: Box::new(err),
                retries,
            });
        }
        return Err(err);
    }

    // Get the download size from headers if available
    let size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|val| val.to_str().ok())
        .and_then(|val| val.parse::<u64>().ok());

    // Stream download directly to extraction
    let reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read()
        .compat();

    let id = reporter.on_download_start(binary.name(), version, size);
    let mut progress_reader = ProgressReader::new(reader, id, reporter);
    stream::archive(
        download_url,
        &mut progress_reader,
        format.into(),
        temp_dir.path(),
    )
    .await
    .map_err(|e| Error::Extract { source: e })?;
    reporter.on_download_complete(id);

    // Find the binary in the extracted files
    let extracted_binary = match format {
        ArchiveFormat::Zip => {
            // Windows ZIP archives contain the binary directly in the root
            temp_dir.path().join(binary.executable())
        }
        ArchiveFormat::TarGz => {
            // tar.gz archives contain the binary in a subdirectory
            temp_dir
                .path()
                .join(format!("{}-{platform_name}", binary.name()))
                .join(binary.executable())
        }
    };

    if !extracted_binary.exists() {
        return Err(Error::BinaryNotFound {
            expected: extracted_binary,
        });
    }

    // Move the binary to its final location before the temp directory is dropped
    fs_err::tokio::rename(&extracted_binary, cache_entry.path()).await?;

    Ok(cache_entry.path().to_path_buf())
}

/// Progress reporter for binary downloads.
pub trait Reporter: Send + Sync {
    /// Called when a download starts.
    fn on_download_start(&self, name: &str, version: &Version, size: Option<u64>) -> usize;
    /// Called when download progress is made.
    fn on_download_progress(&self, id: usize, inc: u64);
    /// Called when a download completes.
    fn on_download_complete(&self, id: usize);
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
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.as_mut().reader)
            .poll_read(cx, buf)
            .map_ok(|()| {
                self.reporter
                    .on_download_progress(self.index, buf.filled().len() as u64);
            })
    }
}
