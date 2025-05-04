use crate::Error;
use fs_err as fs;
use fs_err::DirEntry;
use reflink_copy as reflink;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tempfile::tempdir_in;
use tracing::{debug, instrument, trace};
use uv_warnings::warn_user_once;
use walkdir::WalkDir;

#[derive(Debug, Default)]
pub struct Locks(Mutex<FxHashMap<PathBuf, Arc<Mutex<()>>>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LinkMode {
    /// Clone (i.e., copy-on-write) packages from the wheel into the `site-packages` directory.
    Clone,
    /// Copy packages from the wheel into the `site-packages` directory.
    Copy,
    /// Hard link packages from the wheel into the `site-packages` directory.
    Hardlink,
    /// Symbolically link packages from the wheel into the `site-packages` directory.
    ///
    /// WARNING: The use of symlinks is discouraged, as they create tight coupling between the
    /// cache and the target environment. For example, clearing the cache (`uv cache clear`) will
    /// break all installed packages by way of removing the underlying source files. Use symlinks
    /// with caution.
    Symlink,
}

impl Default for LinkMode {
    fn default() -> Self {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Self::Clone
        } else {
            Self::Hardlink
        }
    }
}

impl LinkMode {
    /// Extract a wheel by linking all of its files into site packages.
    #[instrument(skip_all)]
    pub fn link_wheel_files(
        self,
        site_packages: impl AsRef<Path>,
        wheel: impl AsRef<Path>,
        locks: &Locks,
    ) -> Result<usize, Error> {
        match self {
            Self::Clone => clone_wheel_files(site_packages, wheel, locks),
            Self::Copy => copy_wheel_files(site_packages, wheel, locks),
            Self::Hardlink => hardlink_wheel_files(site_packages, wheel, locks),
            Self::Symlink => symlink_wheel_files(site_packages, wheel, locks),
        }
    }

    /// Returns `true` if the link mode is [`LinkMode::Symlink`].
    pub fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink)
    }
}

/// Extract a wheel by cloning all of its files into site packages. The files will be cloned
/// via copy-on-write, which is similar to a hard link, but allows the files to be modified
/// independently (that is, the file is copied upon modification).
///
/// This method uses `clonefile` on macOS, and `reflink` on Linux. See [`clone_recursive`] for
/// details.
fn clone_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
) -> Result<usize, Error> {
    let mut count = 0usize;
    let mut attempt = Attempt::default();

    for entry in fs::read_dir(wheel.as_ref())? {
        clone_recursive(
            site_packages.as_ref(),
            wheel.as_ref(),
            locks,
            &entry?,
            &mut attempt,
        )?;
        count += 1;
    }

    // The directory mtime is not updated when cloning and the mtime is used by CPython's
    // import mechanisms to determine if it should look for new packages in a directory.
    // Here, we force the mtime to be updated to ensure that packages are importable without
    // manual cache invalidation.
    //
    // <https://github.com/python/cpython/blob/8336cb2b6f428246803b02a4e97fce49d0bb1e09/Lib/importlib/_bootstrap_external.py#L1601>
    let now = SystemTime::now();

    // `File.set_modified` is not available in `fs_err` yet
    #[allow(clippy::disallowed_types)]
    match std::fs::File::open(site_packages.as_ref()) {
        Ok(dir) => {
            if let Err(err) = dir.set_modified(now) {
                debug!(
                    "Failed to update mtime for {}: {err}",
                    site_packages.as_ref().display()
                );
            }
        }
        Err(err) => debug!(
            "Failed to open {} to update mtime: {err}",
            site_packages.as_ref().display()
        ),
    }

    Ok(count)
}

// Hard linking / reflinking might not be supported but we (afaik) can't detect this ahead of time,
// so we'll try hard linking / reflinking the first file - if this succeeds we'll know later
// errors are not due to lack of os/fs support. If it fails, we'll switch to copying for the rest of the
// install.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Attempt {
    #[default]
    Initial,
    Subsequent,
    UseCopyFallback,
}

/// Recursively clone the contents of `from` into `to`.
///
/// Note the behavior here is platform-dependent.
///
/// On macOS, directories can be recursively copied with a single `clonefile` call. So we only
/// need to iterate over the top-level of the directory, and copy each file or subdirectory
/// unless the subdirectory exists already in which case we'll need to recursively merge its
/// contents with the existing directory.
///
/// On Linux, we need to always reflink recursively, as `FICLONE` ioctl does not support
/// directories. Also note, that reflink is only supported on certain filesystems (btrfs, xfs,
/// ...), and only when it does not cross filesystem boundaries.
///
/// On Windows, we also always need to reflink recursively, as `FSCTL_DUPLICATE_EXTENTS_TO_FILE`
/// ioctl is not supported on directories. Also, it is only supported on certain filesystems
/// (ReFS, SMB, ...).
fn clone_recursive(
    site_packages: &Path,
    wheel: &Path,
    locks: &Locks,
    entry: &DirEntry,
    attempt: &mut Attempt,
) -> Result<(), Error> {
    // Determine the existing and destination paths.
    let from = entry.path();
    let to = site_packages.join(from.strip_prefix(wheel).unwrap());

    trace!("Cloning {} to {}", from.display(), to.display());

    if (cfg!(windows) || cfg!(target_os = "linux")) && from.is_dir() {
        fs::create_dir_all(&to)?;
        for entry in fs::read_dir(from)? {
            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
        }
        return Ok(());
    }

    match attempt {
        Attempt::Initial => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    // If cloning or copying fails and the directory exists already, it must be
                    // merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        if reflink::reflink(&from, &tempfile).is_ok() {
                            fs::rename(&tempfile, to)?;
                        } else {
                            debug!(
                                "Failed to clone `{}` to temporary location `{}`, attempting to copy files as a fallback",
                                from.display(),
                                tempfile.display(),
                            );
                            *attempt = Attempt::UseCopyFallback;
                            synchronized_copy(&from, &to, locks)?;
                        }
                    }
                } else {
                    debug!(
                        "Failed to clone `{}` to `{}`, attempting to copy files as a fallback",
                        from.display(),
                        to.display()
                    );
                    // Fallback to copying
                    *attempt = Attempt::UseCopyFallback;
                    clone_recursive(site_packages, wheel, locks, entry, attempt)?;
                }
            }
        }
        Attempt::Subsequent => {
            if let Err(err) = reflink::reflink(&from, &to) {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    // If cloning/copying fails and the directory exists already, it must be merged recursively.
                    if entry.file_type()?.is_dir() {
                        for entry in fs::read_dir(from)? {
                            clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                        }
                    } else {
                        // If file already exists, overwrite it.
                        let tempdir = tempdir_in(site_packages)?;
                        let tempfile = tempdir.path().join(from.file_name().unwrap());
                        reflink::reflink(&from, &tempfile)?;
                        fs::rename(&tempfile, to)?;
                    }
                } else {
                    return Err(Error::Reflink { from, to, err });
                }
            }
        }
        Attempt::UseCopyFallback => {
            if entry.file_type()?.is_dir() {
                fs::create_dir_all(&to)?;
                for entry in fs::read_dir(from)? {
                    clone_recursive(site_packages, wheel, locks, &entry?, attempt)?;
                }
            } else {
                synchronized_copy(&from, &to, locks)?;
            }
            warn_user_once!("Failed to clone files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, reflinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.");
        }
    }

    if *attempt == Attempt::Initial {
        *attempt = Attempt::Subsequent;
    }
    Ok(())
}

/// Extract a wheel by copying all of its files into site packages.
fn copy_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
) -> Result<usize, Error> {
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();

        let relative = path.strip_prefix(&wheel).expect("walkdir starts with root");
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        synchronized_copy(path, &out_path, locks)?;

        count += 1;
    }

    Ok(count)
}

/// Extract a wheel by hard-linking all of its files into site packages.
fn hardlink_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
) -> Result<usize, Error> {
    let mut attempt = Attempt::default();
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();

        let relative = path.strip_prefix(&wheel).expect("walkdir starts with root");
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of hard-linking.
        if path.ends_with("RECORD") {
            synchronized_copy(path, &out_path, locks)?;
            count += 1;
            continue;
        }

        // Fallback to copying if hardlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (initial attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        if fs::hard_link(path, &tempfile).is_ok() {
                            fs_err::rename(&tempfile, &out_path)?;
                        } else {
                            debug!(
                                "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                                out_path.display(),
                                path.display()
                            );
                            synchronized_copy(path, &out_path, locks)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        debug!(
                            "Failed to hardlink `{}` to `{}`, attempting to copy files as a fallback",
                            out_path.display(),
                            path.display()
                        );
                        synchronized_copy(path, &out_path, locks)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = fs::hard_link(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (subsequent attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        fs::hard_link(path, &tempfile)?;
                        fs_err::rename(&tempfile, &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                synchronized_copy(path, &out_path, locks)?;
                warn_user_once!("Failed to hardlink files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, hardlinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.");
            }
        }

        count += 1;
    }

    Ok(count)
}

/// Extract a wheel by symbolically-linking all of its files into site packages.
fn symlink_wheel_files(
    site_packages: impl AsRef<Path>,
    wheel: impl AsRef<Path>,
    locks: &Locks,
) -> Result<usize, Error> {
    let mut attempt = Attempt::default();
    let mut count = 0usize;

    // Walk over the directory.
    for entry in WalkDir::new(&wheel) {
        let entry = entry?;
        let path = entry.path();

        let relative = path.strip_prefix(&wheel).unwrap();
        let out_path = site_packages.as_ref().join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        // The `RECORD` file is modified during installation, so we copy it instead of symlinking.
        if path.ends_with("RECORD") {
            synchronized_copy(path, &out_path, locks)?;
            count += 1;
            continue;
        }

        // Fallback to copying if symlinks aren't supported for this installation.
        match attempt {
            Attempt::Initial => {
                // Once https://github.com/rust-lang/rust/issues/86442 is stable, use that.
                attempt = Attempt::Subsequent;
                if let Err(err) = create_symlink(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (initial attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        if create_symlink(path, &tempfile).is_ok() {
                            fs::rename(&tempfile, &out_path)?;
                        } else {
                            debug!(
                                "Failed to symlink `{}` to `{}`, attempting to copy files as a fallback",
                                out_path.display(),
                                path.display()
                            );
                            synchronized_copy(path, &out_path, locks)?;
                            attempt = Attempt::UseCopyFallback;
                        }
                    } else {
                        debug!(
                            "Failed to symlink `{}` to `{}`, attempting to copy files as a fallback",
                            out_path.display(),
                            path.display()
                        );
                        synchronized_copy(path, &out_path, locks)?;
                        attempt = Attempt::UseCopyFallback;
                    }
                }
            }
            Attempt::Subsequent => {
                if let Err(err) = create_symlink(path, &out_path) {
                    // If the file already exists, remove it and try again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        debug!(
                            "File already exists (subsequent attempt), overwriting: {}",
                            out_path.display()
                        );
                        // Removing and recreating would lead to race conditions.
                        let tempdir = tempdir_in(&site_packages)?;
                        let tempfile = tempdir.path().join(entry.file_name());
                        create_symlink(path, &tempfile)?;
                        fs::rename(&tempfile, &out_path)?;
                    } else {
                        return Err(err.into());
                    }
                }
            }
            Attempt::UseCopyFallback => {
                synchronized_copy(path, &out_path, locks)?;
                warn_user_once!("Failed to symlink files; falling back to full copy. This may lead to degraded performance.\n         If the cache and target directories are on different filesystems, symlinking may not be supported.\n         If this is intentional, set `export UV_LINK_MODE=copy` or use `--link-mode=copy` to suppress this warning.");
            }
        }

        count += 1;
    }

    Ok(count)
}

/// Copy from `from` to `to`, ensuring that the parent directory is locked. Avoids simultaneous
/// writes to the same file, which can lead to corruption.
///
/// See: <https://github.com/astral-sh/uv/issues/4831>
fn synchronized_copy(from: &Path, to: &Path, locks: &Locks) -> std::io::Result<()> {
    // Ensure we have a lock for the directory.
    let dir_lock = {
        let mut locks_guard = locks.0.lock().unwrap();
        locks_guard
            .entry(to.parent().unwrap().to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };

    // Acquire a lock on the directory.
    let _dir_guard = dir_lock.lock().unwrap();

    // Copy the file, which will also set its permissions.
    fs::copy(from, to)?;

    Ok(())
}

#[cfg(unix)]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn create_symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> std::io::Result<()> {
    if original.as_ref().is_dir() {
        std::os::windows::fs::symlink_dir(original, link)
    } else {
        std::os::windows::fs::symlink_file(original, link)
    }
}
