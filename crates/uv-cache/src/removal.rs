//! Derived from Cargo's `clean` implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/e1ebce1035f9b53bb46a55bd4b0ecf51e24c6458/src/cargo/ops/cargo_clean.rs#L324>

use std::io;
use std::path::Path;

use crate::CleanReporter;

/// Remove a file or directory and all its contents, returning a [`Removal`] with
/// the number of files and directories removed, along with a total byte count.
pub fn rm_rf(path: impl AsRef<Path>) -> io::Result<Removal> {
    Remover::default().rm_rf(path, false)
}

/// A builder for a [`Remover`] that can remove files and directories.
#[derive(Default)]
pub(crate) struct Remover {
    reporter: Option<Box<dyn CleanReporter>>,
}

impl Remover {
    /// Create a new [`Remover`] with the given reporter.
    pub(crate) fn new(reporter: Box<dyn CleanReporter>) -> Self {
        Self {
            reporter: Some(reporter),
        }
    }

    /// Remove a file or directory and all its contents, returning a [`Removal`] with
    /// the number of files and directories removed, along with a total byte count.
    pub(crate) fn rm_rf(
        &self,
        path: impl AsRef<Path>,
        skip_locked_file: bool,
    ) -> io::Result<Removal> {
        let mut removal = Removal::default();
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
}

impl Removal {
    /// Recursively remove a file or directory and all its contents.
    fn rm_rf(
        &mut self,
        path: &Path,
        reporter: Option<&dyn CleanReporter>,
        skip_locked_file: bool,
    ) -> io::Result<()> {
        let metadata = match fs_err::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        if !metadata.is_dir() {
            self.num_files += 1;

            // Remove the file.
            self.total_bytes += metadata.len();
            if metadata.is_symlink() {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::FileTypeExt;

                    if metadata.file_type().is_symlink_dir() {
                        remove_dir(path)?;
                    } else {
                        remove_file(path)?;
                    }
                }

                #[cfg(not(windows))]
                {
                    remove_file(path)?;
                }
            } else {
                remove_file(path)?;
            }

            reporter.map(CleanReporter::on_clean);

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
                            return self.rm_rf(path, reporter, skip_locked_file);
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
                    .strip_prefix(path)
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
                if skip_locked_file && entry.path() == path {
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
                if let Ok(meta) = entry.metadata() {
                    self.total_bytes += meta.len();
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

/// Convert a path to an extended-length path on Windows.
///
/// On Windows, the extended-length path prefix (`\\?\`) allows operating on paths that:
/// - Contain special characters (like trailing dots or spaces) that are normally invalid
/// - Exceed the `MAX_PATH` limit
///
/// On non-Windows systems, this is a no-op that returns the original path.
#[cfg(windows)]
fn to_extended_path(path: &Path) -> std::borrow::Cow<'_, Path> {
    use std::path::PathBuf;

    // If the path already has the extended prefix, return it as-is
    let path_str = path.to_string_lossy();
    if path_str.starts_with(r"\\?") {
        return std::borrow::Cow::Borrowed(path);
    }

    // Convert to absolute path if it's relative
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            // If current_dir() fails, we can't create a valid extended path
            Err(_) => return std::borrow::Cow::Borrowed(path),
        }
    };

    let abs_str = abs_path.to_string_lossy();

    // Handle UNC paths: \\server\share -> \\?\UNC\server\share
    let extended = if let Some(stripped) = abs_str.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{stripped}"))
    } else {
        PathBuf::from(format!(r"\\?\{}", abs_path.display()))
    };

    std::borrow::Cow::Owned(extended)
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn to_extended_path(path: &Path) -> std::borrow::Cow<'_, Path> {
    std::borrow::Cow::Borrowed(path)
}

/// Like [`fs_err::remove_file`], but attempts to change the permissions to force the file to be
/// deleted (if it is readonly). On Windows, also attempts to use extended-length paths to handle
/// files with special characters (like trailing dots).
fn remove_file(path: &Path) -> io::Result<()> {
    match fs_err::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_not_readonly(path).unwrap_or(false) =>
        {
            fs_err::remove_file(path)
        }
        #[cfg(windows)]
        Err(err)
            if err.kind() == io::ErrorKind::NotFound
                || err.kind() == io::ErrorKind::InvalidInput =>
        {
            // On Windows, files with special characters (like trailing dots) may fail with
            // NotFound or InvalidInput errors due to path normalization. Try using the
            // extended-length path prefix (\\?\) to bypass this normalization.
            let extended_path = to_extended_path(path);
            if extended_path.as_ref() != path {
                match fs_err::remove_file(extended_path.as_ref()) {
                    Ok(()) => return Ok(()),
                    // Handle the case where file has special chars AND is readonly
                    Err(e)
                        if e.kind() == io::ErrorKind::PermissionDenied
                            && set_not_readonly(extended_path.as_ref()).unwrap_or(false) =>
                    {
                        return fs_err::remove_file(extended_path.as_ref()).or(Err(err));
                    }
                    Err(_) => {}
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

/// Like [`fs_err::remove_dir`], but attempts to change the permissions to force the directory to
/// be deleted (if it is readonly). On Windows, also attempts to use extended-length paths to handle
/// directories with special characters (like trailing dots).
fn remove_dir(path: &Path) -> io::Result<()> {
    match fs_err::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_readable(path).unwrap_or(false) =>
        {
            fs_err::remove_dir(path)
        }
        #[cfg(windows)]
        Err(err)
            if err.kind() == io::ErrorKind::NotFound
                || err.kind() == io::ErrorKind::InvalidInput =>
        {
            let extended_path = to_extended_path(path);
            if extended_path.as_ref() != path {
                match fs_err::remove_dir(extended_path.as_ref()) {
                    Ok(()) => return Ok(()),
                    Err(e)
                        if e.kind() == io::ErrorKind::PermissionDenied
                            && set_readable(extended_path.as_ref()).unwrap_or(false) =>
                    {
                        return fs_err::remove_dir(extended_path.as_ref()).or(Err(err));
                    }
                    Err(_) => {}
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

/// Like [`fs_err::remove_dir_all`], but attempts to change the permissions to force the directory
/// to be deleted (if it is readonly). On Windows, also attempts to use extended-length paths to
/// handle directories with special characters (like trailing dots).
fn remove_dir_all(path: &Path) -> io::Result<()> {
    match fs_err::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err)
            if err.kind() == io::ErrorKind::PermissionDenied
                && set_readable(path).unwrap_or(false) =>
        {
            fs_err::remove_dir_all(path)
        }
        #[cfg(windows)]
        Err(err)
            if err.kind() == io::ErrorKind::NotFound
                || err.kind() == io::ErrorKind::InvalidInput =>
        {
            let extended_path = to_extended_path(path);
            if extended_path.as_ref() != path {
                match fs_err::remove_dir_all(extended_path.as_ref()) {
                    Ok(()) => return Ok(()),
                    Err(e)
                        if e.kind() == io::ErrorKind::PermissionDenied
                            && set_readable(extended_path.as_ref()).unwrap_or(false) =>
                    {
                        return fs_err::remove_dir_all(extended_path.as_ref()).or(Err(err));
                    }
                    Err(_) => {}
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_normal_file() {
        // Sanity check: normal file removal should still work
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_file = temp_dir.path().join("normal_file.txt");

        fs_err::write(&test_file, "test").expect("Failed to write test file");
        assert!(test_file.exists(), "Test file should exist before removal");

        remove_file(&test_file).expect("Failed to remove normal file");
        assert!(!test_file.exists(), "File should be deleted after removal");
    }

    #[test]
    fn test_remove_readonly_file() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_file = temp_dir.path().join("readonly_file.txt");

        fs_err::write(&test_file, "test").expect("Failed to write test file");

        // Make the file readonly
        let mut perms = fs_err::metadata(&test_file)
            .expect("Failed to get metadata")
            .permissions();
        perms.set_readonly(true);
        fs_err::set_permissions(&test_file, perms).expect("Failed to set permissions");

        assert!(test_file.exists(), "Test file should exist before removal");

        remove_file(&test_file).expect("Failed to remove readonly file");
        assert!(
            !test_file.exists(),
            "Readonly file should be deleted after removal"
        );
    }

    #[test]
    fn test_remove_normal_dir() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().join("test_dir");

        fs_err::create_dir(&test_dir).expect("Failed to create test dir");
        assert!(test_dir.exists(), "Test dir should exist before removal");

        remove_dir(&test_dir).expect("Failed to remove dir");
        assert!(!test_dir.exists(), "Dir should be deleted after removal");
    }

    #[test]
    fn test_remove_dir_all_with_contents() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().join("test_dir");
        let sub_dir = test_dir.join("sub_dir");
        let test_file = test_dir.join("file.txt");
        let sub_file = sub_dir.join("sub_file.txt");

        fs_err::create_dir_all(&sub_dir).expect("Failed to create dirs");
        fs_err::write(&test_file, "test").expect("Failed to write test file");
        fs_err::write(&sub_file, "sub test").expect("Failed to write sub file");

        assert!(test_dir.exists(), "Test dir should exist before removal");

        remove_dir_all(&test_dir).expect("Failed to remove dir_all");
        assert!(
            !test_dir.exists(),
            "Dir and contents should be deleted after removal"
        );
    }

    #[test]
    fn test_rm_rf_file() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_file = temp_dir.path().join("test_file.txt");

        fs_err::write(&test_file, "hello world").expect("Failed to write test file");
        assert!(test_file.exists(), "Test file should exist before removal");

        let removal = rm_rf(&test_file).expect("Failed to rm_rf file");
        assert!(!test_file.exists(), "File should be deleted after rm_rf");
        assert_eq!(removal.num_files, 1);
        assert_eq!(removal.num_dirs, 0);
        assert_eq!(removal.total_bytes, 11); // "hello world" = 11 bytes
    }

    #[test]
    fn test_rm_rf_directory() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().join("test_dir");
        let sub_dir = test_dir.join("sub_dir");
        let file1 = test_dir.join("file1.txt");
        let file2 = sub_dir.join("file2.txt");

        fs_err::create_dir_all(&sub_dir).expect("Failed to create dirs");
        fs_err::write(&file1, "test1").expect("Failed to write file1");
        fs_err::write(&file2, "test2").expect("Failed to write file2");

        assert!(test_dir.exists(), "Test dir should exist before removal");

        let removal = rm_rf(&test_dir).expect("Failed to rm_rf directory");
        assert!(!test_dir.exists(), "Dir should be deleted after rm_rf");
        assert_eq!(removal.num_files, 2);
        assert!(removal.num_dirs >= 1); // At least the subdirectory
        assert_eq!(removal.total_bytes, 10); // "test1" + "test2" = 10 bytes
    }

    #[test]
    fn test_rm_rf_nonexistent() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let nonexistent = temp_dir.path().join("nonexistent");

        // Should not error on nonexistent path
        let removal = rm_rf(&nonexistent).expect("rm_rf should succeed on nonexistent path");
        assert_eq!(removal.num_files, 0);
        assert_eq!(removal.num_dirs, 0);
        assert_eq!(removal.total_bytes, 0);
    }

    #[test]
    #[cfg(windows)]
    fn test_to_extended_path_absolute() {
        let path = Path::new(r"C:\Users\test\file.txt");
        let extended = to_extended_path(path);
        assert!(
            extended.to_string_lossy().starts_with(r"\\?\"),
            "Extended path should start with \\\\?\\, got: {}",
            extended.display()
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_to_extended_path_already_extended() {
        let path = Path::new(r"\\?\C:\Users\test\file.txt");
        let extended = to_extended_path(path);
        assert_eq!(
            extended.as_ref(),
            path,
            "Already extended path should be returned as-is"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_to_extended_path_unc() {
        let path = Path::new(r"\\server\share\file.txt");
        let extended = to_extended_path(path);
        assert!(
            extended.to_string_lossy().starts_with(r"\\?\UNC\"),
            "UNC path should be converted to extended UNC format, got: {}",
            extended.display()
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn test_to_extended_path_noop_on_unix() {
        let path = Path::new("/home/user/file.txt");
        let extended = to_extended_path(path);
        assert_eq!(
            extended.as_ref(),
            path,
            "On non-Windows, path should be returned unchanged"
        );
    }

    #[test]
    fn test_removal_add_assign() {
        let mut removal1 = Removal {
            num_files: 5,
            num_dirs: 2,
            total_bytes: 1000,
        };
        let removal2 = Removal {
            num_files: 3,
            num_dirs: 1,
            total_bytes: 500,
        };

        removal1 += removal2;

        assert_eq!(removal1.num_files, 8);
        assert_eq!(removal1.num_dirs, 3);
        assert_eq!(removal1.total_bytes, 1500);
    }
}
