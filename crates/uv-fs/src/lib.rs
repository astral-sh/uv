use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use tempfile::NamedTempFile;
use tracing::{debug, error, info, trace, warn};

pub use crate::path::*;

pub mod cachedir;
mod path;
pub mod which;

/// Attempt to check if the two paths refer to the same file.
///
/// Returns `Some(true)` if the files are missing, but would be the same if they existed.
pub fn is_same_file_allow_missing(left: &Path, right: &Path) -> Option<bool> {
    // First, check an exact path comparison.
    if left == right {
        return Some(true);
    }

    // Second, check the files directly.
    if let Ok(value) = same_file::is_same_file(left, right) {
        return Some(value);
    };

    // Often, one of the directories won't exist yet so perform the comparison up a level.
    if let (Some(left_parent), Some(right_parent), Some(left_name), Some(right_name)) = (
        left.parent(),
        right.parent(),
        left.file_name(),
        right.file_name(),
    ) {
        match same_file::is_same_file(left_parent, right_parent) {
            Ok(true) => return Some(left_name == right_name),
            Ok(false) => return Some(false),
            _ => (),
        }
    };

    // We couldn't determine if they're the same.
    None
}

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
/// On Windows, this uses the `junction` crate to create a junction point. The
/// operation is _not_ atomic, as we first delete the junction, then create a
/// junction at the same path.
///
/// Note that because junctions are used, the source must be a directory.
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
///
/// On Unix, this method creates a temporary file, then moves it into place.
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

#[cfg(unix)]
pub fn remove_symlink(path: impl AsRef<Path>) -> std::io::Result<()> {
    fs_err::remove_file(path.as_ref())
}

/// Create a symlink at `dst` pointing to `src` on Unix or copy `src` to `dst` on Windows
///
/// This does not replace an existing symlink or file at `dst`.
///
/// This does not fallback to copying on Unix.
///
/// This function should only be used for files. If targeting a directory, use [`replace_symlink`]
/// instead; it will use a junction on Windows, which is more performant.
pub fn symlink_or_copy_file(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        fs_err::copy(src.as_ref(), dst.as_ref())?;
    }
    #[cfg(unix)]
    {
        fs_err::os::unix::fs::symlink(src.as_ref(), dst.as_ref())?;
    }

    Ok(())
}

#[cfg(windows)]
pub fn remove_symlink(path: impl AsRef<Path>) -> std::io::Result<()> {
    match junction::delete(dunce::simplified(path.as_ref())) {
        Ok(()) => match fs_err::remove_dir_all(path.as_ref()) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Return a [`NamedTempFile`] in the specified directory.
///
/// Sets the permissions of the temporary file to `0o666`, to match the non-temporary file default.
/// ([`NamedTempfile`] defaults to `0o600`.)
#[cfg(unix)]
pub fn tempfile_in(path: &Path) -> std::io::Result<NamedTempFile> {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o666))
        .tempfile_in(path)
}

/// Return a [`NamedTempFile`] in the specified directory.
#[cfg(not(unix))]
pub fn tempfile_in(path: &Path) -> std::io::Result<NamedTempFile> {
    tempfile::Builder::new().tempfile_in(path)
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
#[cfg(feature = "tokio")]
pub async fn write_atomic(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = tempfile_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::tokio::write(&temp_file, &data).await?;
    persist_with_retry(temp_file, path.as_ref()).await
}

/// Write `data` to `path` atomically using a temporary file and atomic rename.
pub fn write_atomic_sync(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let temp_file = tempfile_in(
        path.as_ref()
            .parent()
            .expect("Write path must have a parent"),
    )?;
    fs_err::write(&temp_file, &data)?;
    persist_with_retry_sync(temp_file, path.as_ref())
}

/// Copy `from` to `to` atomically using a temporary file and atomic rename.
pub fn copy_atomic_sync(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<()> {
    let temp_file = tempfile_in(to.as_ref().parent().expect("Write path must have a parent"))?;
    fs_err::copy(from.as_ref(), &temp_file)?;
    persist_with_retry_sync(temp_file, to.as_ref())
}

#[cfg(windows)]
fn backoff_file_move() -> backon::ExponentialBackoff {
    use backon::BackoffBuilder;
    // This amounts to 10 total seconds of trying the operation.
    // We start at 10 milliseconds and try 9 times, doubling each time, so the last try will take
    // about 10*(2^9) milliseconds ~= 5 seconds. All other attempts combined should equal
    // the length of the last attempt (because it's a sum of powers of 2), so 10 seconds overall.
    backon::ExponentialBuilder::default()
        .with_min_delay(std::time::Duration::from_millis(10))
        .with_max_times(9)
        .build()
}

/// Rename a file, retrying (on Windows) if it fails due to transient operating system errors.
#[cfg(feature = "tokio")]
pub async fn rename_with_retry(
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        use backon::Retryable;
        // On Windows, antivirus software can lock files temporarily, making them inaccessible.
        // This is most common for DLLs, and the common suggestion is to retry the operation with
        // some backoff.
        //
        // See: <https://github.com/astral-sh/uv/issues/1491> & <https://github.com/astral-sh/uv/issues/9531>
        let from = from.as_ref();
        let to = to.as_ref();

        let rename = || async { fs_err::rename(from, to) };

        rename
            .retry(backoff_file_move())
            .sleep(tokio::time::sleep)
            .when(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
            .notify(|err, _dur| {
                warn!(
                    "Retrying rename from {} to {} due to transient error: {}",
                    from.display(),
                    to.display(),
                    err
                );
            })
            .await
    }
    #[cfg(not(windows))]
    {
        fs_err::tokio::rename(from, to).await
    }
}

/// Rename or copy a file, retrying (on Windows) if it fails due to transient operating system
/// errors, in a synchronous context.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn with_retry_sync(
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
    operation_name: &str,
    operation: impl Fn() -> Result<(), std::io::Error>,
) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        use backon::BlockingRetryable;
        // On Windows, antivirus software can lock files temporarily, making them inaccessible.
        // This is most common for DLLs, and the common suggestion is to retry the operation with
        // some backoff.
        //
        // See: <https://github.com/astral-sh/uv/issues/1491> & <https://github.com/astral-sh/uv/issues/9531>
        let from = from.as_ref();
        let to = to.as_ref();

        operation
            .retry(backoff_file_move())
            .sleep(std::thread::sleep)
            .when(|err| err.kind() == std::io::ErrorKind::PermissionDenied)
            .notify(|err, _dur| {
                warn!(
                    "Retrying {} from {} to {} due to transient error: {}",
                    operation_name,
                    from.display(),
                    to.display(),
                    err
                );
            })
            .call()
            .map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "Failed {} {} to {}: {}",
                        operation_name,
                        from.display(),
                        to.display(),
                        err
                    ),
                )
            })
    }
    #[cfg(not(windows))]
    {
        operation()
    }
}

/// Why a file persist failed
#[cfg(windows)]
enum PersistRetryError {
    /// Something went wrong while persisting, maybe retry (contains error message)
    Persist(String),
    /// Something went wrong trying to retrieve the file to persist, we must bail
    LostState,
}

/// Persist a `NamedTempFile`, retrying (on Windows) if it fails due to transient operating system errors, in a synchronous context.
pub async fn persist_with_retry(
    from: NamedTempFile,
    to: impl AsRef<Path>,
) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        use backon::Retryable;
        // On Windows, antivirus software can lock files temporarily, making them inaccessible.
        // This is most common for DLLs, and the common suggestion is to retry the operation with
        // some backoff.
        //
        // See: <https://github.com/astral-sh/uv/issues/1491> & <https://github.com/astral-sh/uv/issues/9531>
        let to = to.as_ref();

        // Ok there's a lot of complex ownership stuff going on here.
        //
        // the `NamedTempFile` `persist` method consumes `self`, and returns it back inside
        // the Error in case of `PersistError`:
        // https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist
        // So every time we fail, we need to reset the `NamedTempFile` to try again.
        //
        // Every time we (re)try we call this outer closure (`let persist = ...`), so it needs to
        // be at least a `FnMut` (as opposed to `Fnonce`). However the closure needs to return a
        // totally owned `Future` (so effectively it returns a `FnOnce`).
        //
        // But if the `Future` is totally owned it *necessarily* can't write back the `NamedTempFile`
        // to somewhere the outer `FnMut` can see using references. So we need to use `Arc`s
        // with interior mutability (`Mutex`) to have the closure and all the Futures it creates share
        // a single memory location that the `NamedTempFile` can be shuttled in and out of.
        //
        // In spite of the Mutex all of this code will run logically serially, so there shouldn't be a
        // chance for a race where we try to get the `NamedTempFile` but it's actually None. The code
        // is just written pedantically/robustly.
        let from = std::sync::Arc::new(std::sync::Mutex::new(Some(from)));
        let persist = || {
            // Turn our by-ref-captured Arc into an owned Arc that the Future can capture by-value
            let from2 = from.clone();

            async move {
                let maybe_file: Option<NamedTempFile> = from2
                    .lock()
                    .map_err(|_| PersistRetryError::LostState)?
                    .take();
                if let Some(file) = maybe_file {
                    file.persist(to).map_err(|err| {
                        let error_message: String = err.to_string();
                        // Set back the `NamedTempFile` returned back by the Error
                        if let Ok(mut guard) = from2.lock() {
                            *guard = Some(err.file);
                            PersistRetryError::Persist(error_message)
                        } else {
                            PersistRetryError::LostState
                        }
                    })
                } else {
                    Err(PersistRetryError::LostState)
                }
            }
        };

        let persisted = persist
            .retry(backoff_file_move())
            .sleep(tokio::time::sleep)
            .when(|err| matches!(err, PersistRetryError::Persist(_)))
            .notify(|err, _dur| {
                if let PersistRetryError::Persist(error_message) = err {
                    warn!(
                        "Retrying to persist temporary file to {}: {}",
                        to.display(),
                        error_message,
                    );
                };
            })
            .await;

        match persisted {
            Ok(_) => Ok(()),
            Err(PersistRetryError::Persist(error_message)) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to persist temporary file to {}: {}",
                    to.display(),
                    error_message,
                ),
            )),
            Err(PersistRetryError::LostState) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to retrieve temporary file while trying to persist to {}",
                    to.display()
                ),
            )),
        }
    }
    #[cfg(not(windows))]
    {
        async { fs_err::rename(from, to) }.await
    }
}

/// Persist a `NamedTempFile`, retrying (on Windows) if it fails due to transient operating system errors, in a synchronous context.
pub fn persist_with_retry_sync(
    from: NamedTempFile,
    to: impl AsRef<Path>,
) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        use backon::BlockingRetryable;
        // On Windows, antivirus software can lock files temporarily, making them inaccessible.
        // This is most common for DLLs, and the common suggestion is to retry the operation with
        // some backoff.
        //
        // See: <https://github.com/astral-sh/uv/issues/1491> & <https://github.com/astral-sh/uv/issues/9531>
        let to = to.as_ref();

        // the `NamedTempFile` `persist` method consumes `self`, and returns it back inside the Error in case of `PersistError`
        // https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist
        // So we will update the `from` optional value in safe and borrow-checker friendly way every retry
        // Allows us to use the NamedTempFile inside a FnMut closure used for backoff::retry
        let mut from = Some(from);
        let persist = || {
            // Needed because we cannot move out of `from`, a captured variable in an `FnMut` closure, and then pass it to the async move block
            if let Some(file) = from.take() {
                file.persist(to).map_err(|err| {
                    let error_message = err.to_string();
                    // Set back the NamedTempFile returned back by the Error
                    from = Some(err.file);
                    PersistRetryError::Persist(error_message)
                })
            } else {
                Err(PersistRetryError::LostState)
            }
        };

        let persisted = persist
            .retry(backoff_file_move())
            .sleep(std::thread::sleep)
            .when(|err| matches!(err, PersistRetryError::Persist(_)))
            .notify(|err, _dur| {
                if let PersistRetryError::Persist(error_message) = err {
                    warn!(
                        "Retrying to persist temporary file to {}: {}",
                        to.display(),
                        error_message,
                    );
                };
            })
            .call();

        match persisted {
            Ok(_) => Ok(()),
            Err(PersistRetryError::Persist(error_message)) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to persist temporary file to {}: {}",
                    to.display(),
                    error_message,
                ),
            )),
            Err(PersistRetryError::LostState) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to retrieve temporary file while trying to persist to {}",
                    to.display()
                ),
            )),
        }
    }
    #[cfg(not(windows))]
    {
        fs_err::rename(from, to)
    }
}

/// Iterate over the subdirectories of a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn directories(path: impl AsRef<Path>) -> Result<impl Iterator<Item = PathBuf>, io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    Ok(entries
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {err}");
                None
            }
        })
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .map(|entry| entry.path()))
}

/// Iterate over the entries in a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn entries(path: impl AsRef<Path>) -> Result<impl Iterator<Item = PathBuf>, io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    Ok(entries
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {err}");
                None
            }
        })
        .map(|entry| entry.path()))
}

/// Iterate over the files in a directory.
///
/// If the directory does not exist, returns an empty iterator.
pub fn files(path: impl AsRef<Path>) -> Result<impl Iterator<Item = PathBuf>, io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    Ok(entries
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read entry: {err}");
                None
            }
        })
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .map(|entry| entry.path()))
}

/// Returns `true` if a path is a temporary file or directory.
pub fn is_temporary(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".tmp"))
}

/// A file lock that is automatically released when dropped.
#[derive(Debug)]
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
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!(
                            "Could not acquire lock for `{resource}` at `{}`: {}",
                            file.path().user_display(),
                            err
                        ),
                    )
                })?;

                debug!("Acquired lock for `{resource}`");
                Ok(Self(file))
            }
        }
    }

    /// The same as [`LockedFile::acquire`], but for synchronous contexts. Do not use from an async
    /// context, as this can block the runtime while waiting for another process to release the
    /// lock.
    pub fn acquire_blocking(
        path: impl AsRef<Path>,
        resource: impl Display,
    ) -> Result<Self, std::io::Error> {
        let file = Self::create(path)?;
        let resource = resource.to_string();
        Self::lock_file_blocking(file, &resource)
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

    #[cfg(unix)]
    fn create(path: impl AsRef<Path>) -> Result<fs_err::File, std::io::Error> {
        use std::os::unix::fs::PermissionsExt;

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

/// An asynchronous reader that reports progress as bytes are read.
#[cfg(feature = "tokio")]
pub struct ProgressReader<Reader: tokio::io::AsyncRead + Unpin, Callback: Fn(usize) + Unpin> {
    reader: Reader,
    callback: Callback,
}

#[cfg(feature = "tokio")]
impl<Reader: tokio::io::AsyncRead + Unpin, Callback: Fn(usize) + Unpin>
    ProgressReader<Reader, Callback>
{
    /// Create a new [`ProgressReader`] that wraps another reader.
    pub fn new(reader: Reader, callback: Callback) -> Self {
        Self { reader, callback }
    }
}

#[cfg(feature = "tokio")]
impl<Reader: tokio::io::AsyncRead + Unpin, Callback: Fn(usize) + Unpin> tokio::io::AsyncRead
    for ProgressReader<Reader, Callback>
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.as_mut().reader)
            .poll_read(cx, buf)
            .map_ok(|()| {
                (self.callback)(buf.filled().len());
            })
    }
}

/// Recursively copy a directory and its contents.
pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs_err::create_dir_all(&dst)?;
    for entry in fs_err::read_dir(src.as_ref())? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs_err::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
