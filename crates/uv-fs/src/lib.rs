use std::fmt::Display;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use fs_err as fs;
use tempfile::NamedTempFile;
use tracing::{error, warn};

use uv_warnings::warn_user;

pub use crate::path::*;

mod path;

/// Reads the contents of the file path given into memory.
///
/// If the file path is `-`, then contents are read from stdin instead.
pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    let path = path.as_ref();
    if path == Path::new("-") {
        let mut buf = Vec::with_capacity(1024);
        std::io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        fs::read(path)
    }
}

/// Reads the contents of the file path given into memory as a `String`.
///
/// If the file path is `-`, then contents are read from stdin instead.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    use std::io::Read;

    let path = path.as_ref();
    if path == Path::new("-") {
        let mut buf = String::with_capacity(1024);
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        fs::read_to_string(path)
    }
}

/// Create a symlink from `src` to `dst`, replacing any existing symlink.
///
/// On Windows, this uses the `junction` crate to create a junction point.
#[cfg(windows)]
pub fn replace_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    // Remove the existing symlink, if any.
    match junction::delete(dunce::simplified(dst.as_ref())) {
        Ok(()) => match fs_err::remove_dir_all(dst.as_ref()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    };

    // Replace it with a new symlink.
    junction::create(
        dunce::simplified(src.as_ref()),
        dunce::simplified(dst.as_ref()),
    )
}

/// Create a symlink from `src` to `dst`, replacing any existing symlink.
#[cfg(unix)]
pub fn replace_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    // Create a symlink to the directory store.
    let temp_dir =
        tempfile::tempdir_in(dst.as_ref().parent().expect("Cache entry to have parent"))?;
    let temp_file = temp_dir.path().join("link");
    std::os::unix::fs::symlink(src, &temp_file)?;

    // Move the symlink into the wheel cache.
    fs_err::rename(&temp_file, dst.as_ref())?;

    Ok(())
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
#[cfg(feature = "tokio")]
pub async fn write_atomic(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = NamedTempFile::new_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::tokio::write(&temp_file, &data).await?;
    temp_file.persist(&path).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to persist temporary file to {}: {}",
                path.normalized_display(),
                err.error
            ),
        )
    })?;
    Ok(())
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
pub fn write_atomic_sync(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = NamedTempFile::new_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::write(&temp_file, &data)?;
    temp_file.persist(&path).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Failed to persist temporary file to {}: {}",
                path.normalized_display(),
                err.error
            ),
        )
    })?;
    Ok(())
}

/// Remove the file or directory at `path`, if it exists.
///
/// Returns `true` if the file or directory was removed, and `false` if the path did not exist.
pub fn force_remove_all(path: impl AsRef<Path>) -> Result<bool, std::io::Error> {
    let path = path.as_ref();

    let Some(metadata) = metadata_if_exists(path)? else {
        return Ok(false);
    };

    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }

    Ok(true)
}

/// Iterate over the subdirectories of a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn directories(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    path.as_ref()
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {}", err);
                None
            }
        })
        .filter(|entry| {
            entry
                .file_type()
                .map_or(false, |file_type| file_type.is_dir())
        })
        .map(|entry| entry.path())
}

/// Iterate over the symlinks in a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn symlinks(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    path.as_ref()
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {}", err);
                None
            }
        })
        .filter(|entry| {
            entry
                .file_type()
                .map_or(false, |file_type| file_type.is_symlink())
        })
        .map(|entry| entry.path())
}

/// Iterate over the files in a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn files(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    path.as_ref()
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {}", err);
                None
            }
        })
        .filter(|entry| {
            entry
                .file_type()
                .map_or(false, |file_type| file_type.is_file())
        })
        .map(|entry| entry.path())
}

/// A file lock that is automatically released when dropped.
#[derive(Debug)]
pub struct LockedFile(fs_err::File);

impl LockedFile {
    pub fn acquire(path: impl AsRef<Path>, resource: impl Display) -> Result<Self, std::io::Error> {
        let file = fs_err::File::create(path.as_ref())?;
        match file.file().try_lock_exclusive() {
            Ok(()) => Ok(Self(file)),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                warn_user!(
                    "Waiting to acquire lock for {} (lockfile: {})",
                    resource,
                    path.normalized_display(),
                );
                file.file().lock_exclusive()?;
                Ok(Self(file))
            }
            Err(err) => Err(err),
        }
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        if let Err(err) = self.0.file().unlock() {
            error!(
                "Failed to unlock {}; program may be stuck: {}",
                self.0.path().display(),
                err
            );
        }
    }
}

/// Given a path, return its metadata if the file exists, or `None` if it does not.
///
/// If the file exists but cannot be read, returns an error.
pub fn metadata_if_exists(path: impl AsRef<Path>) -> std::io::Result<Option<std::fs::Metadata>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
