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

use uv_fs::Simplified;

pub use crate::environment::PythonEnvironment;
pub use crate::find_python::{find_best_python, find_default_python, find_requested_python};
pub use crate::interpreter::Interpreter;
use crate::interpreter::InterpreterInfoError;
pub use crate::pointer_size::PointerSize;
pub use crate::python_version::PythonVersion;
pub use crate::target::Target;
pub use crate::virtualenv::{PyVenvConfiguration, VirtualEnvironment};

mod environment;
mod find_python;
mod implementation;
mod interpreter;
pub mod managed;
pub mod platform;
mod pointer_size;
mod py_launcher;
mod python_version;
mod target;
mod virtualenv;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected `{}` to be a virtualenv, but `pyvenv.cfg` is missing", _0.user_display())]
    MissingPyVenvCfg(PathBuf),
    #[error("No versions of Python could be found. Is Python installed?")]
    PythonNotFound,
    #[error("Failed to locate a virtualenv or Conda environment (checked: `VIRTUAL_ENV`, `CONDA_PREFIX`, and `.venv`). Run `uv venv` to create a virtualenv.")]
    VenvNotFound,
    #[error("Virtualenv does not exist at: `{}`", _0.user_display())]
    VenvDoesNotExist(PathBuf),
    #[error("Failed to locate Python interpreter at: `{0}`")]
    RequestedPythonNotFound(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to query Python interpreter at `{interpreter}`")]
    PythonSubcommandLaunch {
        interpreter: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error(transparent)]
    PyLauncher(#[from] py_launcher::Error),
    #[cfg(windows)]
    #[error(
        "No Python {0} found through `py --list-paths` or in `PATH`. Is Python {0} installed?"
    )]
    NoSuchPython(String),
    #[cfg(unix)]
    #[error("No Python {0} in `PATH`. Is Python {0} installed?")]
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
    #[error("Requested Python version ({0}) is unsupported")]
    UnsupportedPython(String),
    #[error("Failed to write to cache")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("Broken virtualenv: Failed to parse pyvenv.cfg")]
    Cfg(#[from] virtualenv::Error),
    #[error("Error finding `{}` in PATH", _0.to_string_lossy())]
    WhichError(OsString, #[source] which::Error),
    #[error("Can't use Python at `{interpreter}`")]
    QueryScript {
        #[source]
        err: InterpreterInfoError,
        interpreter: PathBuf,
    },
}
