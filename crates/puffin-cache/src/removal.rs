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

        let mut rm_file = |path: &Path, meta: Result<std::fs::Metadata, walkdir::Error>| {
            if let Ok(meta) = meta {
                self.total_bytes += meta.len();
            }
            remove_file(path)?;

            Ok(())
        };

        if !metadata.is_dir() {
            self.num_files += 1;
            return rm_file(path, Ok(metadata));
        }

        for entry in walkdir::WalkDir::new(path).contents_first(true) {
            let entry = entry?;
            if cfg!(windows) && entry.file_type().is_symlink() {
                // In this branch, we try to handle junction removal.
                self.num_files += 1;
                fs_err::remove_dir(entry.path())?;
            } else if entry.file_type().is_dir() {
                self.num_dirs += 1;

                // The contents should have been removed by now, but sometimes a race condition is
                // hit where other files have been added by the OS. Fall back to `remove_dir_all`,
                // which will remove the directory robustly across platforms.
                fs_err::remove_dir_all(entry.path())?;
            } else {
                self.num_files += 1;
                rm_file(entry.path(), entry.metadata())?;
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

/// Like [`fs_err::remove_file`], but attempts to change the permissions to force the file to be
/// deleted (if it is readonly).
fn remove_file(path: &Path) -> io::Result<()> {
    /// If the file is readonly, change the permissions to make it _not_ readonly.
    fn set_not_readonly(path: &Path) -> io::Result<bool> {
        let mut perms = path.metadata()?.permissions();
        if !perms.readonly() {
            return Ok(false);
        }

        // We're about to delete the file, so it's fine to set the permissions to world-writable.
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);

        fs_err::set_permissions(path, perms)?;

        Ok(true)
    }

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
