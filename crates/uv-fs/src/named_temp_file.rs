use std::fs::Permissions;
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
    tempfile_in_with_permissions(path, None)
}

/// Create a temporary file with permissions no broader than the requested permissions.
///
/// On Unix, the umask can further restrict the initial permissions. Set the final permissions
/// after writing when replacing an existing file whose mode must be retained.
/// Without explicit permissions, use the defaults from [`tempfile_in`].
pub(crate) fn tempfile_in_with_permissions(
    path: &Path,
    permissions: Option<Permissions>,
) -> io::Result<NamedTempFile> {
    #[cfg(unix)]
    let permissions = Some(permissions.unwrap_or_else(|| Permissions::from_mode(0o666)));
    let mut builder = tempfile::Builder::new();
    if let Some(permissions) = permissions {
        builder.permissions(permissions);
    }
    Ok(NamedTempFile(builder.tempfile_in(verbatim_path(path))?))
}

impl NamedTempFile {
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
        self.0.path()
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
