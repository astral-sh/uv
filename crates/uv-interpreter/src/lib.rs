use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

pub use crate::cfg::PyVenvConfiguration;
pub use crate::interpreter::Interpreter;
pub use crate::python_environment::PythonEnvironment;
pub use crate::python_query::{find_default_python, find_requested_python};
pub use crate::python_version::PythonVersion;

mod cfg;
mod interpreter;
mod python_environment;
mod python_query;
mod python_version;
mod virtualenv_layout;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected `{0}` to be a virtualenv, but pyvenv.cfg is missing")]
    MissingPyVenvCfg(PathBuf),
    #[error("Broken virtualenv `{0}`, it contains a pyvenv.cfg but no Python binary at `{1}`")]
    BrokenVenv(PathBuf, PathBuf),
    #[error("Both VIRTUAL_ENV and CONDA_PREFIX are set. Please unset one of them.")]
    Conflict,
    #[error("No versions of Python could be found. Is Python installed?")]
    PythonNotFound,
    #[error("Failed to locate a virtualenv or Conda environment (checked: `VIRTUAL_ENV`, `CONDA_PREFIX`, and `.venv`). Run `uv venv` to create a virtualenv.")]
    VenvNotFound,
    #[error("Failed to locate Python interpreter at: `{0}`")]
    RequestedPythonNotFound(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to query python interpreter `{interpreter}`")]
    PythonSubcommandLaunch {
        interpreter: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to run `py --list-paths` to find Python installations. Is Python installed?")]
    PyList(#[source] io::Error),
    #[cfg(windows)]
    #[error(
        "No Python {0} found through `py --list-paths` or in `PATH`. Is Python {0} installed?"
    )]
    NoSuchPython(String),
    #[cfg(unix)]
    #[error("No Python {0} In `PATH`. Is Python {0} installed?")]
    NoSuchPython(String),
    #[error("Neither `python` nor `python3` are in `PATH`. Is Python installed?")]
    NoPythonInstalledUnix,
    #[error(
        "Could not find `python.exe` through `py --list-paths` or in 'PATH'. Is Python installed?"
    )]
    NoPythonInstalledWindows,
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    PythonSubcommandOutput {
        message: String,
        stdout: String,
        stderr: String,
    },
    #[error("Python 2 or older is not supported. Please use Python 3 or newer.")]
    Python2OrOlder,
    #[error("Failed to write to cache")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("Broken virtualenv: Failed to parse pyvenv.cfg")]
    Cfg(#[from] cfg::Error),
    #[error("Error finding `{}` in PATH", _0.to_string_lossy())]
    WhichError(OsString, #[source] which::Error),
}
