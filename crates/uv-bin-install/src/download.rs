use std::path::PathBuf;

use async_trait::async_trait;

use uv_cache::Cache;
use uv_client::BaseClient;
use uv_platform::{Arch, Libc, Os};

use crate::Result;

/// Trait for downloading and caching binary tools.
#[async_trait]
pub trait BinaryDownloader: Send + Sync {
    /// The name of the tool being downloaded.
    fn tool_name(&self) -> &str;

    /// The default version to use when none is specified.
    fn default_version(&self) -> &str;

    /// Map platform information to a tool-specific platform identifier.
    fn platform_identifier(&self, os: &Os, arch: &Arch, libc: &Libc) -> Option<String>;

    /// Get the download URL for a specific version and platform.
    fn download_url(&self, version: &str, platform: &str) -> String;

    /// Get the archive extension for the platform.
    fn archive_extension(&self, os: &Os) -> &str;

    /// Get the expected binary name within the archive.
    fn binary_name(&self, os: &Os) -> &str;

    /// Get the expected directory structure within the archive.
    /// Returns None if the binary is at the root of the archive.
    fn archive_directory(&self, platform: &str) -> Option<String>;

    /// Download the binary for the specified version and platform.
    async fn download(
        &self,
        version: Option<&str>,
        os: &Os,
        arch: &Arch,
        libc: &Libc,
        client: &BaseClient,
        cache: &Cache,
    ) -> Result<PathBuf>;
}