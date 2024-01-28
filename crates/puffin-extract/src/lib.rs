use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use rustc_hash::FxHashSet;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use zip::result::ZipError;
use zip::ZipArchive;

pub use crate::vendor::{CloneableSeekableReader, HasLength};

mod vendor;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error(transparent)]
    AsyncZip(#[from] async_zip::error::ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Unsupported archive type: {0}")]
    UnsupportedArchive(PathBuf),
    #[error(
        "The top level of the archive must only contain a list directory, but it contains: {0:?}"
    )]
    InvalidArchive(Vec<fs_err::DirEntry>),
}

/// Unzip a `.zip` archive into the target directory without requiring Seek.
///
/// This is useful for unzipping files as they're being downloaded. If the archive
/// is already fully on disk, consider using `unzip_archive`, which can use multiple
/// threads to work faster in that case.
pub async fn unzip_no_seek<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: &Path,
) -> Result<(), Error> {
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Construct the (expected) path to the file on-disk.
        let path = entry.reader().entry().filename().as_str()?;
        let path = target.join(path);
        let is_dir = entry.reader().entry().dir()?;

        // Either create the directory or write the file to disk.
        if is_dir {
            fs_err::tokio::create_dir_all(path).await?;
        } else {
            if let Some(parent) = path.parent() {
                fs_err::tokio::create_dir_all(parent).await?;
            }
            let file = fs_err::tokio::File::create(path).await?;
            let mut writer =
                if let Ok(size) = usize::try_from(entry.reader().entry().uncompressed_size()) {
                    tokio::io::BufWriter::with_capacity(size, file)
                } else {
                    tokio::io::BufWriter::new(file)
                };
            let mut reader = entry.reader_mut().compat();
            tokio::io::copy(&mut reader, &mut writer).await?;
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry.skip().await?;
    }

    // On Unix, we need to set file permissions, which are stored in the central directory, at the
    // end of the archive. The `ZipFileReader` reads until it sees a central directory signature,
    // which indicates the first entry in the central directory. So we continue reading from there.
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // To avoid lots of small reads to `reader` when parsing the central directory, wrap it in
        // a buffer. The buffer size is semi-arbitrary, but the central directory is usually small.
        let mut buf = futures::io::BufReader::with_capacity(1024 * 1024, reader);
        let mut directory = async_zip::base::read::cd::CentralDirectoryReader::new(&mut buf);
        while let Some(entry) = directory.next().await? {
            if entry.dir()? {
                continue;
            }

            // Construct the (expected) path to the file on-disk.
            let path = entry.filename().as_str()?;
            let path = target.join(path);

            if let Some(mode) = entry.unix_permissions() {
                fs_err::set_permissions(&path, Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

pub async fn unzip_no_seek_fast<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: &Path,
) -> Result<(), Error> {
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Construct the (expected) path to the file on-disk.
        let path = entry.reader().entry().filename().as_str()?;
        let path = target.join(path);
        let is_dir = entry.reader().entry().dir()?;

        // Either create the directory or write the file to disk.
        if is_dir {
            fs_err::tokio::create_dir_all(path).await?;
        } else {
            if let Some(parent) = path.parent() {
                fs_err::tokio::create_dir_all(parent).await?;
            }
            let file = fs_err::tokio::File::create(path).await?;
            let mut writer =
                if let Ok(size) = usize::try_from(entry.reader().entry().uncompressed_size()) {
                    tokio::io::BufWriter::with_capacity(size, file)
                } else {
                    tokio::io::BufWriter::new(file)
                };
            let mut reader = entry.reader_mut().compat();
            tokio::io::copy(&mut reader, &mut writer).await?;
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry.skip().await?;
    }

    // On Unix, we need to set file permissions, which are stored in the central directory, at the
    // end of the archive. The `ZipFileReader` reads until it sees a central directory signature,
    // which indicates the first entry in the central directory. So we continue reading from there.
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // To avoid lots of small reads to `reader` when parsing the central directory, wrap it in
        // a buffer. The buffer size is semi-arbitrary, but the central directory is usually small.
        let mut buf = futures::io::BufReader::new(reader);
        let mut directory = async_zip::base::read::cd::CentralDirectoryReader::new(&mut buf);
        while let Some(entry) = directory.next().await? {
            if entry.dir()? {
                continue;
            }

            // Construct the (expected) path to the file on-disk.
            let path = entry.filename().as_str()?;
            let path = target.join(path);

            if let Some(mode) = entry.unix_permissions() {
                fs_err::set_permissions(&path, Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

pub async fn unzip_no_seek_faster<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    target: &Path,
) -> Result<(), Error> {
    let mut reader = reader.compat();
    let mut zip = async_zip::base::read::stream::ZipFileReader::new(&mut reader);

    while let Some(mut entry) = zip.next_with_entry().await? {
        // Construct the (expected) path to the file on-disk.
        let path = entry.reader().entry().filename().as_str()?;
        let path = target.join(path);
        let is_dir = entry.reader().entry().dir()?;

        // Either create the directory or write the file to disk.
        if is_dir {
            fs_err::tokio::create_dir_all(path).await?;
        } else {
            if let Some(parent) = path.parent() {
                fs_err::tokio::create_dir_all(parent).await?;
            }
            let file = fs_err::tokio::File::create(path).await?;
            let mut writer =
                if let Ok(size) = usize::try_from(entry.reader().entry().uncompressed_size()) {
                    tokio::io::BufWriter::with_capacity(size, file)
                } else {
                    tokio::io::BufWriter::new(file)
                };
            let mut reader = entry.reader_mut().compat();
            tokio::io::copy(&mut reader, &mut writer).await?;
        }

        // Close current file to get access to the next one. See docs:
        // https://docs.rs/async_zip/0.0.16/async_zip/base/read/stream/
        zip = entry.skip().await?;
    }

    // On Unix, we need to set file permissions, which are stored in the central directory, at the
    // end of the archive. The `ZipFileReader` reads until it sees a central directory signature,
    // which indicates the first entry in the central directory. So we continue reading from there.
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // To avoid lots of small reads to `reader` when parsing the central directory, wrap it in
        // a buffer. The buffer size is semi-arbitrary, but the central directory is usually small.
        let mut buf = futures::io::BufReader::new(reader);
        let mut directory = async_zip::base::read::cd::CentralDirectoryReader::new(&mut buf);
        while let Some(entry) = directory.next().await? {
            if entry.dir()? {
                continue;
            }

            // Construct the (expected) path to the file on-disk.
            let path = entry.filename().as_str()?;
            let path = target.join(path);

            if let Some(mode) = entry.unix_permissions() {
                fs_err::set_permissions(&path, Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

/// Unzip a `.zip` archive into the target directory.
pub fn unzip_archive<R: Send + std::io::Read + std::io::Seek + HasLength>(
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
                fs_err::create_dir_all(&path)?;
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
