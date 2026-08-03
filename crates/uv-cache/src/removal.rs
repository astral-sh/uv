//! Derived from Cargo's `clean` implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/e1ebce1035f9b53bb46a55bd4b0ecf51e24c6458/src/cargo/ops/cargo_clean.rs#L324>

use std::io;
use std::path::Path;

use tracing::debug;

use crate::CleanReporter;

/// A builder for a [`Remover`] that can remove files and directories.
#[derive(Default)]
pub(crate) struct Remover {
    reporter: Option<Box<dyn CleanReporter>>,
    measure_reclaimed_space: bool,
}

impl Remover {
    /// Create a new [`Remover`] with the given reporter.
    pub(crate) fn new(reporter: Box<dyn CleanReporter>) -> Self {
        Self {
            reporter: Some(reporter),
            measure_reclaimed_space: false,
        }
    }

    /// Enable accounting for exclusively owned storage before each file is removed.
    pub(crate) fn with_reclaimed_space(mut self, enabled: bool) -> Self {
        self.measure_reclaimed_space = enabled;
        self
    }

    /// Remove a file or directory and all its contents, returning a [`Removal`] with
    /// the number of files and directories removed, along with a total byte count.
    pub(crate) fn rm_rf(
        &self,
        path: impl AsRef<Path>,
        skip_locked_file: bool,
    ) -> io::Result<Removal> {
        let mut removal = Removal::new(self.measure_reclaimed_space);
        removal.rm_rf(path.as_ref(), self.reporter.as_deref(), skip_locked_file)?;
        Ok(removal)
    }
}

/// A removal operation with statistics on the number of files and directories removed.
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
    /// The exclusively owned allocated file data reclaimed by the removal, when available.
    pub reclaimed_bytes: Option<u64>,
    /// Whether any removed entries could not be measured, making the reclaimed count a lower bound.
    pub reclaimed_bytes_incomplete: bool,
}

impl Removal {
    /// Create an empty removal summary with optional per-file reclaimed-space accounting.
    pub(crate) fn new(measure_reclaimed_space: bool) -> Self {
        Self {
            reclaimed_bytes: measure_reclaimed_space.then_some(0),
            ..Self::default()
        }
    }

    /// Account for a file while its current sharing state can still be inspected.
    fn add_file(&mut self, path: &Path, metadata: &std::fs::Metadata) {
        self.total_bytes += metadata.len();

        if let Some(reclaimed_bytes) = self.reclaimed_bytes {
            match uv_fs::reclaimable_space(path, metadata) {
                Ok(reclaimable) => {
                    self.reclaimed_bytes = Some(reclaimed_bytes.saturating_add(reclaimable));
                }
                Err(error) => {
                    debug!(
                        "Failed to measure reclaimed space for {}: {error}",
                        path.display()
                    );
                    self.reclaimed_bytes_incomplete = true;
                }
            }
        }
    }

    /// Recursively remove a file or directory and all its contents.
    fn rm_rf(
        &mut self,
        path: &Path,
        reporter: Option<&dyn CleanReporter>,
        skip_locked_file: bool,
    ) -> io::Result<()> {
        let path = uv_fs::verbatim_path(path);

        let metadata = match fs_err::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        if !metadata.is_dir() {
            self.num_files += 1;

            // Remove the file.
            self.add_file(&path, &metadata);
            if metadata.is_symlink() {
                cfg_select! {
                    windows => {
                        use std::os::windows::fs::FileTypeExt;

                        if metadata.file_type().is_symlink_dir() {
                            remove_dir(&path)?;
                        } else {
                            remove_file(&path)?;
                        }
                    },
                    _ => {
                        remove_file(&path)?;
                    },
                }
            } else {
                remove_file(&path)?;
            }

            reporter.map(CleanReporter::on_clean);

            return Ok(());
        }

        for entry in walkdir::WalkDir::new(&path).contents_first(true) {
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
                            return self.rm_rf(&path, reporter, skip_locked_file);
                        }
                    }
                }
            }

            let entry = entry?;

            // Remove the exclusive lock last.
            if skip_locked_file
                && entry.file_name() == ".lock"
                && entry
                    .path()
                    .strip_prefix(&path)
                    .is_ok_and(|suffix| suffix == Path::new(".lock"))
            {
                continue;
            }

            if entry.file_type().is_symlink() && {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::FileTypeExt;
                    entry.file_type().is_symlink_dir()
                }
                #[cfg(not(windows))]
                {
                    false
                }
            } {
                self.num_files += 1;
                remove_dir(entry.path())?;
            } else if entry.file_type().is_dir() {
                // Remove the directory with the exclusive lock last.
                if skip_locked_file && entry.path() == path.as_ref() {
                    continue;
                }

                self.num_dirs += 1;

                // The contents should have been removed by now, but sometimes a race condition is
                // hit where other files have been added by the OS. Fall back to `remove_dir_all`,
                // which will remove the directory robustly across platforms.
                remove_dir_all(entry.path())?;
            } else {
                self.num_files += 1;

                // Remove the file.
                if let Ok(metadata) = entry.metadata() {
                    self.add_file(entry.path(), &metadata);
                } else if self.reclaimed_bytes.is_some() {
                    self.reclaimed_bytes_incomplete = true;
                }
                remove_file(entry.path())?;
            }

            reporter.map(CleanReporter::on_clean);
        }

        reporter.map(CleanReporter::on_complete);

        Ok(())
    }
}

impl std::ops::AddAssign for Removal {
    fn add_assign(&mut self, other: Self) {
        self.num_files += other.num_files;
        self.num_dirs += other.num_dirs;
        self.total_bytes += other.total_bytes;
        self.reclaimed_bytes = self
            .reclaimed_bytes
            .zip(other.reclaimed_bytes)
            .map(|(left, right)| left.saturating_add(right));
        self.reclaimed_bytes_incomplete |= other.reclaimed_bytes_incomplete;
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", target_os = "ios")))]
mod tests {
    use super::Removal;

    #[test]
    fn retain_measured_space_when_an_entry_cannot_be_measured() -> std::io::Result<()> {
        let directory = tempfile::tempdir()?;
        let measured = directory.path().join("measured.bin");
        fs_err::write(&measured, vec![42; 4096])?;
        let metadata = fs_err::metadata(&measured)?;
        let expected = uv_fs::reclaimable_space(&measured, &metadata)?;

        let mut removal = Removal::new(true);
        removal.add_file(&measured, &metadata);
        removal.add_file(&directory.path().join("missing.bin"), &metadata);
        removal.add_file(&measured, &metadata);

        assert_eq!(removal.reclaimed_bytes, Some(expected.saturating_mul(2)));
        assert!(removal.reclaimed_bytes_incomplete);

        let mut combined = Removal::new(true);
        combined += removal;
        assert_eq!(combined.reclaimed_bytes, Some(expected.saturating_mul(2)));
        assert!(combined.reclaimed_bytes_incomplete);

        Ok(())
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
    #[expect(clippy::permissions_set_readonly_false)]
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
