//! Binary download and installation utilities for uv.
//!
//! This crate provides functionality for downloading and caching binary tools
//! from various sources (GitHub releases, etc.) for use by uv.

use std::path::PathBuf;
use std::str::FromStr;

use futures::TryStreamExt;
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_client::BaseClient;
use uv_distribution_filename::SourceDistExtension;
use uv_extract::stream;
use uv_pep440::Version;
use uv_platform::{Arch, Libc, Os};

/// Result type for binary installation operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Binary tools that can be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binary {
    /// Ruff formatter and linter
    Ruff,
}

impl Binary {
    /// Get the default version for this binary.
    pub fn default_version(&self) -> Version {
        match self {
            Binary::Ruff => Version::from_str("0.12.5").expect("valid version"),
        }
    }

    /// Get the tool name for cache and display purposes.
    pub fn name(&self) -> &'static str {
        match self {
            Binary::Ruff => "ruff",
        }
    }

    /// Get the download URL for a specific version and platform.
    pub fn download_url(&self, version: &Version, platform: &str, os: &Os) -> Url {
        match self {
            Binary::Ruff => {
                let archive_ext = if os.is_windows() { ".zip" } else { ".tar.gz" };
                let url_string = format!(
                    "https://github.com/astral-sh/ruff/releases/download/{version}/ruff-{platform}{archive_ext}"
                );
                Url::parse(&url_string).expect("valid URL")
            }
        }
    }

    /// Get the binary name for the target platform.
    pub fn binary_name(&self, os: &Os) -> String {
        let base_name = match self {
            Binary::Ruff => "ruff",
        };

        if os.is_windows() {
            format!("{}{}", base_name, std::env::consts::EXE_SUFFIX)
        } else {
            base_name.to_string()
        }
    }

    /// Get the expected directory name inside the archive.
    pub fn archive_dir_name(&self, platform: &str) -> String {
        match self {
            Binary::Ruff => format!("ruff-{platform}"),
        }
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

    /// Unsupported platform for binary download.
    #[error("Unsupported platform for {tool}: {platform}")]
    UnsupportedPlatform { tool: String, platform: String },

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

    /// Task join error.
    #[error("Task join error")]
    Join(#[from] tokio::task::JoinError),

    /// I/O error during installation.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Platform detection error.
    #[error("Failed to detect platform")]
    Platform(#[from] uv_platform::Error),

    /// Generic errors.
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

/// Install a binary tool, handling platform detection internally.
pub async fn install(
    binary: Binary,
    version: Option<&Version>,
    client: &BaseClient,
    cache: &Cache,
) -> Result<PathBuf> {
    // Platform detection happens inside
    let os = Os::from_env();
    let arch = Arch::from_env();
    let libc = Libc::from_env()?;

    // Get version to download
    let version = version.cloned().unwrap_or_else(|| binary.default_version());

    // Get platform-specific binary name
    let platform_name = get_platform_name(os, arch, libc);

    // Check cache first
    let cache_entry = CacheEntry::new(
        cache
            .bucket(CacheBucket::ToolBinaries)
            .join(binary.name())
            .join(version.to_string())
            .join(&platform_name),
        binary.binary_name(&os),
    );

    if cache_entry.path().exists() {
        return Ok(cache_entry.into_path_buf());
    }

    // Get download URL
    let download_url = binary.download_url(&version, &platform_name, &os);

    // Create cache directory first
    let cache_dir = cache_entry.dir();
    tokio::fs::create_dir_all(&cache_dir).await?;

    // Create a temporary directory for extraction
    let temp_dir = tempfile::tempdir_in(cache_dir.parent().unwrap())
        .map_err(|e| anyhow::anyhow!("Failed to create temp dir: {}", e))?;

    // Download and extract in one step
    let response = client
        .for_host(&download_url.clone().into())
        .get(download_url.clone())
        .send()
        .await
        .map_err(|e| Error::Download {
            tool: binary.name().to_string(),
            version: version.to_string(),
            url: download_url.to_string(),
            source: e,
        })?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download {}: {} returned {}",
            binary.name(),
            download_url,
            response.status()
        )
        .into());
    }

    // Determine archive type from URL
    let ext = if os.is_windows() {
        SourceDistExtension::Zip
    } else {
        SourceDistExtension::TarGz
    };

    // Stream download directly to extraction
    let mut reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read()
        .compat();

    stream::archive(&mut reader, ext, temp_dir.path())
        .await
        .map_err(|e| Error::Extract {
            tool: binary.name().to_string(),
            source: e.into(),
        })?;

    // Find the binary in the extracted files
    // The archive contains a directory with the platform name
    let binary_name = binary.binary_name(&os);
    let archive_dir_name = binary.archive_dir_name(&platform_name);
    let extracted_binary = temp_dir.path().join(&archive_dir_name).join(&binary_name);

    if !extracted_binary.exists() {
        // Try without the directory structure (in case archive format changes)
        let direct_binary = temp_dir.path().join(&binary_name);
        if direct_binary.exists() {
            // Copy binary to cache location
            tokio::fs::copy(&direct_binary, cache_entry.path()).await?;
        } else {
            return Err(Error::BinaryNotFound {
                tool: binary.name().to_string(),
                expected: extracted_binary,
            });
        }
    } else {
        // Copy binary to cache location
        tokio::fs::copy(&extracted_binary, cache_entry.path()).await?;
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(cache_entry.path()).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(cache_entry.path(), perms).await?;
    }

    Ok(cache_entry.into_path_buf())
}

/// Map UV's platform types to standard target triple naming convention.
fn get_platform_name(os: Os, arch: Arch, libc: Libc) -> String {
    use target_lexicon::{
        Architecture, ArmArchitecture, OperatingSystem, Riscv64Architecture, X86_32Architecture,
    };

    // Get base architecture string
    let arch_str = match arch.family() {
        // Special cases where Display doesn't match target triple
        Architecture::X86_32(X86_32Architecture::I686) => "i686".to_string(),
        Architecture::Riscv64(Riscv64Architecture::Riscv64) => "riscv64gc".to_string(),
        _ => arch.to_string(),
    };

    // Determine vendor
    let vendor = match &*os {
        OperatingSystem::Darwin(_) => "apple",
        OperatingSystem::Windows => "pc",
        _ => "unknown",
    };

    // Map OS names (only Darwin needs special handling)
    let os_name = match &*os {
        OperatingSystem::Darwin(_) => "darwin",
        _ => &os.to_string(),
    };

    // Build base triple
    let mut triple = format!("{arch_str}-{vendor}-{os_name}");

    // Add environment/ABI suffix
    match (&*os, libc) {
        (OperatingSystem::Windows, _) => triple.push_str("-msvc"),
        (OperatingSystem::Linux, Libc::Some(env)) => {
            triple.push('-');
            triple.push_str(&env.to_string());

            // Special suffix for ARM with hardware float
            if matches!(arch.family(), Architecture::Arm(ArmArchitecture::Armv7)) {
                triple.push_str("eabihf");
            }
        }
        _ => {}
    }

    triple
}
