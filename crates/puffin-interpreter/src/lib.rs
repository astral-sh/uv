use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::time::SystemTimeError;

use thiserror::Error;

pub use crate::cfg::Configuration;
pub use crate::interpreter::Interpreter;
pub use crate::python_query::{find_default_python, find_requested_python};
pub use crate::python_version::PythonVersion;
pub use crate::virtual_env::Virtualenv;

mod cfg;
mod interpreter;
mod python_platform;
mod python_query;
mod python_version;
mod virtual_env;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected {0} to be a virtual environment, but pyvenv.cfg is missing")]
    MissingPyVenvCfg(PathBuf),
    #[error("Detected a broken virtualenv at: {0}. It contains a pyvenv.cfg but no Python binary at: {1}")]
    BrokenVenv(PathBuf, PathBuf),
    #[error("Both VIRTUAL_ENV and CONDA_PREFIX are set. Please unset one of them.")]
    Conflict,
    #[error("No versions of Python could be found. Is Python installed?")]
    PythonNotFound,
    #[error("Could not find `{0}` in PATH")]
    WhichNotFound(String, #[source] which::Error),
    #[error("Failed to locate a virtualenv or Conda environment (checked: `VIRTUAL_ENV`, `CONDA_PREFIX`, and `.venv`). Run `puffin venv` to create a virtual environment.")]
    NotFound,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid modified date on {0}")]
    SystemTime(PathBuf, #[source] SystemTimeError),
    #[error("Failed to query python interpreter at: {interpreter}")]
    PythonSubcommandLaunch {
        interpreter: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to run `py --list-paths` to find Python installations. Do you need to install Python?")]
    PyList(#[source] io::Error),
    #[error("No Python {major}.{minor} found through `py --list-paths`")]
    NoSuchPython { major: u8, minor: u8 },
    #[error("Neither `python` nor `python3` are in `PATH`. Do you need to install Python?")]
    NoPythonInstalledUnix,
    #[error("Could not find `python.exe` in PATH and `py --list-paths` did not list any Python versions. Do you need to install Python?")]
    NoPythonInstalledWindows,
    #[error("Patch versions cannot be requested on Windows")]
    PatchVersionRequestedWindows,
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    PythonSubcommandOutput {
        message: String,
        stdout: String,
        stderr: String,
    },
    #[error("Failed to write to cache")]
    Serde(#[from] serde_json::Error),
    #[error("Cache deserialization failed")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("Cache serialization failed")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("Failed to parse pyvenv.cfg")]
    Cfg(#[from] cfg::Error),
    #[error("Couldn't find `{}` in PATH", _0.to_string_lossy())]
    Which(OsString, #[source] which::Error),
}
