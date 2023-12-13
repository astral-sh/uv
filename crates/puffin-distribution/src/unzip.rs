use std::io;
use std::io::{Read, Seek};
use std::path::Path;

use rayon::prelude::*;
use thiserror::Error;
use zip::result::ZipError;
use zip::ZipArchive;

use crate::download::BuiltWheel;
use crate::vendor::{CloneableSeekableReader, HasLength};
use crate::{DiskWheel, InMemoryWheel, LocalWheel};

#[derive(Debug, Error)]
pub enum UnzipError {
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub trait Unzip {
    /// Unzip a wheel into the target directory.
    fn unzip(&self, target: &Path) -> Result<(), UnzipError>;
}

impl Unzip for InMemoryWheel {
    fn unzip(&self, target: &Path) -> Result<(), UnzipError> {
        unzip_archive(std::io::Cursor::new(&self.buffer), target)
    }
}

impl Unzip for DiskWheel {
    fn unzip(&self, target: &Path) -> Result<(), UnzipError> {
        unzip_archive(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for BuiltWheel {
    fn unzip(&self, target: &Path) -> Result<(), UnzipError> {
        unzip_archive(fs_err::File::open(&self.path)?, target)
    }
}

impl Unzip for LocalWheel {
    fn unzip(&self, target: &Path) -> Result<(), UnzipError> {
        match self {
            LocalWheel::InMemory(wheel) => wheel.unzip(target),
            LocalWheel::Disk(wheel) => wheel.unzip(target),
            LocalWheel::Built(wheel) => wheel.unzip(target),
        }
    }
}

/// Unzip a zip archive into the target directory.
fn unzip_archive<R: Send + Read + Seek + HasLength>(
    reader: R,
    target: &Path,
) -> Result<(), UnzipError> {
    // Unzip in parallel.
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;
    (0..archive.len())
        .par_bridge()
        .map(|file_number| {
            let mut archive = archive.clone();
            let mut file = archive.by_index(file_number)?;

            // Determine the path of the file within the wheel.
            let file_path = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => return Ok(()),
            };

            // Create necessary parent directories.
            let path = target.join(file_path);
            if file.is_dir() {
                fs_err::create_dir_all(path)?;
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent)?;
            }

            // Write the file.
            let mut outfile = fs_err::File::create(&path)?;
            std::io::copy(&mut file, &mut outfile)?;

            // Set permissions.
            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&path, Permissions::from_mode(mode))?;
                }
            }

            Ok(())
        })
        .collect::<Result<_, UnzipError>>()
}
