use std::path::PathBuf;
use std::str::FromStr;

use uv_cache::CacheShard;
use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::Hashed;
use uv_fs::files;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Tags;
use uv_pypi_types::{HashDigest, HashDigests};

/// The information about the wheel we either just built or got from the cache.
#[derive(Debug, Clone)]
pub(crate) struct BuiltWheelMetadata {
    /// The path to the built wheel.
    pub(crate) path: PathBuf,
    /// The expected path to the downloaded wheel's entry in the cache.
    pub(crate) target: PathBuf,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The computed hashes of the source distribution from which the wheel was built.
    pub(crate) hashes: HashDigests,
    /// The cache information for the underlying source distribution.
    pub(crate) cache_info: CacheInfo,
}

impl BuiltWheelMetadata {
    /// Find a compatible wheel in the cache.
    pub(crate) fn find_in_cache(
        tags: &Tags,
        cache_shard: &CacheShard,
    ) -> Result<Option<Self>, std::io::Error> {
        for file in files(cache_shard)? {
            if let Some(metadata) = Self::from_path(file, cache_shard) {
                // Validate that the wheel is compatible with the target platform.
                if metadata.filename.is_compatible(tags) {
                    return Ok(Some(metadata));
                }
            }
        }
        Ok(None)
    }

    /// Try to parse a distribution from a cached directory name (like `typing-extensions-4.8.0-py3-none-any.whl`).
    fn from_path(path: PathBuf, cache_shard: &CacheShard) -> Option<Self> {
        let filename = path.file_name()?.to_str()?;
        let filename = WheelFilename::from_str(filename).ok()?;
        Some(Self {
            target: cache_shard.join(filename.stem()),
            path,
            filename,
            cache_info: CacheInfo::default(),
            hashes: HashDigests::empty(),
        })
    }

    #[must_use]
    pub(crate) fn with_hashes(mut self, hashes: HashDigests) -> Self {
        self.hashes = hashes;
        self
    }

    /// Returns `true` if the wheel matches the given package name and version.
    pub(crate) fn matches(&self, name: Option<&PackageName>, version: Option<&Version>) -> bool {
        name.is_none_or(|name| self.filename.name == *name)
            && version.is_none_or(|version| self.filename.version == *version)
    }
}

impl Hashed for BuiltWheelMetadata {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}
