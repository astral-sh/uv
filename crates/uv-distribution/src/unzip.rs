use std::path::Path;

use uv_extract::Error;

use crate::download::BuiltWheel;
use crate::{DiskWheel, LocalWheel};

pub trait Unzip {
    /// Unzip a wheel into the target directory.
    fn unzip(&self, target: &Path) -> Result<(), Error>;
}

impl Unzip for DiskWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        uv_extract::unzip(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for BuiltWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        uv_extract::unzip(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for LocalWheel {
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        match self {
            LocalWheel::Unzipped(_) => Ok(()),
            LocalWheel::Disk(wheel) => wheel.unzip(target),
            LocalWheel::Built(wheel) => wheel.unzip(target),
        }
    }
}
