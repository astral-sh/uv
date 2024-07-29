use std::path::Path;

use distribution_filename::WheelFilename;
use distribution_types::{CachedDirectUrlDist, CachedRegistryDist, Hashed};
use pep508_rs::VerbatimUrl;
use pypi_types::HashDigest;
use uv_cache::{Cache, CacheBucket, CacheEntry};

use crate::archive::Archive;
use crate::{HttpArchivePointer, LocalArchivePointer};

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
    pub fn from_built_source(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();

        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Convert to a cached wheel.
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

    /// Convert a [`CachedWheel`] into an editable [`CachedDirectUrlDist`].
    pub fn into_editable(self, url: VerbatimUrl) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url,
            path: self.entry.into_path_buf(),
            editable: true,
            hashes: self.hashes,
        }
    }

    /// Read a cached wheel from a `.http` pointer (e.g., `anyio-4.0.0-py3-none-any.http`).
    pub fn from_http_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Read the pointer.
        let pointer = HttpArchivePointer::read_from(path).ok()??;
        let Archive { id, hashes } = pointer.into_archive();

        // Convert to a cached wheel.
        let entry = cache.entry(CacheBucket::Archive, "", id);
        Some(Self {
            filename,
            entry,
            hashes,
        })
    }

    /// Read a cached wheel from a `.rev` pointer (e.g., `anyio-4.0.0-py3-none-any.rev`).
    pub fn from_local_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Read the pointer.
        let pointer = LocalArchivePointer::read_from(path).ok()??;
        let Archive { id, hashes } = pointer.into_archive();

        // Convert to a cached wheel.
        let entry = cache.entry(CacheBucket::Archive, "", id);
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
