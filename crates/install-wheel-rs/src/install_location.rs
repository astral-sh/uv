use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use fs_err::File;
use tracing::{error, warn};

use uv_fs::Normalized;

const INSTALL_LOCKFILE: &str = "install-wheel-rs.lock";

/// I'm not sure that's the right way to normalize here, but it's a single place to change
/// everything.
///
/// For displaying to the user, `-` is better, and it's also what poetry lockfile 2.0 does
///
/// Keep in sync with `find_distributions`
pub fn normalize_name(dep_name: &str) -> String {
    dep_name.to_lowercase().replace(['.', '_'], "-")
}

/// A directory for which we acquired a install-wheel-rs.lock lockfile
pub struct LockedDir {
    /// The directory to lock
    path: PathBuf,
    /// handle on the install-wheel-rs.lock that drops the lock
    lockfile: File,
}

impl LockedDir {
    /// Tries to lock the directory, returns Ok(None) if it is already locked
    pub fn try_acquire(path: &Path) -> io::Result<Option<Self>> {
        let lockfile = File::create(path.join(INSTALL_LOCKFILE))?;
        if lockfile.file().try_lock_exclusive().is_ok() {
            Ok(Some(Self {
                path: path.to_path_buf(),
                lockfile,
            }))
        } else {
            Ok(None)
        }
    }

    /// Locks the directory, if necessary blocking until the lock becomes free
    pub fn acquire(path: &Path) -> io::Result<Self> {
        let lockfile = File::create(path.join(INSTALL_LOCKFILE))?;
        lockfile.file().lock_exclusive()?;
        Ok(Self {
            path: path.to_path_buf(),
            lockfile,
        })
    }
}

impl Drop for LockedDir {
    fn drop(&mut self) {
        if let Err(err) = self.lockfile.file().unlock() {
            error!(
                "Failed to unlock {}: {}",
                self.lockfile.path().normalized_display(),
                err
            );
        }
    }
}

impl AsRef<Path> for LockedDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// A virtual environment into which a wheel can be installed.
///
/// We use a lockfile to prevent multiple instance writing stuff on the same time
/// As of pip 22.0, e.g. `pip install numpy; pip install numpy; pip install numpy` will
/// non-deterministically fail.
pub struct InstallLocation<T> {
    /// absolute path
    venv_root: T,
    python_version: (u8, u8),
}

impl<T: AsRef<Path>> InstallLocation<T> {
    pub fn new(venv_base: T, python_version: (u8, u8)) -> Self {
        Self {
            venv_root: venv_base,
            python_version,
        }
    }

    /// Returns the location of the `python` interpreter.
    pub fn python(&self) -> PathBuf {
        if cfg!(unix) {
            // canonicalize on python would resolve the symlink
            self.venv_root.as_ref().join("bin").join("python")
        } else if cfg!(windows) {
            self.venv_root.as_ref().join("Scripts").join("python.exe")
        } else {
            unimplemented!("Only Windows and Unix are supported")
        }
    }

    pub fn python_version(&self) -> (u8, u8) {
        self.python_version
    }

    pub fn venv_root(&self) -> &T {
        &self.venv_root
    }
}

impl InstallLocation<PathBuf> {
    pub fn acquire_lock(&self) -> io::Result<InstallLocation<LockedDir>> {
        let locked_dir = if let Some(locked_dir) = LockedDir::try_acquire(&self.venv_root)? {
            locked_dir
        } else {
            warn!(
                "Could not acquire exclusive lock for installing, is another installation process \
                running? Sleeping until lock becomes free"
            );
            LockedDir::acquire(&self.venv_root)?
        };

        Ok(InstallLocation {
            venv_root: locked_dir,
            python_version: self.python_version,
        })
    }
}
