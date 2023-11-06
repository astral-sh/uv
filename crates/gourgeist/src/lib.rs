use std::io;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

pub use interpreter::parse_python_cli;
use platform_host::PlatformError;
use puffin_interpreter::{InterpreterInfo, Virtualenv};

use crate::bare::create_bare_venv;

mod bare;
mod interpreter;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to determine python interpreter to use")]
    InvalidPythonInterpreter(#[source] Box<dyn std::error::Error + Sync + Send>),
    #[error("{0} is not a valid UTF-8 path")]
    NonUTF8Path(PathBuf),
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

/// Create a virtualenv.
pub fn create_venv(
    location: impl Into<PathBuf>,
    base_python: impl AsRef<Path>,
    info: &InterpreterInfo,
) -> Result<Virtualenv, Error> {
    let location = Utf8PathBuf::from_path_buf(location.into()).map_err(Error::NonUTF8Path)?;
    let base_python = Utf8Path::from_path(base_python.as_ref())
        .ok_or_else(|| Error::NonUTF8Path(base_python.as_ref().to_path_buf()))?;
    let paths = create_bare_venv(&location, base_python, info)?;
    Ok(Virtualenv::new_prefix(paths.root.as_std_path(), info))
}
