use std::path::Path;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{
    BuildInfo, CachedDirectUrlDist, CachedRegistryDist, DirectUrlSourceDist, DirectorySourceDist,
    GitSourceDist, Hashed, PathSourceDist,
};
use uv_pypi_types::{HashDigest, HashDigests, VerbatimParsedUrl};

use crate::archive::Archive;
use crate::{HttpArchivePointer, LocalArchivePointer};

#[derive(Debug, Clone)]
pub struct ResolvedWheel {
    /// The filename of the wheel.
    pub filename: WheelFilename,
    /// The [`CacheEntry`] for the wheel.
    pub entry: CacheEntry,
}

impl ResolvedWheel {
    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any`).
    pub fn from_built_source(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Determine the wheel filename.
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_stem(filename).ok()?;

        // Convert to a cached wheel.
        let archive = cache.resolve_link(path).ok()?;
        let entry = CacheEntry::from_path(archive);
        Some(Self { filename, entry })
    }
}

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
    /// The [`BuildInfo`] for the wheel, if it was built.
    pub build_info: Option<BuildInfo>,
}

impl CachedWheel {
    /// Create a [`CachedWheel`] from a [`ResolvedWheel`].
    pub fn from_entry(
        wheel: ResolvedWheel,
        hashes: HashDigests,
        cache_info: CacheInfo,
        build_info: BuildInfo,
    ) -> Self {
        Self {
            filename: wheel.filename,
            entry: wheel.entry,
            hashes,
            cache_info,
            build_info: Some(build_info),
        }
    }

    /// Read a cached wheel from a `.http` pointer
    pub fn from_http_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Read the pointer.
        let pointer = HttpArchivePointer::read_from(path).ok()??;
        let cache_info = pointer.to_cache_info();
        let build_info = pointer.to_build_info();
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
            build_info,
        })
    }

    /// Read a cached wheel from a `.rev` pointer
    pub fn from_local_pointer(path: impl AsRef<Path>, cache: &Cache) -> Option<Self> {
        let path = path.as_ref();

        // Read the pointer.
        let pointer = LocalArchivePointer::read_from(path).ok()??;
        let cache_info = pointer.to_cache_info();
        let build_info = pointer.to_build_info();
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
            build_info,
        })
    }

    /// Convert a [`CachedWheel`] into a [`CachedRegistryDist`].
    pub fn into_registry_dist(self) -> CachedRegistryDist {
        CachedRegistryDist {
            filename: self.filename,
            path: self.entry.into_path_buf().into_boxed_path(),
            hashes: self.hashes,
            cache_info: self.cache_info,
            build_info: self.build_info,
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
            path: self.entry.into_path_buf().into_boxed_path(),
            hashes: self.hashes,
            cache_info: self.cache_info,
            build_info: self.build_info,
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
            path: self.entry.into_path_buf().into_boxed_path(),
            hashes: self.hashes,
            cache_info: self.cache_info,
            build_info: self.build_info,
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
            path: self.entry.into_path_buf().into_boxed_path(),
            hashes: self.hashes,
            cache_info: self.cache_info,
            build_info: self.build_info,
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
            path: self.entry.into_path_buf().into_boxed_path(),
            hashes: self.hashes,
            cache_info: self.cache_info,
            build_info: self.build_info,
        }
    }
}

impl Hashed for CachedWheel {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
