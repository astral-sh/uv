use std::path::PathBuf;

use tracing::warn;

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use puffin_cache::CacheEntry;
use pypi_types::Metadata21;

use crate::source::manifest::{DiskFilenameAndMetadata, Manifest};

/// The information about the wheel we either just built or got from the cache.
#[derive(Debug, Clone)]
pub struct BuiltWheelMetadata {
    /// The path to the built wheel.
    pub(crate) path: PathBuf,
    /// The expected path to the downloaded wheel's entry in the cache.
    pub(crate) target: PathBuf,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The metadata of the built wheel.
    pub(crate) metadata: Metadata21,
}

impl BuiltWheelMetadata {
    /// Find a compatible wheel in the cache based on the given manifest.
    pub(crate) fn find_in_cache(
        tags: &Tags,
        manifest: &Manifest,
        cache_entry: &CacheEntry,
    ) -> Option<Self> {
        // Find a compatible cache entry in the manifest.
        let (filename, cached_dist) = manifest.find_compatible(tags)?;
        let metadata = Self::from_cached(filename.clone(), cached_dist.clone(), cache_entry);

        // Validate that the wheel exists on disk.
        if !metadata.path.is_file() {
            warn!(
                "Wheel `{}` is present in the manifest, but not on disk",
                metadata.path.display()
            );
            return None;
        }

        Some(metadata)
    }

    /// Create a [`BuiltWheelMetadata`] from a cached entry.
    pub(crate) fn from_cached(
        filename: WheelFilename,
        cached_dist: DiskFilenameAndMetadata,
        cache_entry: &CacheEntry,
    ) -> Self {
        Self {
            path: cache_entry.dir().join(&cached_dist.disk_filename),
            target: cache_entry.dir().join(filename.stem()),
            filename,
            metadata: cached_dist.metadata,
        }
    }
}
