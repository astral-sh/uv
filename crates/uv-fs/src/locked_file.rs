use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use std::{env, io};

use thiserror::Error;
use tracing::{debug, error, info, trace, warn};

use uv_static::EnvVars;

use crate::Simplified;

/// Parsed value of `UV_LOCK_TIMEOUT`, with a default of 5 min.
static LOCK_TIMEOUT: LazyLock<Duration> = LazyLock::new(|| {
    let default_timeout = Duration::from_secs(300);
    let Some(lock_timeout) = env::var_os(EnvVars::UV_LOCK_TIMEOUT) else {
        return default_timeout;
    };

    if let Some(lock_timeout) = lock_timeout
        .to_str()
        .and_then(|lock_timeout| lock_timeout.parse::<u64>().ok())
    {
        Duration::from_secs(lock_timeout)
    } else {
        warn!(
            "Could not parse value of {} as integer: {:?}",
            EnvVars::UV_LOCK_TIMEOUT,
            lock_timeout
        );
        default_timeout
    }
});

#[derive(Debug, Error)]
pub enum LockedFileError {
    #[error(
        "Timeout ({}s) when waiting for lock on `{}` at `{}`, is another uv process running? You can set `{}` to increase the timeout.",
        timeout.as_secs(),
        resource,
        path.user_display(),
        EnvVars::UV_LOCK_TIMEOUT
    )]
    Timeout {
        timeout: Duration,
        resource: String,
        path: PathBuf,
    },
    #[error(
        "Could not acquire lock for `{}` at `{}`",
        resource,
        path.user_display()
    )]
    Lock {
        resource: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
}

impl LockedFileError {
    pub fn as_io_error(&self) -> Option<&io::Error> {
        match self {
            Self::Timeout { .. } | Self::JoinError(_) => None,
            Self::Lock { source, .. } => Some(source),
            Self::Io(err) => Some(err),
        }
    }
}

/// Whether to acquire a shared (read) lock or exclusive (write) lock.
#[derive(Debug, Clone, Copy)]
pub enum LockedFileMode {
    Shared,
    Exclusive,
}

impl LockedFileMode {
    /// Try to lock the file and return an error if the lock is already acquired by another process
    /// and cannot be acquired immediately.
    fn try_lock(self, file: &fs_err::File) -> Result<(), std::fs::TryLockError> {
        match self {
            Self::Exclusive => file.try_lock()?,
            Self::Shared => file.try_lock_shared()?,
        }
        Ok(())
    }

    /// Lock the file, blocking until the lock becomes available if necessary.
    fn lock(self, file: &fs_err::File) -> Result<(), io::Error> {
        match self {
            Self::Exclusive => file.lock()?,
            Self::Shared => file.lock_shared()?,
        }
        Ok(())
    }
}

impl Display for LockedFileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Exclusive => write!(f, "exclusive"),
        }
    }
}

/// A file lock that is automatically released when dropped.
#[cfg(feature = "tokio")]
#[derive(Debug)]
#[must_use]
pub struct LockedFile(fs_err::File);

#[cfg(feature = "tokio")]
impl LockedFile {
    /// Inner implementation for [`LockedFile::acquire`].
    async fn lock_file(
        file: fs_err::File,
        mode: LockedFileMode,
        resource: &str,
    ) -> Result<Self, LockedFileError> {
        trace!(
            "Checking lock for `{resource}` at `{}`",
            file.path().user_display()
        );
        // If there's no contention, return directly.
        let try_lock_exclusive = tokio::task::spawn_blocking(move || (mode.try_lock(&file), file));
        let file = match try_lock_exclusive.await? {
            (Ok(()), file) => {
                debug!("Acquired {mode} lock for `{resource}`");
                return Ok(Self(file));
            }
            (Err(err), file) => {
                // Log error code and enum kind to help debugging more exotic failures.
                if !crate::is_known_already_locked_error(&err) {
                    debug!("Try lock {mode} error: {err:?}");
                }
                file
            }
        };

        // If there's lock contention, wait and break deadlocks with a timeout if necessary.
        info!(
            "Waiting to acquire {mode} lock for `{resource}` at `{}`",
            file.path().user_display(),
        );
        let path = file.path().to_path_buf();
        let lock_exclusive = tokio::task::spawn_blocking(move || (mode.lock(&file), file));
        let (result, file) = tokio::time::timeout(*LOCK_TIMEOUT, lock_exclusive)
            .await
            .map_err(|_| LockedFileError::Timeout {
                timeout: *LOCK_TIMEOUT,
                resource: resource.to_string(),
                path: path.clone(),
            })??;
        // Not an fs_err method, we need to build our own path context
        result.map_err(|err| LockedFileError::Lock {
            resource: resource.to_string(),
            path,
            source: err,
        })?;

        debug!("Acquired {mode} lock for `{resource}`");
        Ok(Self(file))
    }

    /// Inner implementation for [`LockedFile::acquire_no_wait`].
    fn lock_file_no_wait(file: fs_err::File, mode: LockedFileMode, resource: &str) -> Option<Self> {
        trace!(
            "Checking lock for `{resource}` at `{}`",
            file.path().user_display()
        );
        match mode.try_lock(&file) {
            Ok(()) => {
                debug!("Acquired {mode} lock for `{resource}`");
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

    /// Acquire a cross-process lock for a resource using a file at the provided path.
    pub async fn acquire(
        path: impl AsRef<Path>,
        mode: LockedFileMode,
        resource: impl Display,
    ) -> Result<Self, LockedFileError> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        Self::lock_file(file, mode, &resource).await
    }

    /// Acquire a cross-process lock for a resource using a file at the provided path
    ///
    /// Unlike [`LockedFile::acquire`] this function will not wait for the lock to become available.
    ///
    /// If the lock is not immediately available, [`None`] is returned.
    pub fn acquire_no_wait(
        path: impl AsRef<Path>,
        mode: LockedFileMode,
        resource: impl Display,
    ) -> Option<Self> {
        let file = Self::create(path).ok()?;
        let resource = resource.to_string();
        Self::lock_file_no_wait(file, mode, &resource)
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

#[cfg(feature = "tokio")]
impl Drop for LockedFile {
    /// Unlock the file.
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
