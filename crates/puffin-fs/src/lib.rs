use std::path::{Path, PathBuf};

use fs_err as fs;
use tempfile::NamedTempFile;
use tracing::warn;

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
