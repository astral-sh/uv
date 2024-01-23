use std::fmt::Display;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use fs_err as fs;
use tempfile::NamedTempFile;
use tracing::{error, warn};

use puffin_warnings::warn_user;

/// Symlink a directory.
#[cfg(windows)]
pub fn symlink_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(src, dst)
}

/// Symlink a directory.
#[cfg(unix)]
pub fn symlink_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
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
                path.as_ref().display(),
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
                path.as_ref().display(),
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

    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
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
                    path.as_ref().display()
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
