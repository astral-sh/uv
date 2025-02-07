use std::borrow::Cow;
use std::path::{Path, PathBuf};
use uv_fs::relative_to;

/// Create a symlink at `dst` pointing to `src`, replacing any existing symlink.
///
/// On Windows, we emulate symlinks by writing a file with the target path.
#[cfg(windows)]
pub fn create_link(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    // Use a relative path, if possible.
    let target = relative_to(&src, &dst).map(Cow::Owned).unwrap_or(Cow::Borrowed(src.as_ref()));

    // First, attempt to create a file at the location, but fail if it already exists.
    match fs_err::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst.as_ref())
    {
        Ok(mut file) => {
            // Write the target path to the file.
            use std::io::Write;
            file.write_all(target.to_string_lossy().as_bytes())?;
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            // Write to a temporary file, then move it into place.
            let temp_dir = tempfile::tempdir_in(dst.as_ref().parent().unwrap())?;
            let temp_file = temp_dir.path().join("link");
            fs_err::write(&temp_file, target.to_string_lossy().as_bytes())?;

            // Move the symlink into the target location.
            fs_err::rename(&temp_file, dst.as_ref())?;

            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Create a symlink at `dst` pointing to `src`, replacing any existing symlink if necessary.
///
/// On Unix, this method creates a temporary file, then moves it into place.
///
/// TODO(charlie): Consider using the `rust-atomicwrites` crate.
#[cfg(unix)]
pub fn create_link(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
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

/// Canonicalize a symlink, returning the fully-resolved path.
///
/// If the symlink target does not exist, returns an error.
#[cfg(unix)]
pub fn resolve_link(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    path.as_ref().canonicalize()
}

/// Canonicalize a symlink, returning the fully-resolved path.
///
/// If the symlink target does not exist, returns an error.
#[cfg(windows)]
pub fn resolve_link(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    // On Windows, we emulate symlinks by writing a file with the target path.
    let contents = fs_err::read_to_string(path.as_ref())?;
    let path = path.as_ref().join(contents.trim());
    path.canonicalize()
}
