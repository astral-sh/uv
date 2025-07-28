//! Binary download and installation utilities for uv.
//!
//! These utilities are specifically for consuming distributions that are _not_ Python packages,
//! e.g., `ruff` (which does have a Python package, but also has standalone binaries on GitHub).

use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use futures::TryStreamExt;
use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_client::BaseClient;
use uv_distribution_filename::SourceDistExtension;
use uv_extract::stream;
use uv_pep440::Version;
use uv_platform::{Arch, Libc, Os};

/// Binary tools that can be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binary {
    Ruff,
}

impl Binary {
    /// Get the default version for this binary.
    pub fn default_version(&self) -> Version {
        match self {
            Binary::Ruff => Version::from_str("0.12.5").expect("valid version"),
        }
    }

    /// The name of the binary.
    ///
    /// See [`Binary::executable`] for the platform-specific executable name.
    pub fn name(&self) -> &'static str {
        match self {
            Binary::Ruff => "ruff",
        }
    }

    /// Get the download URL for a specific version and platform.
    pub fn download_url(
        &self,
        version: &Version,
        platform: &str,
        ext: &SourceDistExtension,
    ) -> Result<Url, Error> {
        match self {
            Binary::Ruff => {
                let url = format!(
                    "https://github.com/astral-sh/ruff/releases/download/{version}/ruff-{platform}.{ext}"
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

/// Errors that can occur during binary download and installation.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to download binary.
    #[error("Failed to download {tool} {version} from {url}")]
    Download {
        tool: String,
        version: String,
        url: String,
        #[source]
        source: reqwest_middleware::Error,
    },

    /// Failed to parse download URL.
    #[error("Failed to parse download URL: {url}")]
    UrlParse {
        url: String,
        #[source]
        source: url::ParseError,
    },

    /// Failed to extract archive.
    #[error("Failed to extract {tool} archive")]
    Extract {
        tool: String,
        #[source]
        source: anyhow::Error,
    },

    /// Binary not found in extracted archive.
    #[error("Binary not found in {tool} archive at expected location: {expected}")]
    BinaryNotFound { tool: String, expected: PathBuf },

    /// I/O error during installation.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Platform detection error.
    #[error("Failed to detect platform")]
    Platform(#[from] uv_platform::Error),
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
        let before = buf.filled().len();
        match Pin::new(&mut self.reader).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let after = buf.filled().len();
                let bytes = after - before;
                if bytes > 0 {
                    self.reporter.on_download_progress(self.index, bytes as u64);
                }
                Poll::Ready(Ok(()))
            }
            poll => poll,
        }
    }
}

/// Install a binary for the given tool.
pub async fn bin_install(
    binary: Binary,
    version: Option<&Version>,
    client: &BaseClient,
    cache: &Cache,
    reporter: Option<&dyn Reporter>,
) -> Result<PathBuf, Error> {
    let os = Os::from_env();
    let arch = Arch::from_env();
    let libc = Libc::from_env()?;
    let version = version.cloned().unwrap_or_else(|| binary.default_version());
    let platform_name = platform_name_for_binary(os, arch, libc);

    // Check the cache first
    let cache_entry = CacheEntry::new(
        cache
            .bucket(CacheBucket::Binaries)
            .join(binary.name())
            .join(version.to_string())
            .join(&platform_name),
        binary.executable(),
    );

    if let Ok(true) = cache_entry.path().try_exists() {
        return Ok(cache_entry.into_path_buf());
    }

    let ext = if os.is_windows() {
        SourceDistExtension::Zip
    } else {
        SourceDistExtension::TarGz
    };

    let download_url = binary.download_url(&version, &platform_name, &ext)?;

    let cache_dir = cache_entry.dir();
    tokio::fs::create_dir_all(&cache_dir).await?;

    // Create a temporary directory for extraction
    let temp_dir = tempfile::tempdir_in(cache_dir.parent().unwrap())?;

    let response = client
        .for_host(&download_url.clone().into())
        .get(download_url.clone())
        .send()
        .await
        .map_err(|err| Error::Download {
            tool: binary.name().to_string(),
            version: version.to_string(),
            url: download_url.to_string(),
            source: err,
        })?;

    let response = response.error_for_status().map_err(|err| Error::Download {
        tool: binary.name().to_string(),
        version: version.to_string(),
        url: download_url.to_string(),
        source: reqwest_middleware::Error::Reqwest(err),
    })?;

    // Get the download size from headers if available
    let size = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|val| val.to_str().ok())
        .and_then(|val| val.parse::<u64>().ok());

    // Stream download directly to extraction
    let mut reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read()
        .compat();

    if let Some(reporter) = reporter {
        let id = reporter.on_download_start(binary.name(), &version, size);
        let mut progress_reader = ProgressReader::new(reader, id, reporter);
        stream::archive(&mut progress_reader, ext, temp_dir.path())
            .await
            .map_err(|e| Error::Extract {
                tool: binary.name().to_string(),
                source: e.into(),
            })?;
        reporter.on_download_complete(id);
    } else {
        stream::archive(&mut reader, ext, temp_dir.path())
            .await
            .map_err(|e| Error::Extract {
                tool: binary.name().to_string(),
                source: e.into(),
            })?;
    }

    // Find the binary in the extracted files
    // The archive contains a directory with the platform name
    let extracted_binary = temp_dir
        .path()
        .join(format!("{}-{platform_name}", binary.name()))
        .join(binary.executable());

    uv_fs::rename_with_retry(&extracted_binary, cache_entry.path()).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(cache_entry.path()).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(cache_entry.path(), perms).await?;
    }

    Ok(cache_entry.into_path_buf())
}

/// Cast platform types to the binary target triple format.
///
/// This performs some normalization to match cargo-dist's styling.
fn platform_name_for_binary(os: Os, arch: Arch, libc: Libc) -> String {
    use target_lexicon::{
        Architecture, ArmArchitecture, OperatingSystem, Riscv64Architecture, X86_32Architecture,
    };
    let arch_name = match arch.family() {
        // Special cases where Display doesn't match target triple
        Architecture::X86_32(X86_32Architecture::I686) => "i686".to_string(),
        Architecture::Riscv64(Riscv64Architecture::Riscv64) => "riscv64gc".to_string(),
        _ => arch.to_string(),
    };
    let vendor = match &*os {
        OperatingSystem::Darwin(_) => "apple",
        OperatingSystem::Windows => "pc",
        _ => "unknown",
    };
    let os_name = match &*os {
        OperatingSystem::Darwin(_) => "darwin",
        _ => &os.to_string(),
    };

    let abi = match (&*os, libc) {
        (OperatingSystem::Windows, _) => Some("msvc".to_string()),
        (OperatingSystem::Linux, Libc::Some(env)) => Some({
            // Special suffix for ARM with hardware float
            if matches!(arch.family(), Architecture::Arm(ArmArchitecture::Armv7)) {
                format!("{env}eabihf")
            } else {
                env.to_string()
            }
        }),
        _ => None,
    };

    format!(
        "{arch_name}-{vendor}-{os_name}{abi}",
        abi = abi.map(|abi| format!("-{abi}")).unwrap_or_default()
    )
}
