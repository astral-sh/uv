use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use thiserror::Error;

use crate::verbatim_path;

/// A temporary file that supports long Windows paths without the process's long-path opt-in.
///
/// On Windows, both its temporary path and its persist destination use verbatim paths, since
/// `tempfile` passes them directly to Win32 APIs. The file is removed on drop unless it is persisted.
#[derive(Debug)]
pub struct NamedTempFile(tempfile::NamedTempFile);

/// Return a [`NamedTempFile`] in the specified directory.
///
/// On Windows, stores a verbatim path so later persistence supports long paths.
/// On Unix, requests `0o666` permissions (subject to the umask), matching non-temporary files.
pub fn tempfile_in(path: &Path) -> io::Result<NamedTempFile> {
    #[cfg(unix)]
    let file = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o666))
        .tempfile_in(path)?;
    #[cfg(not(unix))]
    let file = tempfile::Builder::new().tempfile_in(verbatim_path(path))?;
    Ok(NamedTempFile(file))
}

impl NamedTempFile {
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    #[expect(clippy::disallowed_types, reason = "tempfile exposes a std::fs::File")]
    pub fn as_file(&self) -> &std::fs::File {
        self.0.as_file()
    }

    /// Persist the temporary file, atomically replacing any existing file at `path`.
    ///
    /// The destination must be on the same filesystem. This does not synchronize the file contents
    /// or the containing directory to disk.
    ///
    /// On failure, the returned [`PersistError`] retains the temporary file so callers can retry.
    #[expect(clippy::disallowed_types, reason = "tempfile exposes a std::fs::File")]
    pub fn persist(self, path: impl AsRef<Path>) -> Result<std::fs::File, PersistError> {
        self.0
            .persist(verbatim_path(path.as_ref()))
            .map_err(|error| PersistError {
                error: error.error,
                file: Self(error.file),
            })
    }
}

impl AsRef<Path> for NamedTempFile {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Write for NamedTempFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// An error persisting a [`NamedTempFile`], retaining the file for another attempt.
#[derive(Debug, Error)]
#[error("failed to persist temporary file: {error}")]
pub struct PersistError {
    #[source]
    pub error: io::Error,
    pub file: NamedTempFile,
}
