use std::path::{Path, PathBuf};

use rayon::prelude::*;
use zip::result::ZipError;
use zip::ZipArchive;

pub use crate::vendor::{CloneableSeekableReader, HasLength};

mod vendor;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Unsupported archive type: {0}")]
    UnsupportedArchive(PathBuf),
    #[error(
        "The top level of the archive must only contain a list directory, but it contains: {0:?}"
    )]
    InvalidArchive(Vec<fs_err::DirEntry>),
}

/// Unzip a `.zip` archive into the target directory.
pub fn unzip_archive<R: Send + std::io::Read + std::io::Seek + HasLength>(
    reader: R,
    target: &Path,
) -> Result<(), Error> {
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
        .collect::<Result<_, Error>>()
}

/// Extract a `.zip` or `.tar.gz` archive into the target directory.
pub fn extract_archive(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<(), Error> {
    // .zip
    if source
        .as_ref()
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        unzip_archive(fs_err::File::open(source.as_ref())?, target.as_ref())?;
        return Ok(());
    }

    // .tar.gz
    if source
        .as_ref()
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
    {
        if source.as_ref().file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
        }) {
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(fs_err::File::open(
                source.as_ref(),
            )?));
            // https://github.com/alexcrichton/tar-rs/issues/349
            archive.set_preserve_mtime(false);
            archive.unpack(target)?;
            return Ok(());
        }
    }

    Err(Error::UnsupportedArchive(source.as_ref().to_path_buf()))
}

/// Extract a source distribution into the target directory.
///
/// Returns the path to the top-level directory of the source distribution.
pub fn extract_source(
    source: impl AsRef<Path>,
    target: impl AsRef<Path>,
) -> Result<PathBuf, Error> {
    extract_archive(&source, &target)?;

    // > A .tar.gz source distribution (sdist) contains a single top-level directory called
    // > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
    // TODO(konstin): Verify the name of the directory.
    let top_level =
        fs_err::read_dir(target.as_ref())?.collect::<std::io::Result<Vec<fs_err::DirEntry>>>()?;
    let [root] = top_level.as_slice() else {
        return Err(Error::InvalidArchive(top_level));
    };

    Ok(root.path())
}
