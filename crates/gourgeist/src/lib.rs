use std::io;
use std::path::Path;

use camino::{FromPathError, Utf8Path};
use thiserror::Error;

pub use interpreter::parse_python_cli;
use platform_host::PlatformError;
use uv_interpreter::{Interpreter, Virtualenv};

pub use crate::bare::create_bare_venv;

mod bare;
mod interpreter;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to determine python interpreter to use")]
    InvalidPythonInterpreter(#[source] Box<dyn std::error::Error + Sync + Send>),
    #[error(transparent)]
    Platform(#[from] PlatformError),
}

/// Create a virtualenv.
pub fn create_venv(location: &Path, interpreter: Interpreter) -> Result<Virtualenv, Error> {
    let location: &Utf8Path = location
        .try_into()
        .map_err(|err: FromPathError| err.into_io_error())?;
    let paths = create_bare_venv(location, &interpreter)?;
    Ok(Virtualenv::from_interpreter(
        interpreter,
        paths.root.as_std_path(),
    ))
}
