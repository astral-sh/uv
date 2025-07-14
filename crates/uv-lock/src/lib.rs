use std::fmt::Display;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fs_err as fs;
use fs2::FileExt;
use tracing::{debug, error, info, trace};

use uv_cache_key::{CacheKey, cache_digest};
use uv_fs::Simplified;
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

/// Acquire a cross-process lock for files at the provided path.
#[cfg(feature = "tokio")]
pub async fn acquire_path(path: impl AsRef<Path>) -> Result<LockedFile, std::io::Error> {
    let locks = get_or_init_locks()?;
    let path = path.as_ref();
    LockedFile::acquire(locks.join(cache_digest(&path)), path.display()).await
}

/// Acquire a cross-process lock for an arbitrary hashable resource (like a URL).
#[cfg(feature = "tokio")]
pub async fn acquire_resource<T: CacheKey + Display>(
    resource: T,
) -> Result<LockedFile, std::io::Error> {
    let locks = get_or_init_locks()?;
    LockedFile::acquire(locks.join(cache_digest(&resource)), resource).await
}

/// Get or initialize the global filesystem locks.
fn get_or_init_locks() -> std::io::Result<&'static FilesystemLocks> {
    static FILESYSTEM_LOCKS: OnceLock<FilesystemLocks> = OnceLock::new();

    // Return the existing filesystem locks, if they are already initialized.
    if let Some(locks) = FILESYSTEM_LOCKS.get() {
        return Ok(locks);
    }

    // Initialize the filesystem locks.
    let locks = FilesystemLocks::init()?;
    _ = FILESYSTEM_LOCKS.set(locks);

    Ok(FILESYSTEM_LOCKS.get().unwrap())
}

/// Filesystem locks used to synchronize access to shared resources across processes.
#[derive(Debug, Clone)]
struct FilesystemLocks {
    /// The path to the top-level directory of the filesystem locks.
    root: PathBuf,
}

impl FilesystemLocks {
    /// Create a new [`FilesystemLocks`].
    ///
    /// Prefer, in order:
    ///
    /// 1. The specific tool directory specified by the user, i.e., `UV_LOCK_DIR`
    /// 2. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/locks`
    /// 3. A directory in the local data directory, e.g., `./.uv/locks`
    fn init() -> Result<Self, std::io::Error> {
        let root = if let Some(lock_dir) =
            std::env::var_os(EnvVars::UV_LOCK_DIR).filter(|s| !s.is_empty())
        {
            std::path::absolute(lock_dir)?
        } else {
            StateStore::from_settings(None)?.bucket(StateBucket::Locks)
        };

        // Create the directory, if it doesn't exist.
        fs::create_dir_all(&root)?;

        // Add a .gitignore.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        Ok(Self { root })
    }

    /// Join a path to the root of the locks directory.
    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root.join(path)
    }
}

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
        match file.file().try_lock_exclusive() {
            Ok(()) => {
                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
            Err(err) => {
                // Log error code and enum kind to help debugging more exotic failures.
                if err.kind() != std::io::ErrorKind::WouldBlock {
                    debug!("Try lock error: {err:?}");
                }
                info!(
                    "Waiting to acquire lock for `{resource}` at `{}`",
                    file.path().user_display(),
                );
                file.file().lock_exclusive().map_err(|err| {
                    // Not an fs_err method, we need to build our own path context
                    std::io::Error::other(format!(
                        "Could not acquire lock for `{resource}` at `{}`: {}",
                        file.path().user_display(),
                        err
                    ))
                })?;

                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
        }
    }

    /// The same as [`LockedFile::acquire`], but for synchronous contexts. Do not use from an async
    /// context, as this can block the runtime while waiting for another process to release the
    /// lock.
    #[allow(dead_code)]
    fn acquire_blocking(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        Self::lock_file_blocking(file, &resource)
    }

    /// Acquire a cross-process lock for a resource using a file at the provided path.
    #[cfg(feature = "tokio")]
    async fn acquire(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        tokio::task::spawn_blocking(move || Self::lock_file_blocking(file, &resource)).await?
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
        if let Err(err) = fs2::FileExt::unlock(self.0.file()) {
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
