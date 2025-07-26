use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use futures::TryStreamExt;
use tokio::io::AsyncWriteExt;
use tracing::debug;
use url::Url;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_client::BaseClient;
use uv_extract::unzip;
use uv_platform::{Arch, Libc, Os};

use crate::download::BinaryDownloader;
use crate::error::Error;

/// Download and cache Ruff binaries.
pub struct RuffDownloader;

impl RuffDownloader {
    /// Default Ruff version to use when none is specified.
    /// This should be updated when UV is released to ensure compatibility.
    const DEFAULT_VERSION: &'static str = "0.12.5";

    /// Map UV's platform types to Ruff's binary naming convention.
    fn platform_to_ruff_name(os: &Os, arch: &Arch, libc: &Libc) -> Option<&'static str> {
        // Convert to string representations for matching
        // This is a workaround until we have a dedicated uv-platform crate
        let os_str = os.to_string();
        let arch_str = arch.to_string();
        let libc_str = libc.to_string();

        match (os_str.as_str(), arch_str.as_str(), libc_str.as_str()) {
            // macOS
            ("macos", "x86_64", _) => Some("x86_64-apple-darwin"),
            ("macos", "aarch64", _) => Some("aarch64-apple-darwin"),

            // Windows
            ("windows", "x86_64", _) => Some("x86_64-pc-windows-msvc"),
            ("windows", "aarch64", _) => Some("aarch64-pc-windows-msvc"),
            ("windows", "x86", _) => Some("i686-pc-windows-msvc"),

            // Linux with glibc
            ("linux", "x86_64", "gnu") => Some("x86_64-unknown-linux-gnu"),
            ("linux", "aarch64", "gnu") => Some("aarch64-unknown-linux-gnu"),
            ("linux", "x86", "gnu") => Some("i686-unknown-linux-gnu"),
            ("linux", "arm", "gnu") => Some("armv7-unknown-linux-gnueabihf"),
            ("linux", "s390x", "gnu") => Some("s390x-unknown-linux-gnu"),
            ("linux", "powerpc64", "gnu") => Some("powerpc64-unknown-linux-gnu"),
            ("linux", "powerpc64le", "gnu") => Some("powerpc64le-unknown-linux-gnu"),
            ("linux", "riscv64", "gnu") => Some("riscv64gc-unknown-linux-gnu"),

            // Linux with musl
            ("linux", "x86_64", "musl") => Some("x86_64-unknown-linux-musl"),
            ("linux", "aarch64", "musl") => Some("aarch64-unknown-linux-musl"),
            ("linux", "x86", "musl") => Some("i686-unknown-linux-musl"),
            ("linux", "arm", "musl") => Some("armv7-unknown-linux-musleabihf"),

            _ => None,
        }
    }

    /// Get the file extension for the platform.
    fn get_archive_extension(os: &Os) -> &'static str {
        match &**os {
            // Check if it's Windows by looking at the Display output
            os if os.to_string().contains("windows") => ".zip",
            _ => ".tar.gz",
        }
    }
}

#[async_trait]
impl BinaryDownloader for RuffDownloader {
    fn tool_name(&self) -> &str {
        "ruff"
    }

    fn default_version(&self) -> &str {
        Self::DEFAULT_VERSION
    }

    fn platform_identifier(&self, os: &Os, arch: &Arch, libc: &Libc) -> Option<String> {
        Self::platform_to_ruff_name(os, arch, libc).map(String::from)
    }

    fn download_url(&self, version: &str, platform: &str) -> String {
        let archive_extension = if platform.contains("windows") {
            ".zip"
        } else {
            ".tar.gz"
        };
        let archive_name = format!("ruff-{}{}", platform, archive_extension);
        format!(
            "https://github.com/astral-sh/ruff/releases/download/{}/{}",
            version, archive_name
        )
    }

    fn archive_extension(&self, os: &Os) -> &str {
        Self::get_archive_extension(os)
    }

    fn binary_name(&self, os: &Os) -> &str {
        if os.to_string().contains("windows") {
            "ruff.exe"
        } else {
            "ruff"
        }
    }

    fn archive_directory(&self, platform: &str) -> Option<String> {
        Some(format!("ruff-{}", platform))
    }

    async fn download(
        &self,
        version: Option<&str>,
        os: &Os,
        arch: &Arch,
        libc: &Libc,
        client: &BaseClient,
        cache: &Cache,
    ) -> crate::Result<PathBuf> {
        debug!("RuffDownloader::download called with version: {:?}", version);
        // Get version to download
        let version = if let Some(v) = version {
            v.to_string()
        } else {
            self.default_version().to_string()
        };

        // Get platform-specific binary name
        debug!("Getting platform name for os={}, arch={}, libc={}", os, arch, libc);
        let platform_name = self.platform_identifier(os, arch, libc)
            .ok_or_else(|| Error::UnsupportedPlatform {
                tool: self.tool_name().to_string(),
                platform: format!("{:?}-{:?}", os, arch),
            })?;
        debug!("Platform name: {}", platform_name);

        let archive_extension = self.archive_extension(os);
        let archive_name = format!("ruff-{}{}", platform_name, archive_extension);
        debug!("Archive name: {}", archive_name);

        // Check cache first
        debug!("Creating cache entry");
        let cache_entry = CacheEntry::new(
            cache.bucket(CacheBucket::ToolBinaries).join(self.tool_name()).join(&version).join(&platform_name),
            self.binary_name(os),
        );
        debug!("Cache entry created at: {}", cache_entry.path().display());

        if cache_entry.path().exists() {
            debug!("Using cached Ruff binary at {}", cache_entry.path().display());
            return Ok(cache_entry.into_path_buf());
        }
        
        debug!("Cache entry path: {}", cache_entry.path().display());
        debug!("Cache dir: {}", cache_entry.dir().display());

        // Download URL
        let download_url = self.download_url(&version, &platform_name);

        debug!("Downloading Ruff {} from {}", version, download_url);

        // Create cache directory first
        let cache_bucket_dir = cache.bucket(CacheBucket::ToolBinaries);
        debug!("Creating cache bucket dir: {}", cache_bucket_dir.display());
        tokio::fs::create_dir_all(&cache_bucket_dir).await
            .map_err(|e| anyhow::anyhow!("Failed to create cache bucket dir {}: {}", cache_bucket_dir.display(), e))?;
        debug!("Cache bucket dir created successfully");
        
        // Download to temporary file
        debug!("Creating temp dir in: {}", cache_bucket_dir.display());
        let temp_dir = tempfile::tempdir_in(&cache_bucket_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create temp dir in {}: {}", cache_bucket_dir.display(), e))?;
        debug!("Temp dir created at: {}", temp_dir.path().display());
        let archive_path = temp_dir.path().join(&archive_name);
        debug!("Archive will be downloaded to: {}", archive_path.display());

        debug!("Sending HTTP request to download URL");
        let response = client
            .for_host(&Url::parse(&download_url).map_err(|e| Error::UrlParse {
                url: download_url.clone(),
                source: e,
            })?.into())
            .get(reqwest::Url::from_str(&download_url).map_err(|e| Error::UrlParse {
                url: download_url.clone(),
                source: e,
            })?)
            .send()
            .await
            .map_err(|e| Error::Download {
                tool: self.tool_name().to_string(),
                version: version.clone(),
                url: download_url.clone(),
                source: e,
            })?;
        debug!("HTTP response received: status={}", response.status());

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download Ruff: {} returned {}",
                download_url,
                response.status()
            ).into());
        }

        // Write to file
        debug!("Creating archive file at: {}", archive_path.display());
        let mut file = tokio::fs::File::create(&archive_path).await
            .map_err(|e| anyhow::anyhow!("Failed to create archive file {}: {}", archive_path.display(), e))?;
        debug!("Archive file created, starting download");
        let stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
        
        futures::pin_mut!(stream);
        while let Some(chunk) = stream.try_next().await? {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);
        debug!("Download complete, file written to: {}", archive_path.display());

        // Extract archive
        let extracted_dir = temp_dir.path().join("extracted");
        debug!("Creating extraction directory: {}", extracted_dir.display());
        tokio::fs::create_dir_all(&extracted_dir).await
            .map_err(|e| anyhow::anyhow!("Failed to create extracted dir {}: {}", extracted_dir.display(), e))?;
        debug!("Extraction directory created");
        
        // Extract based on file extension
        debug!("Starting extraction for archive type: {}", archive_extension);
        if archive_name.ends_with(".zip") {
            let file = std::fs::File::open(&archive_path)?;
            tokio::task::spawn_blocking({
                let extracted_dir = extracted_dir.clone();
                move || unzip(file, &extracted_dir)
            })
            .await?
            .map_err(|e| Error::Extract {
                tool: self.tool_name().to_string(),
                source: e.into(),
            })?;
            debug!("ZIP extraction complete");
        } else {
            debug!("Extracting tar.gz archive");
            // For .tar.gz files, we need to extract manually
            let file = std::fs::File::open(&archive_path)
                .map_err(|e| anyhow::anyhow!("Failed to open archive {}: {}", archive_path.display(), e))?;
            let tar = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(tar);
            tokio::task::spawn_blocking({
                let extracted_dir = extracted_dir.clone();
                move || archive.unpack(&extracted_dir)
                    .map_err(|e| anyhow::anyhow!("Failed to unpack archive: {}", e))
            })
            .await?
            .map_err(|e| Error::Extract {
                tool: self.tool_name().to_string(),
                source: e,
            })?;
            debug!("tar.gz extraction complete");
        }

        // Create cache directory first before copying
        let cache_dir = cache_entry.dir();
        debug!("Creating cache directory: {}", cache_dir.display());
        tokio::fs::create_dir_all(cache_dir).await?;
        debug!("Cache directory created");

        // Find the ruff binary in the extracted files
        // The archive contains a directory with the platform name
        let binary_name = self.binary_name(os);
        let archive_dir_name = format!("ruff-{}", platform_name);
        let extracted_binary = extracted_dir.join(&archive_dir_name).join(binary_name);
        debug!("Looking for binary at: {}", extracted_binary.display());
        
        if !extracted_binary.exists() {
            debug!("Binary not found at expected location, trying direct path");
            // Try without the directory structure (in case archive format changes)
            let direct_binary = extracted_dir.join(binary_name);
            debug!("Checking direct binary path: {}", direct_binary.display());
            if direct_binary.exists() {
                debug!("Found binary at direct path");
                // Copy binary to cache location
                debug!("Copying binary from {} to {}", direct_binary.display(), cache_entry.path().display());
                tokio::fs::copy(&direct_binary, cache_entry.path()).await?;
                debug!("Binary copied successfully");
            } else {
                return Err(Error::BinaryNotFound {
                    tool: self.tool_name().to_string(),
                    expected: extracted_binary,
                });
            }
        } else {
            debug!("Found binary at expected location");
            // Copy binary to cache location
            debug!("Copying binary from {} to {}", extracted_binary.display(), cache_entry.path().display());
            tokio::fs::copy(&extracted_binary, cache_entry.path()).await?;
            debug!("Binary copied successfully");
        }


        // Make executable on Unix
        #[cfg(unix)]
        {
            debug!("Setting executable permissions on binary");
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(cache_entry.path()).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(cache_entry.path(), perms).await?;
            debug!("Executable permissions set");
        }

        debug!("Cached Ruff binary at {}", cache_entry.path().display());
        debug!("Binary exists: {}", cache_entry.path().exists());
        
        Ok(cache_entry.into_path_buf())
    }
}