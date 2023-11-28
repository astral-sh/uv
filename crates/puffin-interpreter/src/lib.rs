use std::io;
use std::path::PathBuf;
use std::time::SystemTimeError;

use thiserror::Error;

pub use crate::interpreter::Interpreter;
pub use crate::virtual_env::Virtualenv;

mod interpreter;
mod python_platform;
mod virtual_env;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected {0} to be a virtual environment, but pyvenv.cfg is missing")]
    MissingPyVenvCfg(PathBuf),
    #[error("Your virtualenv at {0} is broken. It contains a pyvenv.cfg but no python at {1}")]
    BrokenVenv(PathBuf, PathBuf),
    #[error("Both VIRTUAL_ENV and CONDA_PREFIX are set. Please unset one of them.")]
    Conflict,
    #[error("Couldn't find a virtualenv or conda environment (Looked for VIRTUAL_ENV, CONDA_PREFIX and .venv)")]
    NotFound,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid modified date on {0}")]
    SystemTime(PathBuf, #[source] SystemTimeError),
    #[error("Failed to query python interpreter at {interpreter}")]
    PythonSubcommandLaunch {
        interpreter: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("{message}:\n--- stdout:\n{stdout}\n--- stderr:\n{stderr}\n---")]
    PythonSubcommandOutput {
        message: String,
        stdout: String,
        stderr: String,
    },
    #[error("Failed to write to cache")]
    Serde(#[from] serde_json::Error),
}
