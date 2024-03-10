//! Find matching Python interpreter and query information about python interpreter.
//!
//! * The `venv` subcommand uses [`find_requested_python`] if `-p`/`--python` is used and
//!   `find_default_python` otherwise.
//! * The `compile` subcommand uses [`find_best_python`].
//! * The `sync`, `install`, `uninstall`, `freeze`, `list` and `show` subcommands use
//!   [`find_default_python`] when `--python` is used, [`find_default_python`] when `--system` is used
//!   and the current venv by default.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

use thiserror::Error;

pub use crate::cfg::PyVenvConfiguration;
pub use crate::find_python::{find_best_python, find_default_python, find_requested_python};
pub use crate::interpreter::Interpreter;
pub use crate::python_environment::PythonEnvironment;
pub use crate::python_version::PythonVersion;
pub use crate::virtualenv::Virtualenv;

mod cfg;
mod find_python;
mod interpreter;
mod python_environment;
mod python_version;
mod virtualenv;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected `{0}` to be a virtualenv, but `pyvenv.cfg` is missing")]
    MissingPyVenvCfg(PathBuf),
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
    #[error("{message} with {exit_code}\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    PythonSubcommandOutput {
        message: String,
        exit_code: ExitStatus,
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
