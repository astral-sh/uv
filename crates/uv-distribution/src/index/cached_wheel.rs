use std::path::Path;

use distribution_filename::WheelFilename;
use distribution_types::{CachedDirectUrlDist, CachedRegistryDist, Hashed};
use pep508_rs::VerbatimUrl;
use pypi_types::HashDigest;
use uv_cache::{CacheEntry, CachedByTimestamp};
use uv_client::DataWithCachePolicy;

use crate::archive::Archive;

#[derive(Debug, Clone)]
pub struct CachedWheel {
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The [`CacheEntry`] for the wheel.
    pub entry: CacheEntry,
    /// The [`HashDigest`]s for the wheel.
    pub hashes: Vec<HashDigest>,
}

impl CachedWheel {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn from_built_source(path: &Path) -> Option<Self> {
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;
        let archive = path.canonicalize().ok()?;
        let entry = CacheEntry::from_path(archive);
        let hashes = Vec::new();
        Some(Self {
            filename,
            entry,
            hashes,
        })
    }

    /// Convert a [`CachedWheel`] into a [`CachedRegistryDist`].
    pub fn into_registry_dist(self) -> CachedRegistryDist {
        CachedRegistryDist {
            filename: self.filename,
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`].
    pub fn into_url_dist(self, url: VerbatimUrl) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url,
            path: self.entry.into_path_buf(),
            editable: false,
            hashes: self.hashes,
        }
    }

    /// Read a cached wheel from a `.http` pointer (e.g., `anyio-4.0.0-py3-none-any.http`).
    pub fn from_http_pointer(path: &Path) -> Option<Self> {
        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Read the pointer.
        let file = fs_err::File::open(path).ok()?;
        let data = DataWithCachePolicy::from_reader(file).ok()?.data;
        let archive = rmp_serde::from_slice::<Archive>(&data).ok()?;

        // Convert to a cached wheel.
        let entry = CacheEntry::from_path(archive.path);
        let hashes = archive.hashes;
        Some(Self {
            filename,
            entry,
            hashes,
        })
    }

    /// Read a cached wheel from a `.rev` pointer (e.g., `anyio-4.0.0-py3-none-any.rev`).
    pub fn from_revision_pointer(path: &Path) -> Option<Self> {
        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Read the pointer.
        let cached = fs_err::read(path).ok()?;
        let archive = rmp_serde::from_slice::<CachedByTimestamp<Archive>>(&cached)
            .ok()?
            .data;

        // Convert to a cached wheel.
        let entry = CacheEntry::from_path(archive.path);
        let hashes = archive.hashes;
        Some(Self {
            filename,
            entry,
            hashes,
        })
    }
}

impl Hashed for CachedWheel {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}
