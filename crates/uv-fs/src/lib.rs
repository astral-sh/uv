use std::path::{Path, PathBuf};

#[cfg(feature = "tokio")]
use std::io::Read;

#[cfg(feature = "tokio")]
use encoding_rs_io::DecodeReaderBytes;
use tempfile::NamedTempFile;
use tracing::warn;

pub use crate::locked_file::*;
pub use crate::path::*;

pub mod cachedir;
pub mod link;
mod locked_file;
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
    }

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
    }

    // We couldn't determine if they're the same.
    None
}

/// Reads data from the path and requires that it be valid UTF-8 or UTF-16.
///
/// This uses BOM sniffing to determine if the data should be transcoded from UTF-16 to Rust's
/// `String` type (which uses UTF-8).
///
/// This should generally only be used when one specifically wants to support reading UTF-16
/// transparently.
///
/// If the file path is `-`, then contents are read from stdin instead.
#[cfg(feature = "tokio")]
pub async fn read_to_string_transcode(path: impl AsRef<Path>) -> std::io::Result<String> {
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
///
/// Changes to this function should be reflected in [`create_symlink`].
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
    }

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
    match fs_err::os::unix::fs::symlink(src.as_ref(), dst.as_ref()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            // Create a symlink, using a temporary file to ensure atomicity.
            let temp_dir = tempfile::tempdir_in(dst.as_ref().parent().unwrap())?;
            let temp_file = temp_dir.path().join("link");
            fs_err::os::unix::fs::symlink(src, &temp_file)?;

            // Move the symlink into the target location.
            fs_err::rename(&temp_file, dst.as_ref())?;

            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Create a symlink at `dst` pointing to `src`.
///
/// On Windows, this uses the `junction` crate to create a junction point.
///
/// Note that because junctions are used, the source must be a directory.
///
/// Changes to this function should be reflected in [`replace_symlink`].
#[cfg(windows)]
pub fn create_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
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

    junction::create(
        dunce::simplified(src.as_ref()),
        dunce::simplified(dst.as_ref()),
    )
}

/// Create a symlink at `dst` pointing to `src`.
#[cfg(unix)]
pub fn create_symlink(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs_err::os::unix::fs::symlink(src.as_ref(), dst.as_ref())
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
    // We retry 10 times, starting at 10*(2^0) milliseconds for the first retry, doubling with each
    // retry, so the last (10th) one will take about 10*(2^9) milliseconds ~= 5 seconds. All other
    // attempts combined should equal the length of the last attempt (because it's a sum of powers
    // of 2), so 10 seconds overall.
    backon::ExponentialBuilder::default()
        .with_min_delay(std::time::Duration::from_millis(10))
        .with_max_times(10)
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

        let rename = async || fs_err::rename(from, to);

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

// TODO(zanieb): Look into reusing this code?
/// Wrap an arbitrary operation on two files, e.g., copying, with retries on transient operating
/// system errors.
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
                std::io::Error::other(format!(
                    "Failed {} {} to {}: {}",
                    operation_name,
                    from.display(),
                    to.display(),
                    err
                ))
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

/// Persist a `NamedTempFile`, retrying (on Windows) if it fails due to transient operating system
/// errors.
#[cfg(feature = "tokio")]
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
                }
            })
            .await;

        match persisted {
            Ok(_) => Ok(()),
            Err(PersistRetryError::Persist(error_message)) => Err(std::io::Error::other(format!(
                "Failed to persist temporary file to {}: {}",
                to.display(),
                error_message,
            ))),
            Err(PersistRetryError::LostState) => Err(std::io::Error::other(format!(
                "Failed to retrieve temporary file while trying to persist to {}",
                to.display()
            ))),
        }
    }
    #[cfg(not(windows))]
    {
        async { fs_err::rename(from, to) }.await
    }
}

/// Persist a `NamedTempFile`, retrying (on Windows) if it fails due to transient operating system
/// errors.
///
/// This is a synchronous implementation of [`persist_with_retry`].
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
                }
            })
            .call();

        match persisted {
            Ok(_) => Ok(()),
            Err(PersistRetryError::Persist(error_message)) => Err(std::io::Error::other(format!(
                "Failed to persist temporary file to {}: {}",
                to.display(),
                error_message,
            ))),
            Err(PersistRetryError::LostState) => Err(std::io::Error::other(format!(
                "Failed to retrieve temporary file while trying to persist to {}",
                to.display()
            ))),
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
pub fn directories(
    path: impl AsRef<Path>,
) -> Result<impl Iterator<Item = PathBuf>, std::io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
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
pub fn entries(path: impl AsRef<Path>) -> Result<impl Iterator<Item = PathBuf>, std::io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
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
pub fn files(path: impl AsRef<Path>) -> Result<impl Iterator<Item = PathBuf>, std::io::Error> {
    let entries = match path.as_ref().read_dir() {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
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

/// Checks if the grandparent directory of the given executable is the base
/// of a virtual environment.
///
/// The procedure described in PEP 405 includes checking both the parent and
/// grandparent directory of an executable, but in practice we've found this to
/// be unnecessary.
pub fn is_virtualenv_executable(executable: impl AsRef<Path>) -> bool {
    executable
        .as_ref()
        .parent()
        .and_then(Path::parent)
        .is_some_and(is_virtualenv_base)
}

/// Returns `true` if a path is the base path of a virtual environment,
/// indicated by the presence of a `pyvenv.cfg` file.
///
/// The procedure described in PEP 405 includes scanning `pyvenv.cfg`
/// for a `home` key, but in practice we've found this to be
/// unnecessary.
pub fn is_virtualenv_base(path: impl AsRef<Path>) -> bool {
    path.as_ref().join("pyvenv.cfg").is_file()
}

/// Whether the error is due to a lock being held.
fn is_known_already_locked_error(err: &std::fs::TryLockError) -> bool {
    match err {
        std::fs::TryLockError::WouldBlock => true,
        std::fs::TryLockError::Error(err) => {
            // On Windows, we've seen: Os { code: 33, kind: Uncategorized, message: "The process cannot access the file because another process has locked a portion of the file." }
            if cfg!(windows) && err.raw_os_error() == Some(33) {
                return true;
            }
            false
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
