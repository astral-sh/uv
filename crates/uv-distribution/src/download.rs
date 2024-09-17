use std::path::{Path, PathBuf};

use crate::Error;
use distribution_filename::WheelFilename;
use distribution_types::{CachedDist, Dist, Hashed};
use pypi_types::{HashDigest, Metadata23};
use uv_metadata::read_flat_wheel_metadata;

use uv_cache_info::CacheInfo;

/// A locally available wheel.
#[derive(Debug, Clone)]
pub struct LocalWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The canonicalized path in the cache directory to which the wheel was downloaded.
    /// Typically, a directory within the archive bucket.
    pub(crate) archive: PathBuf,
    /// The cache index of the wheel.
    pub(crate) cache: CacheInfo,
    /// The computed hashes of the wheel.
    pub(crate) hashes: Vec<HashDigest>,
}

impl LocalWheel {
    /// Return the path to the downloaded wheel's entry in the cache.
    pub fn target(&self) -> &Path {
        &self.archive
    }

    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> &WheelFilename {
        &self.filename
    }

    /// Read the [`Metadata23`] from a wheel.
    pub fn metadata(&self) -> Result<Metadata23, Error> {
        read_flat_wheel_metadata(&self.filename, &self.archive)
            .map_err(|err| Error::WheelMetadata(self.archive.clone(), Box::new(err)))
    }
}

impl Hashed for LocalWheel {
    fn hashes(&self) -> &[HashDigest] {
        &self.hashes
    }
}

/// Convert a [`LocalWheel`] into a [`CachedDist`].
impl From<LocalWheel> for CachedDist {
    fn from(wheel: LocalWheel) -> CachedDist {
        CachedDist::from_remote(
            wheel.dist,
            wheel.filename,
            wheel.hashes,
            wheel.cache,
            wheel.archive,
        )
    }
}

impl std::fmt::Display for LocalWheel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote())
    }
}
