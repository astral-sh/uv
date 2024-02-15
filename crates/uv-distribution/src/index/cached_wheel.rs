use std::path::Path;

use distribution_filename::WheelFilename;
use distribution_types::{CachedDirectUrlDist, CachedRegistryDist};
use pep508_rs::VerbatimUrl;
use uv_cache::CacheEntry;

#[derive(Debug, Clone)]
pub struct CachedWheel {
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The [`CacheEntry`] for the wheel.
    pub entry: CacheEntry,
}

impl CachedWheel {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn from_path(path: &Path) -> Option<Self> {
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;
        let archive = path.canonicalize().ok()?;
        let entry = CacheEntry::from_path(archive);
        Some(Self { filename, entry })
    }

    /// Convert a [`CachedWheel`] into a [`CachedRegistryDist`].
    pub fn into_registry_dist(self) -> CachedRegistryDist {
        CachedRegistryDist {
            filename: self.filename,
            path: self.entry.into_path_buf(),
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`].
    pub fn into_url_dist(self, url: VerbatimUrl) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url,
            path: self.entry.into_path_buf(),
            editable: false,
        }
    }
}
