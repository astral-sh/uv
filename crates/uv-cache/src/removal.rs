//! Derived from Cargo's `clean` implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/e1ebce1035f9b53bb46a55bd4b0ecf51e24c6458/src/cargo/ops/cargo_clean.rs#L324>

use std::io;
use std::path::Path;

use tracing::debug;

use crate::CleanReporter;

/// The storage accounting used when removing cache entries.
#[derive(Debug, Clone, Copy, Default)]
pub enum RemovalMode {
    /// Report the logical size of the removed files.
    #[default]
    Logical,
    /// Report the exclusively owned physical storage reclaimed by the removed files.
    Physical,
}

/// A builder for a [`Remover`] that can remove files and directories.
#[derive(Default)]
pub(crate) struct Remover {
    reporter: Option<Box<dyn CleanReporter>>,
    removal_mode: RemovalMode,
}

impl Remover {
    /// Create a new [`Remover`] with the given reporter.
    pub(crate) fn new(reporter: Box<dyn CleanReporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..Self::default()
        }
    }

    /// Set the storage accounting used before each file is removed.
    pub(crate) fn with_removal_mode(mut self, removal_mode: RemovalMode) -> Self {
        self.removal_mode = removal_mode;
        self
    }

    /// Remove a file or directory and all its contents, returning a [`Removal`] with
    /// the number of files and directories removed, along with a total byte count.
    pub(crate) fn rm_rf(
        &self,
        path: impl AsRef<Path>,
        skip_locked_file: bool,
    ) -> io::Result<Removal> {
        let mut removal = Removal::new(self.removal_mode);
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
    /// The logical number of bytes removed.
    ///
    /// Note: this will both over-count bytes removed for hard-linked files, and under-count
    /// bytes in general since it's a measure of the exact byte size (as opposed to the block size).
    pub logical_bytes: u64,
    /// The exclusively owned physical file data reclaimed by the removal, when available.
    pub physical_bytes: Option<u64>,
    /// Whether any removed entries could not be measured, making the physical count a lower bound.
    pub physical_bytes_incomplete: bool,
}

impl Removal {
    /// Create an empty removal summary with the requested storage accounting.
    pub(crate) fn new(removal_mode: RemovalMode) -> Self {
        Self {
            physical_bytes: match removal_mode {
                RemovalMode::Logical => None,
                RemovalMode::Physical => Some(0),
            },
            ..Self::default()
        }
    }

    /// Account for a file while its current sharing state can still be inspected.
    fn add_file(&mut self, path: &Path, metadata: &std::fs::Metadata) {
        self.logical_bytes += metadata.len();

        if let Some(physical_bytes) = self.physical_bytes {
            match uv_fs::physical_space(path, metadata) {
                Ok(physical) => {
                    self.physical_bytes = Some(physical_bytes.saturating_add(physical));
                }
                Err(error) => {
                    debug!(
                        "Failed to measure physical space for {}: {error}",
                        path.display()
                    );
                    self.physical_bytes_incomplete = true;
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
            // Capture file metadata up front so overlong files share the directory error handling.
            let mut metadata = None;
            let entry = match entry {
                Ok(entry) if !entry.file_type().is_dir() => match entry.metadata() {
                    Ok(entry_metadata) => {
                        metadata = Some(entry_metadata);
                        Ok(entry)
                    }
                    #[cfg(target_os = "macos")]
                    Err(error)
                        if error.io_error().is_some_and(|error| {
                            error.kind() == io::ErrorKind::InvalidFilename
                        }) =>
                    {
                        Err(error)
                    }
                    Err(_) => Ok(entry),
                },
                entry => entry,
            };

            if let Err(ref err) = entry {
                // On Unix, `ENAMETOOLONG` is the only OS error mapped to `InvalidFilename`.
                // NOTE: In the future, we may want to extend this to Linux and other targets,
                // although it's less likely there given that Linux's `MAX_PATH` is 4096 instead
                // of macOS's 1024.
                #[cfg(target_os = "macos")]
                if err
                    .io_error()
                    .is_some_and(|error| error.kind() == io::ErrorKind::InvalidFilename)
                    && let Some(parent) = err.path().and_then(Path::parent)
                    && parent != path.as_ref()
                {
                    return self.rm_rf_overlong_subtree(&path, parent, reporter, skip_locked_file);
                }

                // If we hit a directory that lacks read permissions, try to make it readable.
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
                remove_dir(entry.path())?;
                self.num_files += 1;
            } else if entry.file_type().is_dir() {
                // Remove the directory with the exclusive lock last.
                if skip_locked_file && entry.path() == path.as_ref() {
                    continue;
                }

                // The contents should have been removed by now, but sometimes a race condition is
                // hit where other files have been added by the OS. Fall back to `remove_dir_all`,
                // which will remove the directory robustly across platforms.
                remove_dir_all(entry.path())?;
                self.num_dirs += 1;
            } else {
                // Remove the file.
                if let Some(metadata) = &metadata {
                    self.add_file(entry.path(), metadata);
                } else if self.physical_bytes.is_some() {
                    self.physical_bytes_incomplete = true;
                }

                remove_file(entry.path())?;
                self.num_files += 1;
            }

            reporter.map(CleanReporter::on_clean);
        }

        reporter.map(CleanReporter::on_complete);

        Ok(())
    }

    /// Remove an overlong subtree with descriptor-relative operations, then restart the walker.
    #[cfg(target_os = "macos")]
    fn rm_rf_overlong_subtree(
        &mut self,
        root: &Path,
        directory: &Path,
        reporter: Option<&dyn CleanReporter>,
        skip_locked_file: bool,
    ) -> io::Result<()> {
        // `remove_dir_all` uses `openat` and `unlinkat`, so it can remove descendants whose
        // complete paths exceed `PATH_MAX`. It does not report its contents, so only count the
        // directory passed to it.
        remove_dir_all(directory)?;
        self.num_dirs += 1;
        if self.physical_bytes.is_some() {
            self.physical_bytes_incomplete = true;
        }
        reporter.map(CleanReporter::on_clean);

        // Restart because `walkdir` may otherwise yield entries from the removed directory.
        self.rm_rf(root, reporter, skip_locked_file)
    }
}

impl std::ops::AddAssign for Removal {
    fn add_assign(&mut self, other: Self) {
        self.num_files += other.num_files;
        self.num_dirs += other.num_dirs;
        self.logical_bytes += other.logical_bytes;
        self.physical_bytes = self
            .physical_bytes
            .zip(other.physical_bytes)
            .map(|(left, right)| left.saturating_add(right));
        self.physical_bytes_incomplete |= other.physical_bytes_incomplete;
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
