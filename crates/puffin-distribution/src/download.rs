use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use tempfile::TempDir;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::{Dist, RemoteSource};
use install_wheel_rs::find_dist_info;
use pypi_types::Metadata21;

use crate::error::Error;

/// A downloaded wheel that's stored in-memory.
#[derive(Debug)]
pub struct InMemoryWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The contents of the wheel.
    pub(crate) buffer: Vec<u8>,
}

/// A downloaded wheel that's stored on-disk.
#[derive(Debug)]
pub struct DiskWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The path to the downloaded wheel.
    pub(crate) path: PathBuf,
    /// The download location, to be dropped after use.
    #[allow(dead_code)]
    pub(crate) temp_dir: Option<TempDir>,
}

/// A downloaded wheel.
#[derive(Debug)]
pub enum WheelDownload {
    InMemory(InMemoryWheel),
    Disk(DiskWheel),
}

/// A downloaded source distribution.
#[derive(Debug)]
pub struct SourceDistDownload {
    /// The remote distribution from which this source distribution was downloaded.
    pub(crate) dist: Dist,
    /// The path to the downloaded archive or directory.
    pub(crate) sdist_file: PathBuf,
    /// The subdirectory within the archive or directory.
    pub(crate) subdirectory: Option<PathBuf>,
    /// We can't use source dist archives, we build them into wheels which we persist and then drop
    /// the source distribution. This field is non for git dependencies, which we keep in the cache.
    #[allow(dead_code)]
    pub(crate) temp_dir: Option<TempDir>,
}

/// A downloaded distribution, either a wheel or a source distribution.
#[derive(Debug)]
pub enum Download {
    Wheel(WheelDownload),
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

impl std::fmt::Display for WheelDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WheelDownload::InMemory(wheel) => write!(f, "{}", wheel.dist),
            WheelDownload::Disk(wheel) => write!(f, "{}", wheel.dist),
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
        let filename = self
            .filename()
            .map_err(|err| Error::FilenameParse(self.dist.to_string(), err))?;
        let dist_info_dir =
            find_dist_info(&filename, archive.file_names().map(|name| (name, name)))
                .map_err(|err| Error::DistInfo(self.dist.to_string(), err))?
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
        let filename = self
            .filename()
            .map_err(|err| Error::FilenameParse(self.dist.to_string(), err))?;
        let dist_info_dir =
            find_dist_info(&filename, archive.file_names().map(|name| (name, name)))
                .map_err(|err| Error::DistInfo(self.dist.to_string(), err))?
                .1;
        let dist_info =
            std::io::read_to_string(archive.by_name(&format!("{dist_info_dir}/METADATA"))?)?;
        Ok(Metadata21::parse(dist_info.as_bytes())?)
    }
}

impl WheelDownload {
    /// Read the [`Metadata21`] from a wheel.
    pub fn read_dist_info(&self) -> Result<Metadata21, Error> {
        match self {
            WheelDownload::InMemory(wheel) => wheel.read_dist_info(),
            WheelDownload::Disk(wheel) => wheel.read_dist_info(),
        }
    }
}

impl DiskWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> Result<WheelFilename> {
        // If the wheel was downloaded to disk, it's either a download of a remote wheel, or a
        // built source distribution, both of which imply a valid wheel filename.
        let filename = WheelFilename::from_str(
            self.path
                .file_name()
                .context("Missing filename")?
                .to_str()
                .context("Invalid filename")?,
        )?;
        Ok(filename)
    }
}

impl InMemoryWheel {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> Result<WheelFilename> {
        // If the wheel is an in-memory buffer, it's assumed that the underlying distribution is
        // itself a wheel, which in turn requires that the filename be parseable.
        let filename = WheelFilename::from_str(self.dist.filename()?)?;
        Ok(filename)
    }
}

impl WheelDownload {
    /// Return the [`Dist`] from which this wheel was downloaded.
    pub fn remote(&self) -> &Dist {
        match self {
            WheelDownload::InMemory(wheel) => wheel.remote(),
            WheelDownload::Disk(wheel) => wheel.remote(),
        }
    }

    /// Return the [`WheelFilename`] of this wheel.
    pub fn filename(&self) -> Result<WheelFilename> {
        match self {
            WheelDownload::InMemory(wheel) => wheel.filename(),
            WheelDownload::Disk(wheel) => wheel.filename(),
        }
    }
}

impl SourceDistDownload {
    /// Return the [`Dist`] from which this source distribution was downloaded.
    pub fn remote(&self) -> &Dist {
        &self.dist
    }
}
