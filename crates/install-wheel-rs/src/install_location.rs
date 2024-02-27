use std::path::{Path, PathBuf};

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
