use std::path::PathBuf;

use crate::sysconfig::SysconfigPaths;

/// The layout of a virtual environment.
#[derive(Debug)]
pub struct Virtualenv {
    /// The absolute path to the root of the virtualenv, e.g., `/path/to/.venv`.
    pub root: PathBuf,

    /// The path to the Python interpreter inside the virtualenv, e.g., `.venv/bin/python`
    /// (Unix, Python 3.11).
    pub executable: PathBuf,

    /// The `sysconfig` paths for the virtualenv, as returned by `sysconfig.get_paths()`.
    pub sysconfig_paths: SysconfigPaths,
}
