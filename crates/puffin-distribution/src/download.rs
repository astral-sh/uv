use std::path::{Path, PathBuf};

use anyhow::Result;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::Dist;
use install_wheel_rs::read_dist_info;
use pypi_types::Metadata21;

use crate::error::Error;

/// A downloaded wheel that's stored in-memory.
#[derive(Debug, Clone)]
pub struct InMemoryWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
    /// The expected path to the downloaded wheel's entry in the cache.
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
    InMemory(InMemoryWheel),
    Disk(DiskWheel),
    Built(BuiltWheel),
}

impl LocalWheel {
    /// Return the path to the downloaded wheel's entry in the cache.
    pub fn target(&self) -> &Path {
        match self {
            LocalWheel::InMemory(wheel) => &wheel.target,
            LocalWheel::Disk(wheel) => &wheel.target,
            LocalWheel::Built(wheel) => &wheel.target,
        }
    }

    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        match self {
            LocalWheel::InMemory(wheel) => wheel.remote(),
            LocalWheel::Disk(wheel) => wheel.remote(),
            LocalWheel::Built(wheel) => wheel.remote(),
        }
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> &WheelFilename {
        match self {
            LocalWheel::InMemory(wheel) => &wheel.filename,
            LocalWheel::Disk(wheel) => &wheel.filename,
            LocalWheel::Built(wheel) => &wheel.filename,
        }
    }
}

impl DiskWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }
}

impl InMemoryWheel {
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

impl BuiltWheel {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        let mut archive = ZipArchive::new(fs_err::File::open(&self.path)?)?;
        let dist_info = read_dist_info(&self.filename, &mut archive).map_err(|err| {
            Error::DistInfo(Box::new(self.filename.clone()), self.dist.to_string(), err)
        })?;
        Ok(Metadata21::parse(&dist_info)?)
    }
}
