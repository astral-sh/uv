//! Binary download and installation utilities for uv.
//!
//! These utilities are specifically for consuming distributions that are _not_ Python packages,
//! e.g., `ruff` (which does have a Python package, but also has standalone binaries on GitHub).

use std::error::Error as _;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTimeError};

use futures::{StreamExt, TryStreamExt};
use reqwest_retry::Retryable;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;
use uv_client::retryable_on_request_failure;
use uv_distribution_filename::SourceDistExtension;

use uv_cache::{Cache, CacheBucket, CacheEntry, Error as CacheError};
use uv_client::{BaseClient, RetriableError, fetch_with_url_fallback};
use uv_extract::{Error as ExtractError, stream};
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_platform::Platform;
use uv_redacted::DisplaySafeUrl;

/// Binary tools that can be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binary {
    Ruff,
    Uv,
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
            Self::Uv => VersionSpecifiers::empty(),
        }
    }

    /// The name of the binary.
    ///
    /// See [`Binary::executable`] for the platform-specific executable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ruff => "ruff",
            Self::Uv => "uv",
        }
    }

    /// Get the ordered list of download URLs for a specific version and platform.
    pub fn download_urls(
        &self,
        version: &Version,
        platform: &str,
        format: ArchiveFormat,
    ) -> Result<Vec<DisplaySafeUrl>, Error> {
        match self {
            Self::Ruff => {
                let suffix = format!("{version}/ruff-{platform}.{}", format.extension());
                let canonical = format!("{RUFF_GITHUB_URL_PREFIX}{suffix}");
                let mirror = format!("{RUFF_DEFAULT_MIRROR}{suffix}");
                Ok(vec![
                    DisplaySafeUrl::parse(&mirror).map_err(|err| Error::UrlParse {
                        url: mirror,
                        source: err,
                    })?,
                    DisplaySafeUrl::parse(&canonical).map_err(|err| Error::UrlParse {
                        url: canonical,
                        source: err,
                    })?,
                ])
            }
            Self::Uv => {
                let canonical = format!(
                    "{UV_GITHUB_URL_PREFIX}{version}/uv-{platform}.{}",
                    format.extension()
                );
                Ok(vec![DisplaySafeUrl::parse(&canonical).map_err(|err| {
                    Error::UrlParse {
                        url: canonical,
                        source: err,
                    }
                })?])
            }
        }
    }

    /// Return the ordered list of manifest URLs to try for this binary.
    fn manifest_urls(self) -> Vec<DisplaySafeUrl> {
        let name = self.name();
        match self {
            // These are static strings so parsing cannot fail.
            Self::Ruff => vec![
                DisplaySafeUrl::parse(&format!("{VERSIONS_MANIFEST_MIRROR}/{name}.ndjson"))
                    .unwrap(),
                DisplaySafeUrl::parse(&format!("{VERSIONS_MANIFEST_URL}/{name}.ndjson")).unwrap(),
            ],
            Self::Uv => vec![
                DisplaySafeUrl::parse(&format!("{VERSIONS_MANIFEST_MIRROR}/{name}.ndjson"))
                    .unwrap(),
                DisplaySafeUrl::parse(&format!("{VERSIONS_MANIFEST_URL}/{name}.ndjson")).unwrap(),
            ],
        }
    }

    /// Given a canonical artifact URL (e.g., from the versions manifest), return the ordered list
    /// of URLs to try for this binary.
    fn mirror_urls(self, canonical_url: DisplaySafeUrl) -> Vec<DisplaySafeUrl> {
        match self {
            Self::Ruff => {
                if let Some(suffix) = canonical_url.as_str().strip_prefix(RUFF_GITHUB_URL_PREFIX) {
                    let mirror_str = format!("{RUFF_DEFAULT_MIRROR}{suffix}");
                    if let Ok(mirror_url) = DisplaySafeUrl::parse(&mirror_str) {
                        return vec![mirror_url, canonical_url];
                    }
                }
                vec![canonical_url]
            }
            Self::Uv => vec![canonical_url],
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

/// The canonical GitHub URL prefix for Ruff releases.
const RUFF_GITHUB_URL_PREFIX: &str = "https://github.com/astral-sh/ruff/releases/download/";

/// The canonical GitHub URL prefix for uv releases.
const UV_GITHUB_URL_PREFIX: &str = "https://github.com/astral-sh/uv/releases/download/";

/// The default Astral mirror for Ruff releases.
///
/// This mirror is tried first for Ruff downloads. If it fails, uv falls back to the canonical
/// GitHub URL.
const RUFF_DEFAULT_MIRROR: &str = "https://releases.astral.sh/github/ruff/releases/download/";

/// The canonical base URL for the versions manifest.
const VERSIONS_MANIFEST_URL: &str = "https://raw.githubusercontent.com/astral-sh/versions/main/v1";

/// The default Astral mirror for the versions manifest.
const VERSIONS_MANIFEST_MIRROR: &str = "https://releases.astral.sh/github/versions/main/v1";

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
    /// The ordered list of download URLs to try for this version and current platform.
    pub artifact_urls: Vec<DisplaySafeUrl>,
    /// The archive format.
    pub archive_format: ArchiveFormat,
}

impl ResolvedVersion {
    /// Construct a [`ResolvedVersion`] from a [`Binary`] and a [`Version`] by inferring the
    /// download URLs and archive format from the current platform.
    pub fn from_version(binary: Binary, version: Version) -> Result<Self, Error> {
        let platform = Platform::from_env()?;
        let platform_name = platform.as_cargo_dist_triple();
        let archive_format = if platform.os.is_windows() {
            ArchiveFormat::Zip
        } else {
            ArchiveFormat::TarGz
        };
        let artifact_urls = binary.download_urls(&version, &platform_name, archive_format)?;
        Ok(Self {
            version,
            artifact_urls,
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

    #[error("Failed to read from: {url}")]
    Stream {
        url: DisplaySafeUrl,
        #[source]
        source: reqwest::Error,
    },

    #[error("Failed to parse URL: {url}")]
    UrlParse {
        url: String,
        #[source]
        source: uv_redacted::DisplaySafeUrlError,
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

    #[error(
        "Request failed after {retries} {subject} in {duration:.1}s",
        subject = if *retries > 1 { "retries" } else { "retry" },
        duration = duration.as_secs_f32()
    )]
    RetriedError {
        #[source]
        err: Box<Self>,
        retries: u32,
        duration: Duration,
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

    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

impl RetriableError for Error {
    fn retries(&self) -> u32 {
        if let Self::RetriedError { retries, .. } = self {
            return *retries;
        }
        0
    }

    /// Returns `true` if trying an alternative URL makes sense after this error.
    ///
    /// Download and streaming failures qualify, as do malformed manifest responses.
    fn should_try_next_url(&self) -> bool {
        match self {
            Self::Download { .. }
            | Self::ManifestFetch { .. }
            | Self::ManifestParse(..)
            | Self::ManifestUtf8(..) => true,
            Self::Stream { .. } => true,
            Self::RetriedError { err, .. } => err.should_try_next_url(),
            err => {
                // Walk the error chain to see if there's a nested download or streaming error.
                let mut source = err.source();
                while let Some(err) = source {
                    if let Some(io_err) = err.downcast_ref::<io::Error>() {
                        if io_err
                            .get_ref()
                            .and_then(|e| e.downcast_ref::<Self>() as Option<&Self>)
                            .is_some_and(|e| {
                                matches!(e, Self::Stream { .. } | Self::Download { .. })
                            })
                        {
                            return true;
                        }
                    }
                    source = err.source();
                }
                // Make sure all retriable errors also trigger a fallback to the next URL.
                retryable_on_request_failure(err) == Some(Retryable::Transient)
            }
        }
    }

    fn into_retried(self, retries: u32, duration: Duration) -> Self {
        Self::RetriedError {
            err: Box::new(self),
            retries,
            duration,
        }
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

    fetch_with_url_fallback(
        &binary.manifest_urls(),
        *retry_policy,
        &format!("manifest for `{binary}`"),
        |url| {
            fetch_and_find_matching_version(
                binary,
                constraints,
                exclude_newer,
                &platform_name,
                url,
                client,
            )
        },
    )
    .await
}

/// Fetch the manifest from a single URL and find a matching version.
///
/// Separated from [`find_matching_version`] so that [`fetch_with_url_fallback`] can call it
/// independently for each URL in the fallback list.
async fn fetch_and_find_matching_version(
    binary: Binary,
    constraints: Option<&uv_pep440::VersionSpecifiers>,
    exclude_newer: Option<jiff::Timestamp>,
    platform_name: &str,
    manifest_url: DisplaySafeUrl,
    client: &BaseClient,
) -> Result<ResolvedVersion, Error> {
    let response = client
        .for_host(&manifest_url)
        .get(Url::from(manifest_url.clone()))
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
            binary,
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
    binary: Binary,
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

        let Ok(canonical_url) = DisplaySafeUrl::parse(&artifact.url) else {
            continue;
        };

        let archive_format = match artifact.archive_format.as_str() {
            "tar.gz" => ArchiveFormat::TarGz,
            "zip" => ArchiveFormat::Zip,
            _ => continue,
        };

        return Some(ResolvedVersion {
            version: version_info.version.clone(),
            artifact_urls: binary.mirror_urls(canonical_url),
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

    bin_install_from_urls(
        binary,
        &resolved.version,
        &resolved.artifact_urls,
        resolved.archive_format,
        &platform_name,
        client,
        retry_policy,
        cache,
        reporter,
    )
    .await
}

/// Install a binary from an ordered list of URLs, trying each in sequence.
async fn bin_install_from_urls(
    binary: Binary,
    version: &Version,
    download_urls: &[DisplaySafeUrl],
    format: ArchiveFormat,
    platform_name: &str,
    client: &BaseClient,
    retry_policy: &ExponentialBackoff,
    cache: &Cache,
    reporter: &dyn Reporter,
) -> Result<PathBuf, Error> {
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

    let path = fetch_with_url_fallback(
        download_urls,
        *retry_policy,
        &format!("`{binary}`"),
        |url| {
            download_and_unpack(
                binary,
                version,
                client,
                cache,
                reporter,
                platform_name,
                format,
                url,
                &cache_entry,
            )
        },
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

/// Download and unpack a binary from a single URL.
///
/// Use [`bin_install_from_urls`] (via [`fetch_with_url_fallback`]) to get URL-fallback and retry.
async fn download_and_unpack(
    binary: Binary,
    version: &Version,
    client: &BaseClient,
    cache: &Cache,
    reporter: &dyn Reporter,
    platform_name: &str,
    format: ArchiveFormat,
    download_url: DisplaySafeUrl,
    cache_entry: &CacheEntry,
) -> Result<PathBuf, Error> {
    // Create a temporary directory for extraction
    let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::Binaries))?;

    let response = client
        .for_host(&download_url)
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
                // This value is overwritten in `download_and_unpack_with_retry`.
                duration: Duration::default(),
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
        .map_err(|err| {
            std::io::Error::other(Error::Stream {
                url: download_url.clone(),
                source: err,
            })
        })
        .into_async_read()
        .compat();

    let id = reporter.on_download_start(binary.name(), version, size);
    let mut progress_reader = ProgressReader::new(reader, id, reporter);
    stream::archive(&mut progress_reader, format.into(), temp_dir.path())
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::io::Write;
    use uv_client::{BaseClientBuilder, fetch_with_url_fallback, retryable_on_request_failure};
    use uv_redacted::DisplaySafeUrl;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn spawn_manifest_server(response: ResponseTemplate) -> (DisplaySafeUrl, MockServer) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/uv.ndjson"))
            .respond_with(response)
            .mount(&server)
            .await;

        (
            DisplaySafeUrl::parse(&format!("{}/uv.ndjson", server.uri())).unwrap(),
            server,
        )
    }

    fn manifest_response(body: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/x-ndjson")
    }

    fn not_found_response() -> ResponseTemplate {
        ResponseTemplate::new(404)
    }

    fn uv_manifest_line(version: &str, platform: &str) -> String {
        let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
        let url = format!(
            "https://github.com/astral-sh/uv/releases/download/{version}/uv-{platform}.{extension}"
        );

        format!(
            "{}\n",
            json!({
                "version": version,
                "date": "2025-01-01T00:00:00Z",
                "artifacts": [{
                    "platform": platform,
                    "url": url,
                    "archive_format": extension,
                }],
            })
        )
    }

    async fn resolve_version_from_manifest_urls(
        urls: &[DisplaySafeUrl],
        constraints: Option<&VersionSpecifiers>,
    ) -> Result<ResolvedVersion, Error> {
        let platform = Platform::from_env().unwrap();
        let platform_name = platform.as_cargo_dist_triple();
        let client_builder = BaseClientBuilder::default().retries(0);
        let retry_policy = client_builder.retry_policy();
        let client = client_builder.build().expect("failed to build base client");

        fetch_with_url_fallback(urls, retry_policy, "manifest for `uv`", |url| {
            fetch_and_find_matching_version(
                Binary::Uv,
                constraints,
                None,
                &platform_name,
                url,
                &client,
            )
        })
        .await
    }

    #[test]
    fn test_uv_download_urls() {
        let urls = Binary::Uv
            .download_urls(
                &Version::new([0, 6, 0]),
                "x86_64-unknown-linux-gnu",
                ArchiveFormat::TarGz,
            )
            .expect("uv download URLs should be valid");

        let urls = urls
            .into_iter()
            .map(|url| url.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![
                "https://github.com/astral-sh/uv/releases/download/0.6.0/uv-x86_64-unknown-linux-gnu.tar.gz"
                    .to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_manifest_falls_back_on_404() {
        let platform = Platform::from_env().unwrap();
        let platform_name = platform.as_cargo_dist_triple();
        let (mirror_url, mirror_server) = spawn_manifest_server(not_found_response()).await;
        let (canonical_url, canonical_server) = spawn_manifest_server(manifest_response(
            &uv_manifest_line("1.2.3", &platform_name),
        ))
        .await;

        let resolved = resolve_version_from_manifest_urls(&[mirror_url, canonical_url], None)
            .await
            .expect("404 from mirror should fall back to canonical manifest");

        assert_eq!(resolved.version, Version::new([1, 2, 3]));
        assert_eq!(mirror_server.received_requests().await.unwrap().len(), 1);
        assert_eq!(canonical_server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_manifest_falls_back_on_parse_error() {
        let platform = Platform::from_env().unwrap();
        let platform_name = platform.as_cargo_dist_triple();
        let (mirror_url, mirror_server) =
            spawn_manifest_server(manifest_response("{not json}\n")).await;
        let (canonical_url, canonical_server) = spawn_manifest_server(manifest_response(
            &uv_manifest_line("1.2.3", &platform_name),
        ))
        .await;

        let resolved = resolve_version_from_manifest_urls(&[mirror_url, canonical_url], None)
            .await
            .expect("parse failure from mirror should fall back to canonical manifest");

        assert_eq!(resolved.version, Version::new([1, 2, 3]));
        assert_eq!(mirror_server.received_requests().await.unwrap().len(), 1);
        assert_eq!(canonical_server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_manifest_no_matching_version_does_not_fallback() {
        let platform = Platform::from_env().unwrap();
        let platform_name = platform.as_cargo_dist_triple();
        let (mirror_url, mirror_server) = spawn_manifest_server(manifest_response(
            &uv_manifest_line("1.2.3", &platform_name),
        ))
        .await;
        let (canonical_url, canonical_server) = spawn_manifest_server(manifest_response(
            &uv_manifest_line("9.9.9", &platform_name),
        ))
        .await;
        let constraints =
            VersionSpecifiers::from(VersionSpecifier::equals_version(Version::new([9, 9, 9])));

        let err =
            resolve_version_from_manifest_urls(&[mirror_url, canonical_url], Some(&constraints))
                .await
                .expect_err("no matching version should not fall back to canonical manifest");

        assert!(matches!(err, Error::NoMatchingVersion { .. }));
        assert_eq!(mirror_server.received_requests().await.unwrap().len(), 1);
        assert_eq!(canonical_server.received_requests().await.unwrap().len(), 0);
    }

    /// Verify that `should_try_next_url` returns `true` even for streaming errors
    /// that `retryable_on_request_failure` does not recognise as transient.
    ///
    /// This exercises a realistic body-streaming protocol failure: the server
    /// advertises chunked transfer encoding but sends an invalid chunk size.
    #[tokio::test]
    async fn test_non_retryable_stream_error_triggers_url_fallback() {
        use futures::TryStreamExt;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nZZZ\r\nhello\r\n0\r\n\r\n",
                )
                .unwrap();
        });

        let url = DisplaySafeUrl::parse(&format!("http://{addr}/ruff.tar.gz")).unwrap();
        let client = BaseClientBuilder::default()
            .build()
            .expect("failed to build base client");
        let response = client
            .for_host(&url)
            .get(Url::from(url.clone()))
            .send()
            .await
            .unwrap();

        let reqwest_err = response.bytes_stream().try_next().await.unwrap_err();
        assert!(reqwest_err.is_body() || reqwest_err.is_decode());

        let err = Error::Extract {
            source: ExtractError::Io(io::Error::other(Error::Stream {
                url,
                source: reqwest_err,
            })),
        };

        assert!(retryable_on_request_failure(&err).is_none());
        assert!(
            err.should_try_next_url(),
            "non-retryable streaming error should still trigger URL fallback, got: {err}"
        );
    }
}
