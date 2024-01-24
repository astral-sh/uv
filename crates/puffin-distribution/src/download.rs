use std::path::{Path, PathBuf};

use distribution_filename::WheelFilename;
use distribution_types::{CachedDist, Dist};

/// A wheel that's been unzipped while downloading
#[derive(Debug, Clone)]
pub struct UnzippedWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The path in the cache dir where the wheel was downloaded.
    pub(crate) target: PathBuf,
}

/// A downloaded wheel that's stored on-disk.
#[derive(Debug, Clone)]
pub struct DiskWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The path to the downloaded wheel.
    pub(crate) path: PathBuf,
    /// The expected path to the downloaded wheel's entry in the cache.
    pub(crate) target: PathBuf,
}

/// A wheel built from a source distribution that's stored on-disk.
#[derive(Debug, Clone)]
pub struct BuiltWheel {
    /// The remote source distribution from which this wheel was built.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The path to the built wheel.
    pub(crate) path: PathBuf,
    /// The expected path to the downloaded wheel's entry in the cache.
    pub(crate) target: PathBuf,
}

/// A downloaded or built wheel.
#[derive(Debug, Clone)]
pub enum LocalWheel {
    Unzipped(UnzippedWheel),
    Disk(DiskWheel),
    Built(BuiltWheel),
}

impl LocalWheel {
    /// Return the path to the downloaded wheel's entry in the cache.
    pub fn target(&self) -> &Path {
        match self {
            LocalWheel::Unzipped(wheel) => &wheel.target,
            LocalWheel::Disk(wheel) => &wheel.target,
            LocalWheel::Built(wheel) => &wheel.target,
        }
    }

    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        match self {
            LocalWheel::Unzipped(wheel) => wheel.remote(),
            LocalWheel::Disk(wheel) => wheel.remote(),
            LocalWheel::Built(wheel) => wheel.remote(),
        }
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> &WheelFilename {
        match self {
            LocalWheel::Unzipped(wheel) => &wheel.filename,
            LocalWheel::Disk(wheel) => &wheel.filename,
            LocalWheel::Built(wheel) => &wheel.filename,
        }
    }

    /// Convert a [`LocalWheel`] into a [`CachedDist`].
    pub fn into_cached_dist(self) -> CachedDist {
        match self {
            LocalWheel::Unzipped(wheel) => {
                CachedDist::from_remote(wheel.dist, wheel.filename, wheel.target)
            }
            LocalWheel::Disk(wheel) => {
                CachedDist::from_remote(wheel.dist, wheel.filename, wheel.target)
            }
            LocalWheel::Built(wheel) => {
                CachedDist::from_remote(wheel.dist, wheel.filename, wheel.target)
            }
        }
    }
}

impl UnzippedWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }
}

impl DiskWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }
}

impl BuiltWheel {
    /// Return the [`Dist`] from which this source distribution that this wheel was built from was
    /// downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }
}

impl std::fmt::Display for LocalWheel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote())
    }
}
