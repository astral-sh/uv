use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

pub use crate::cfg::Configuration;
pub use crate::interpreter::Interpreter;
pub use crate::python_query::{find_default_python, find_requested_python};
pub use crate::python_version::PythonVersion;
pub use crate::virtual_env::Virtualenv;
use crate::Error::WhichError;

mod cfg;
mod interpreter;
mod python_platform;
mod python_query;
mod python_version;
mod virtual_env;

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
    #[error("Failed to locate a virtualenv or Conda environment (checked: `VIRTUAL_ENV`, `CONDA_PREFIX`, and `.venv`). Run `puffin venv` to create a virtualenv.")]
    NotFound,
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
    #[error("No Python {major}.{minor} found through `py --list-paths`. Is Python {major}.{minor} installed?")]
    NoSuchPython { major: u8, minor: u8 },
    #[error("Neither `python` nor `python3` are in `PATH`. Is Python installed?")]
    NoPythonInstalledUnix,
    #[error("Could not find `python.exe` in PATH and `py --list-paths` did not list any Python versions. Is Python installed?")]
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
    Encode(#[from] rmp_serde::encode::Error),
    #[error("Broken virtualenv: Failed to parse pyvenv.cfg")]
    Cfg(#[from] cfg::Error),
    #[error("Error finding `{}` in PATH", _0.to_string_lossy())]
    WhichError(OsString, #[source] which::Error),
    #[error("Couldn't find `{}` in PATH. Is this Python version installed?", _0.to_string_lossy())]
    NotInPath(OsString),
}

impl Error {
    pub(crate) fn from_which_error(requested: OsString, source: which::Error) -> Self {
        match source {
            which::Error::CannotFindBinaryPath => Self::NotInPath(requested),
            _ => WhichError(requested, source),
        }
    }
}
