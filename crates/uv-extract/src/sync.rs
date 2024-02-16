use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use rustc_hash::FxHashSet;
use zip::ZipArchive;

use crate::vendor::{CloneableSeekableReader, HasLength};
use crate::Error;

/// Unzip a `.zip` archive into the target directory.
pub fn unzip<R: Send + std::io::Read + std::io::Seek + HasLength>(
    reader: R,
    target: &Path,
) -> Result<(), Error> {
    // Unzip in parallel.
    let archive = ZipArchive::new(CloneableSeekableReader::new(reader))?;
    let directories = Mutex::new(FxHashSet::default());
    (0..archive.len())
        .par_bridge()
        .map(|file_number| {
            let mut archive = archive.clone();
            let mut file = archive.by_index(file_number)?;

            // Determine the path of the file within the wheel.
            let Some(enclosed_name) = file.enclosed_name() else {
                return Ok(());
            };

            // Create necessary parent directories.
            let path = target.join(enclosed_name);
            if file.is_dir() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(path.clone()) {
                    fs_err::create_dir_all(path)?;
                }
                return Ok(());
            }

            if let Some(parent) = path.parent() {
                let mut directories = directories.lock().unwrap();
                if directories.insert(parent.to_path_buf()) {
                    fs_err::create_dir_all(parent)?;
                }
            }

            // Create the file, with the correct permissions (on Unix).
            let mut options = OpenOptions::new();
            options.write(true);
            options.create_new(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;

                if let Some(mode) = file.unix_mode() {
                    options.mode(mode);
                }
            }

            // Copy the file contents.
            let mut outfile = options.open(&path)?;
            std::io::copy(&mut file, &mut outfile)?;

            Ok(())
        })
        .collect::<Result<_, Error>>()
}

/// Extract a `.zip` or `.tar.gz` archive into the target directory.
pub fn archive(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<(), Error> {
    // `.zip`
    if source
        .as_ref()
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        unzip(fs_err::File::open(source.as_ref())?, target.as_ref())?;
        return Ok(());
    }

    // `.tar.gz`
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

/// Extract the top-level directory from an unpacked archive.
///
/// The specification says:
/// > A .tar.gz source distribution (sdist) contains a single top-level directory called
/// > `{name}-{version}` (e.g. foo-1.0), containing the source files of the package.
///
/// This function returns the path to that top-level directory.
pub fn strip_component(source: impl AsRef<Path>) -> Result<PathBuf, Error> {
    // TODO(konstin): Verify the name of the directory.
    let top_level =
        fs_err::read_dir(source.as_ref())?.collect::<std::io::Result<Vec<fs_err::DirEntry>>>()?;
    let [root] = top_level.as_slice() else {
        return Err(Error::InvalidArchive(
            top_level.into_iter().map(|e| e.file_name()).collect(),
        ));
    };
    Ok(root.path())
}
