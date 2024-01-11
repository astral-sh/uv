use std::path::Path;

use puffin_extract::{unzip_archive, Error};

use crate::download::BuiltWheel;
use crate::{DiskWheel, InMemoryWheel, LocalWheel};

pub trait Unzip {
    /// Unzip a wheel into the target directory.
    fn unzip(&self, target: &Path) -> Result<(), Error>;
}

impl Unzip for InMemoryWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        unzip_archive(std::io::Cursor::new(&self.buffer), target)
    }
}

impl Unzip for DiskWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        unzip_archive(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for BuiltWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        unzip_archive(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for LocalWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        match self {
            LocalWheel::Unzipped(_) => Ok(()),
            LocalWheel::InMemory(wheel) => wheel.unzip(target),
            LocalWheel::Disk(wheel) => wheel.unzip(target),
            LocalWheel::Built(wheel) => wheel.unzip(target),
        }
    }
}
