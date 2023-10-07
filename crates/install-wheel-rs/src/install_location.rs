//! Multiplexing between venv install and monotrail install

use fs2::FileExt;
use fs_err as fs;
use fs_err::File;
use std::io;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use tracing::{error, warn};

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
                self.lockfile.path().display(),
                err
            );
        }
    }
}

impl Deref for LockedDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

/// Multiplexing between venv install and monotrail install
///
/// For monotrail, we have a structure that is {monotrail}/{normalized(name)}/{version}/tag
///
/// We use a lockfile to prevent multiple instance writing stuff on the same time
/// As of pip 22.0, e.g. `pip install numpy; pip install numpy; pip install numpy` will
/// nondeterministically fail
///
/// I was also thinking about making a shared lock on the import side, but monotrail install
/// is supposedly atomic (by directory renaming), while for venv installation there can't be
/// atomicity (we need to add lots of different file without a top level directory / key-turn
/// file we could rename) and the locking would also need to happen in the import mechanism
/// itself to ensure
pub enum InstallLocation<T: Deref<Target = Path>> {
    Venv {
        /// absolute path
        venv_base: T,
        python_version: (u8, u8),
    },
    Monotrail {
        monotrail_root: T,
        python: PathBuf,
        python_version: (u8, u8),
    },
}

impl<T: Deref<Target = Path>> InstallLocation<T> {
    /// Returns the location of the python interpreter
    pub fn get_python(&self) -> PathBuf {
        match self {
            InstallLocation::Venv { venv_base, .. } => {
                if cfg!(windows) {
                    venv_base.join("Scripts").join("python.exe")
                } else {
                    // canonicalize on python would resolve the symlink
                    venv_base.join("bin").join("python")
                }
            }
            // TODO: For monotrail use the monotrail launcher
            InstallLocation::Monotrail { python, .. } => python.clone(),
        }
    }

    pub fn get_python_version(&self) -> (u8, u8) {
        match self {
            InstallLocation::Venv { python_version, .. } => *python_version,
            InstallLocation::Monotrail { python_version, .. } => *python_version,
        }
    }

    /// TODO: This function is unused?
    pub fn is_installed(&self, normalized_name: &str, version: &str) -> bool {
        match self {
            InstallLocation::Venv {
                venv_base,
                python_version,
            } => {
                let site_packages = if cfg!(target_os = "windows") {
                    venv_base.join("Lib").join("site-packages")
                } else {
                    venv_base
                        .join("lib")
                        .join(format!("python{}.{}", python_version.0, python_version.1))
                        .join("site-packages")
                };
                site_packages
                    .join(format!("{}-{}.dist-info", normalized_name, version))
                    .is_dir()
            }
            InstallLocation::Monotrail { monotrail_root, .. } => monotrail_root
                .join(format!("{}-{}", normalized_name, version))
                .is_dir(),
        }
    }
}

impl InstallLocation<PathBuf> {
    pub fn acquire_lock(&self) -> io::Result<InstallLocation<LockedDir>> {
        let root = match self {
            Self::Venv { venv_base, .. } => venv_base,
            Self::Monotrail { monotrail_root, .. } => monotrail_root,
        };

        // If necessary, create monotrail dir
        fs::create_dir_all(root)?;

        let locked_dir = if let Some(locked_dir) = LockedDir::try_acquire(root)? {
            locked_dir
        } else {
            warn!(
                "Could not acquire exclusive lock for installing, is another installation process \
                running? Sleeping until lock becomes free"
            );
            LockedDir::acquire(root)?
        };

        Ok(match self {
            Self::Venv { python_version, .. } => InstallLocation::Venv {
                venv_base: locked_dir,
                python_version: *python_version,
            },
            Self::Monotrail {
                python_version,
                python,
                ..
            } => InstallLocation::Monotrail {
                monotrail_root: locked_dir,
                python: python.clone(),
                python_version: *python_version,
            },
        })
    }
}
