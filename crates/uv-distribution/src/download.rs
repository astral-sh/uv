use std::path::{Path, PathBuf};

use distribution_filename::WheelFilename;
use distribution_types::{CachedDist, Dist};
use pypi_types::Metadata23;

use crate::Error;

/// A wheel that's been unzipped while downloading
#[derive(Debug, Clone)]
pub struct UnzippedWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The canonicalized path in the cache directory to which the wheel was downloaded.
    /// Typically, a directory within the archive bucket.
    pub(crate) archive: PathBuf,
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
    /// Typically, a symlink within the wheels or built wheels bucket.
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
    /// Typically, a symlink within the wheels or built wheels bucket.
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
            Self::Unzipped(wheel) => &wheel.archive,
            Self::Disk(wheel) => &wheel.target,
            Self::Built(wheel) => &wheel.target,
        }
    }

    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        match self {
            Self::Unzipped(wheel) => wheel.remote(),
            Self::Disk(wheel) => wheel.remote(),
            Self::Built(wheel) => wheel.remote(),
        }
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> &WheelFilename {
        match self {
            Self::Unzipped(wheel) => &wheel.filename,
            Self::Disk(wheel) => &wheel.filename,
            Self::Built(wheel) => &wheel.filename,
        }
    }

    /// Convert a [`LocalWheel`] into a [`CachedDist`].
    pub fn into_cached_dist(self, archive: PathBuf) -> CachedDist {
        match self {
            Self::Unzipped(wheel) => CachedDist::from_remote(wheel.dist, wheel.filename, archive),
            Self::Disk(wheel) => CachedDist::from_remote(wheel.dist, wheel.filename, archive),
            Self::Built(wheel) => CachedDist::from_remote(wheel.dist, wheel.filename, archive),
        }
    }

    /// Read the [`Metadata23`] from a wheel.
    pub fn metadata(&self) -> Result<Metadata23, Error> {
        match self {
            Self::Unzipped(wheel) => read_flat_wheel_metadata(&wheel.filename, &wheel.archive),
            Self::Disk(wheel) => read_built_wheel_metadata(&wheel.filename, &wheel.path),
            Self::Built(wheel) => read_built_wheel_metadata(&wheel.filename, &wheel.path),
        }
    }
}

impl UnzippedWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }

    /// Convert an [`UnzippedWheel`] into a [`CachedDist`].
    pub fn into_cached_dist(self) -> CachedDist {
        CachedDist::from_remote(self.dist, self.filename, self.archive)
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

/// Read the [`Metadata23`] from a built wheel.
fn read_built_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl AsRef<Path>,
) -> Result<Metadata23, Error> {
    let file = fs_err::File::open(wheel.as_ref()).map_err(Error::CacheRead)?;
    let reader = std::io::BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)?;
    let metadata = install_wheel_rs::metadata::read_archive_metadata(filename, &mut archive)?;
    Ok(Metadata23::parse_metadata(&metadata)?)
}

/// Read the [`Metadata23`] from an unzipped wheel.
fn read_flat_wheel_metadata(
    filename: &WheelFilename,
    wheel: impl AsRef<Path>,
) -> Result<Metadata23, Error> {
    let dist_info = install_wheel_rs::metadata::find_flat_dist_info(filename, &wheel)?;
    let metadata = install_wheel_rs::metadata::read_dist_info_metadata(&dist_info, &wheel)?;
    Ok(Metadata23::parse_metadata(&metadata)?)
}
