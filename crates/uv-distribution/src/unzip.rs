use std::path::Path;

use tracing::instrument;

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
    #[instrument(skip_all, fields(filename=self.filename().to_string()))]
    fn unzip(&self, target: &Path) -> Result<(), Error> {
        match self {
            Self::Unzipped(_) => Ok(()),
            Self::Disk(wheel) => wheel.unzip(target),
            Self::Built(wheel) => wheel.unzip(target),
        }
    }
}
