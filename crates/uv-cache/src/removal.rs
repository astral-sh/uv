//! Derived from Cargo's `clean` implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/e1ebce1035f9b53bb46a55bd4b0ecf51e24c6458/src/cargo/ops/cargo_clean.rs#L324>

use std::io;
use std::path::Path;

/// Remove a file or directory and all its contents, returning a [`Removal`] with
/// the number of files and directories removed, along with a total byte count.
pub(crate) fn rm_rf(path: impl AsRef<Path>) -> io::Result<Removal> {
    let mut removal = Removal::default();
    removal.rm_rf(path.as_ref())?;
    Ok(removal)
}

#[derive(Debug, Default)]
pub struct Removal {
    /// The number of files removed.
    pub num_files: u64,
    /// The number of directories removed.
    pub num_dirs: u64,
    /// The total number of bytes removed.
    ///
    /// Note: this will both over-count bytes removed for hard-linked files, and under-count
    /// bytes in general since it's a measure of the exact byte size (as opposed to the block size).
    pub total_bytes: u64,
}

impl Removal {
    /// Recursively remove a file or directory and all its contents.
    fn rm_rf(&mut self, path: &Path) -> io::Result<()> {
        let metadata = match fs_err::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        if !metadata.is_dir() {
            self.num_files += 1;

            // Remove the file.
            self.total_bytes += metadata.len();
            if cfg!(windows) && metadata.is_symlink() {
                // Remove the junction.
                remove_dir(path)?;
            } else {
                remove_file(path)?;
            }

            return Ok(());
        }

        for entry in walkdir::WalkDir::new(path).contents_first(true) {
            // If we hit a directory that lacks read permissions, try to make it readable.
            if let Err(ref err) = entry {
                if err
                    .io_error()
                    .is_some_and(|err| err.kind() == io::ErrorKind::PermissionDenied)
                {
                    if let Some(dir) = err.path() {
                        if set_readable(dir).unwrap_or(false) {
                            // Retry the operation; if we _just_ `self.rm_rf(dir)` and continue,
                            // `walkdir` may give us duplicate entries for the directory.
                            return self.rm_rf(path);
                        }
                    }
                }
            }

            let entry = entry?;
            if cfg!(windows) && entry.file_type().is_symlink() {
                // Remove the junction.
                self.num_files += 1;
                remove_dir(entry.path())?;
            } else if entry.file_type().is_dir() {
                self.num_dirs += 1;

                // The contents should have been removed by now, but sometimes a race condition is
                // hit where other files have been added by the OS. Fall back to `remove_dir_all`,
                // which will remove the directory robustly across platforms.
                remove_dir_all(entry.path())?;
            } else {
                self.num_files += 1;

                // Remove the file.
                if let Ok(meta) = entry.metadata() {
                    self.total_bytes += meta.len();
                }
                remove_file(entry.path())?;
            }
        }

        Ok(())
    }
}

impl std::ops::AddAssign for Removal {
    fn add_assign(&mut self, other: Self) {
        self.num_files += other.num_files;
        self.num_dirs += other.num_dirs;
        self.total_bytes += other.total_bytes;
    }
}

/// If the directory isn't readable by the current user, change the permissions to make it readable.
#[cfg_attr(windows, allow(unused_variables, clippy::unnecessary_wraps))]
fn set_readable(path: &Path) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs_err::metadata(path)?.permissions();
        if perms.mode() & 0o500 == 0 {
            perms.set_mode(perms.mode() | 0o500);
            fs_err::set_permissions(path, perms)?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// If the file is readonly, change the permissions to make it _not_ readonly.
fn set_not_readonly(path: &Path) -> io::Result<bool> {
    let mut perms = fs_err::metadata(path)?.permissions();
    if !perms.readonly() {
        return Ok(false);
    }

    // We're about to delete the file, so it's fine to set the permissions to world-writable.
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);

    fs_err::set_permissions(path, perms)?;

    Ok(true)
}

/// Like [`fs_err::remove_file`], but attempts to change the permissions to force the file to be
/// deleted (if it is readonly).
fn remove_file(path: &Path) -> io::Result<()> {
    match fs_err::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_not_readonly(path).unwrap_or(false) =>
        {
            fs_err::remove_file(path)
        }
        Err(err) => Err(err),
    }
}

/// Like [`fs_err::remove_dir`], but attempts to change the permissions to force the directory to
/// be deleted (if it is readonly).
fn remove_dir(path: &Path) -> io::Result<()> {
    match fs_err::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_readable(path).unwrap_or(false) =>
        {
            fs_err::remove_dir(path)
        }
        Err(err) => Err(err),
    }
}

/// Like [`fs_err::remove_dir_all`], but attempts to change the permissions to force the directory
/// to be deleted (if it is readonly).
fn remove_dir_all(path: &Path) -> io::Result<()> {
    match fs_err::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_readable(path).unwrap_or(false) =>
        {
            fs_err::remove_dir_all(path)
        }
        Err(err) => Err(err),
    }
}
