use std::fmt::Display;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use tempfile::NamedTempFile;
use tracing::{debug, error, trace, warn};

use uv_warnings::warn_user;

pub use crate::path::*;

pub mod cachedir;
mod path;

/// Reads data from the path and requires that it be valid UTF-8 or UTF-16.
///
/// This uses BOM sniffing to determine if the data should be transcoded
/// from UTF-16 to Rust's `String` type (which uses UTF-8).
///
/// This should generally only be used when one specifically wants to support
/// reading UTF-16 transparently.
///
/// If the file path is `-`, then contents are read from stdin instead.
#[cfg(feature = "tokio")]
pub async fn read_to_string_transcode(path: impl AsRef<Path>) -> std::io::Result<String> {
    use std::io::Read;

    use encoding_rs_io::DecodeReaderBytes;

    let path = path.as_ref();
    let raw = if path == Path::new("-") {
        let mut buf = Vec::with_capacity(1024);
        std::io::stdin().read_to_end(&mut buf)?;
        buf
    } else {
        fs_err::tokio::read(path).await?
    };
    let mut buf = String::with_capacity(1024);
    DecodeReaderBytes::new(&*raw)
        .read_to_string(&mut buf)
        .map_err(|err| {
            let path = path.display();
            std::io::Error::other(format!("failed to decode file {path}: {err}"))
        })?;
    Ok(buf)
}

/// Create a symlink at `dst` pointing to `src`, replacing any existing symlink.
///
/// On Windows, this uses the `junction` crate to create a junction point.
/// Note because junctions are used, the source must be a directory.
#[cfg(windows)]
pub fn replace_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    // If the source is a file, we can't create a junction
    if src.as_ref().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Cannot create a junction for {}: is not a directory",
                src.as_ref().display()
            ),
        ));
    }

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

/// Create a symlink at `dst` pointing to `src`, replacing any existing symlink if necessary.
#[cfg(unix)]
pub fn replace_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    // Attempt to create the symlink directly.
    match std::os::unix::fs::symlink(src.as_ref(), dst.as_ref()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            // Create a symlink, using a temporary file to ensure atomicity.
            let temp_dir = tempfile::tempdir_in(dst.as_ref().parent().unwrap())?;
            let temp_file = temp_dir.path().join("link");
            std::os::unix::fs::symlink(src, &temp_file)?;

            // Move the symlink into the target location.
            fs_err::rename(&temp_file, dst.as_ref())?;

            Ok(())
        }
        Err(err) => Err(err),
    }
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
                path.user_display(),
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
                path.user_display(),
                err.error
            ),
        )
    })?;
    Ok(())
}

/// Rename a file, retrying (on Windows) if it fails due to transient operating system errors.
#[cfg(feature = "tokio")]
pub async fn rename_with_retry(
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<(), std::io::Error> {
    if cfg!(windows) {
        // On Windows, antivirus software can lock files temporarily, making them inaccessible.
        // This is most common for DLLs, and the common suggestion is to retry the operation with
        // some backoff.
        //
        // See: <https://github.com/astral-sh/uv/issues/1491>
        let from = from.as_ref();
        let to = to.as_ref();

        let backoff = backoff::ExponentialBackoffBuilder::default()
            .with_initial_interval(std::time::Duration::from_millis(10))
            .with_max_elapsed_time(Some(std::time::Duration::from_secs(10)))
            .build();

        backoff::future::retry(backoff, || async move {
            match fs_err::rename(from, to) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    warn!(
                        "Retrying rename from {} to {} due to transient error: {}",
                        from.display(),
                        to.display(),
                        err
                    );
                    Err(backoff::Error::transient(err))
                }
                Err(err) => Err(backoff::Error::permanent(err)),
            }
        })
        .await
    } else {
        fs_err::tokio::rename(from, to).await
    }
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
        trace!("Checking lock for `{resource}`");
        match file.file().try_lock_exclusive() {
            Ok(()) => {
                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
            Err(err) => {
                // Log error code and enum kind to help debugging more exotic failures
                debug!("Try lock error, waiting for exclusive lock: {:?}", err);
                warn_user!(
                    "Waiting to acquire lock for {} (lockfile: {})",
                    resource,
                    path.user_display(),
                );
                file.file().lock_exclusive().map_err(|err| {
                    // Not an fs_err method, we need to build our own path context
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Could not lock {}: {}", path.as_ref().user_display(), err),
                    )
                })?;
                Ok(Self(file))
            }
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
