use crate::Simplified;

use std::fmt::Display;
use std::path::Path;

use tracing::{debug, error, info, trace};

/// A file lock that is automatically released when dropped.
#[derive(Debug)]
#[must_use]
pub struct LockedFile(fs_err::File);

impl LockedFile {
    /// Inner implementation for [`LockedFile::acquire_blocking`] and [`LockedFile::acquire`].
    fn lock_file_blocking(file: fs_err::File, resource: &str) -> Result<Self, std::io::Error> {
        trace!(
            "Checking lock for `{resource}` at `{}`",
            file.path().user_display()
        );
        match file.try_lock() {
            Ok(()) => {
                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
            Err(err) => {
                // Log error code and enum kind to help debugging more exotic failures.
                if !crate::is_known_already_locked_error(&err) {
                    debug!("Try lock error: {err:?}");
                }
                info!(
                    "Waiting to acquire lock for `{resource}` at `{}`",
                    file.path().user_display(),
                );
                file.lock()?;

                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
        }
    }

    /// Inner implementation for [`LockedFile::acquire_no_wait`].
    fn lock_file_no_wait(file: fs_err::File, resource: &str) -> Option<Self> {
        trace!(
            "Checking lock for `{resource}` at `{}`",
            file.path().user_display()
        );
        match file.try_lock() {
            Ok(()) => {
                debug!("Acquired lock for `{resource}`");
                Some(Self(file))
            }
            Err(err) => {
                // Log error code and enum kind to help debugging more exotic failures.
                if !crate::is_known_already_locked_error(&err) {
                    debug!("Try lock error: {err:?}");
                }
                debug!("Lock is busy for `{resource}`");
                None
            }
        }
    }

    /// Inner implementation for [`LockedFile::acquire_shared_blocking`] and
    /// [`LockedFile::acquire_blocking`].
    fn lock_file_shared_blocking(
        file: fs_err::File,
        resource: &str,
    ) -> Result<Self, std::io::Error> {
        trace!(
            "Checking shared lock for `{resource}` at `{}`",
            file.path().user_display()
        );
        match file.try_lock_shared() {
            Ok(()) => {
                debug!("Acquired shared lock for `{resource}`");
                Ok(Self(file))
            }
            Err(err) => {
                // Log error code and enum kind to help debugging more exotic failures.
                if !crate::is_known_already_locked_error(&err) {
                    debug!("Try lock error: {err:?}");
                }
                info!(
                    "Waiting to acquire shared lock for `{resource}` at `{}`",
                    file.path().user_display(),
                );
                file.lock_shared()?;

                debug!("Acquired shared lock for `{resource}`");
                Ok(Self(file))
            }
        }
    }

    /// The same as [`LockedFile::acquire`], but for synchronous contexts.
    ///
    /// Do not use from an async context, as this can block the runtime while waiting for another
    /// process to release the lock.
    pub fn acquire_blocking(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        Self::lock_file_blocking(file, &resource)
    }

    /// The same as [`LockedFile::acquire_blocking`], but for synchronous contexts.
    ///
    /// Do not use from an async context, as this can block the runtime while waiting for another
    /// process to release the lock.
    pub fn acquire_shared_blocking(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        Self::lock_file_shared_blocking(file, &resource)
    }

    /// Acquire a cross-process lock for a resource using a file at the provided path.
    #[cfg(feature = "tokio")]
    pub async fn acquire(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        tokio::task::spawn_blocking(move || Self::lock_file_blocking(file, &resource)).await?
    }

    /// Acquire a cross-process read lock for a shared resource using a file at the provided path.
    #[cfg(feature = "tokio")]
    pub async fn acquire_shared(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        tokio::task::spawn_blocking(move || Self::lock_file_shared_blocking(file, &resource))
            .await?
    }

    /// Acquire a cross-process lock for a resource using a file at the provided path
    ///
    /// Unlike [`LockedFile::acquire`] this function will not wait for the lock to become available.
    ///
    /// If the lock is not immediately available, [`None`] is returned.
    pub fn acquire_no_wait(path: impl AsRef<Path>, resource: impl Display) -> Option<Self> {
        let file = Self::create(path).ok()?;
        let resource = resource.to_string();
        Self::lock_file_no_wait(file, &resource)
    }

    #[cfg(unix)]
    fn create(path: impl AsRef<Path>) -> Result<fs_err::File, std::io::Error> {
        use std::os::unix::fs::PermissionsExt;
        use tempfile::NamedTempFile;
        use tracing::warn;

        // If path already exists, return it.
        if let Ok(file) = fs_err::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())
        {
            return Ok(file);
        }

        // Otherwise, create a temporary file with 777 permissions. We must set
        // permissions _after_ creating the file, to override the `umask`.
        let file = if let Some(parent) = path.as_ref().parent() {
            NamedTempFile::new_in(parent)?
        } else {
            NamedTempFile::new()?
        };
        if let Err(err) = file
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o777))
        {
            warn!("Failed to set permissions on temporary file: {err}");
        }

        // Try to move the file to path, but if path exists now, just open path
        match file.persist_noclobber(path.as_ref()) {
            Ok(file) => Ok(fs_err::File::from_parts(file, path.as_ref())),
            Err(err) => {
                if err.error.kind() == std::io::ErrorKind::AlreadyExists {
                    fs_err::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(path.as_ref())
                } else {
                    Err(err.error)
                }
            }
        }
    }

    #[cfg(not(unix))]
    fn create(path: impl AsRef<Path>) -> std::io::Result<fs_err::File> {
        fs_err::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path.as_ref())
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        if let Err(err) = self.0.unlock() {
            error!(
                "Failed to unlock {}; program may be stuck: {}",
                self.0.path().display(),
                err
            );
        } else {
            debug!("Released lock at `{}`", self.0.path().display());
        }
    }
}
