use std::path::Path;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    CachedDirectUrlDist, CachedRegistryDist, DirectUrlSourceDist, DirectorySourceDist,
    GitSourceDist, Hashed, PathSourceDist,
};
use uv_pypi_types::{HashDigest, HashDigests, VerbatimParsedUrl};

use crate::archive::Archive;
use crate::{HttpArchivePointer, LocalArchivePointer};

#[derive(Debug, Clone)]
pub struct CachedWheel {
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The [`CacheEntry`] for the wheel.
    pub entry: CacheEntry,
    /// The [`HashDigest`]s for the wheel.
    pub hashes: HashDigests,
    /// The [`CacheInfo`] for the wheel.
    pub cache_info: CacheInfo,
}

impl CachedWheel {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn from_built_source(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Convert to a cached wheel.
        let archive = cache.resolve_link(path).ok()?;
        let entry = CacheEntry::from_path(archive);
        let hashes = HashDigests::empty();
        let cache_info = CacheInfo::default();
        Some(Self {
            filename,
            entry,
            hashes,
            cache_info,
        })
    }

    /// Convert a [`CachedWheel`] into a [`CachedRegistryDist`].
    pub fn into_registry_dist(self) -> CachedRegistryDist {
        CachedRegistryDist {
            filename: self.filename,
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
            cache_info: self.cache_info,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`] by merging in the given
    /// [`DirectUrlSourceDist`].
    pub fn into_url_dist(self, dist: &DirectUrlSourceDist) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url: VerbatimParsedUrl {
                parsed_url: dist.parsed_url(),
                verbatim: dist.url.clone(),
            },
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
            cache_info: self.cache_info,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`] by merging in the given
    /// [`PathSourceDist`].
    pub fn into_path_dist(self, dist: &PathSourceDist) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url: VerbatimParsedUrl {
                parsed_url: dist.parsed_url(),
                verbatim: dist.url.clone(),
            },
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
            cache_info: self.cache_info,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`] by merging in the given
    /// [`DirectorySourceDist`].
    pub fn into_directory_dist(self, dist: &DirectorySourceDist) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url: VerbatimParsedUrl {
                parsed_url: dist.parsed_url(),
                verbatim: dist.url.clone(),
            },
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
            cache_info: self.cache_info,
        }
    }

    /// Convert a [`CachedWheel`] into a [`CachedDirectUrlDist`] by merging in the given
    /// [`GitSourceDist`].
    pub fn into_git_dist(self, dist: &GitSourceDist) -> CachedDirectUrlDist {
        CachedDirectUrlDist {
            filename: self.filename,
            url: VerbatimParsedUrl {
                parsed_url: dist.parsed_url(),
                verbatim: dist.url.clone(),
            },
            path: self.entry.into_path_buf(),
            hashes: self.hashes,
            cache_info: self.cache_info,
        }
    }

    /// Read a cached wheel from a `.http` pointer
    pub fn from_http_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Read the pointer.
        let pointer = HttpArchivePointer::read_from(path).ok()??;
        let cache_info = pointer.to_cache_info();
        let archive = pointer.into_archive();

        // Ignore stale pointers.
        if !archive.exists(cache) {
            return None;
        }

        let Archive { id, hashes, .. } = archive;
        let entry = cache.entry(CacheBucket::Archive, "", id);

        // Convert to a cached wheel.
        Some(Self {
            filename: archive.filename,
            entry,
            hashes,
            cache_info,
        })
    }

    /// Read a cached wheel from a `.rev` pointer
    pub fn from_local_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Read the pointer.
        let pointer = LocalArchivePointer::read_from(path).ok()??;
        let cache_info = pointer.to_cache_info();
        let archive = pointer.into_archive();

        // Ignore stale pointers.
        if !archive.exists(cache) {
            return None;
        }

        let Archive { id, hashes, .. } = archive;
        let entry = cache.entry(CacheBucket::Archive, "", id);

        // Convert to a cached wheel.
        Some(Self {
            filename: archive.filename,
            entry,
            hashes,
            cache_info,
        })
    }

    #[must_use]
    pub fn with_cache_info(mut self, cache_info: CacheInfo) -> Self {
        self.cache_info = cache_info;
        self
    }
}

impl Hashed for CachedWheel {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
