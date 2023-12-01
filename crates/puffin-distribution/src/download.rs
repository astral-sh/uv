use std::path::{Path, PathBuf};

use anyhow::Result;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::{Dist, SourceDist};
use install_wheel_rs::find_dist_info;
use pypi_types::Metadata21;

use crate::error::Error;

/// A downloaded wheel that's stored in-memory.
#[derive(Debug, Clone)]
pub struct InMemoryWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename
    pub(crate) filename: WheelFilename,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
    /// The path where the downloaded wheel would have been stored, if it wasn't an in-memory wheel.
    pub(crate) path: PathBuf,
}

/// A downloaded wheel that's stored on-disk.
#[derive(Debug, Clone)]
pub struct DiskWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename
    pub(crate) filename: WheelFilename,
    /// The path to the downloaded wheel.
    pub(crate) path: PathBuf,
}

/// A wheel built from a source distribution that's stored on-disk.
#[derive(Debug, Clone)]
pub struct BuiltWheel {
    /// The remote source distribution from which this wheel was built.
    pub(crate) dist: Dist,
    /// The parsed filename
    pub(crate) filename: WheelFilename,
    /// The path to the built wheel.
    pub(crate) path: PathBuf,
}

/// A downloaded or built wheel.
#[derive(Debug, Clone)]
pub enum LocalWheel {
    InMemory(InMemoryWheel),
    Disk(DiskWheel),
    Built(BuiltWheel),
}

impl LocalWheel {
    pub fn path(&self) -> &Path {
        match self {
            LocalWheel::InMemory(wheel) => &wheel.path,
            LocalWheel::Disk(wheel) => &wheel.path,
            LocalWheel::Built(wheel) => &wheel.path,
        }
    }
}

/// A downloaded source distribution.
#[derive(Debug, Clone)]
pub struct SourceDistDownload {
    /// The remote distribution from which this source distribution was downloaded.
    pub(crate) dist: SourceDist,
    /// The path to the downloaded archive or directory.
    pub(crate) sdist_file: PathBuf,
    /// The subdirectory within the archive or directory.
    pub(crate) subdirectory: Option<PathBuf>,
}

/// A downloaded distribution, either a wheel or a source distribution.
#[derive(Debug)]
pub enum Download {
    Wheel(LocalWheel),
    SourceDist(SourceDistDownload),
}

impl std::fmt::Display for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Download::Wheel(wheel) => write!(f, "{wheel}"),
            Download::SourceDist(sdist) => write!(f, "{sdist}"),
        }
    }
}

impl std::fmt::Display for LocalWheel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalWheel::InMemory(wheel) => write!(f, "{} from {}", wheel.filename, wheel.dist),
            LocalWheel::Disk(wheel) => write!(f, "{} from {}", wheel.filename, wheel.dist),
            LocalWheel::Built(wheel) => write!(f, "{} from {}", wheel.filename, wheel.dist),
        }
    }
}

impl std::fmt::Display for SourceDistDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dist)
    }
}

impl InMemoryWheel {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        let mut archive = ZipArchive::new(std::io::Cursor::new(&self.buffer))?;
        let dist_info_dir = find_dist_info(
            &self.filename,
            archive.file_names().map(|name| (name, name)),
        )
        .map_err(|err| {
            Error::DistInfo(Box::new(self.filename.clone()), self.dist.to_string(), err)
        })?
        .1;
        let dist_info =
            std::io::read_to_string(archive.by_name(&format!("{dist_info_dir}/METADATA"))?)?;
        Ok(Metadata21::parse(dist_info.as_bytes())?)
    }
}

impl DiskWheel {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        let mut archive = ZipArchive::new(fs_err::File::open(&self.path)?)?;
        let dist_info_dir = find_dist_info(
            &self.filename,
            archive.file_names().map(|name| (name, name)),
        )
        .map_err(|err| {
            Error::DistInfo(Box::new(self.filename.clone()), self.dist.to_string(), err)
        })?
        .1;
        let dist_info =
            std::io::read_to_string(archive.by_name(&format!("{dist_info_dir}/METADATA"))?)?;
        Ok(Metadata21::parse(dist_info.as_bytes())?)
    }
}

impl BuiltWheel {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        let mut archive = ZipArchive::new(fs_err::File::open(&self.path)?)?;
        let dist_info_dir = find_dist_info(
            &self.filename,
            archive.file_names().map(|name| (name, name)),
        )
        .map_err(|err| {
            Error::DistInfo(Box::new(self.filename.clone()), self.dist.to_string(), err)
        })?
        .1;
        let dist_info =
            std::io::read_to_string(archive.by_name(&format!("{dist_info_dir}/METADATA"))?)?;
        Ok(Metadata21::parse(dist_info.as_bytes())?)
    }
}

impl LocalWheel {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        match self {
            LocalWheel::InMemory(wheel) => wheel.read_dist_info(),
            LocalWheel::Disk(wheel) => wheel.read_dist_info(),
            LocalWheel::Built(wheel) => wheel.read_dist_info(),
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

impl LocalWheel {
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

impl SourceDistDownload {
    /// Return the [`Dist`] from which this source distribution was downloaded.
    pub fn remote(&self) -> &SourceDist {
        &self.dist
    }
}
