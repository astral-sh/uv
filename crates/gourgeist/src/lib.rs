use std::io;
use std::path::Path;

use camino::{FromPathError, Utf8Path};
use thiserror::Error;

use platform_host::PlatformError;
use uv_interpreter::{Interpreter, Virtualenv};

pub use crate::bare::create_bare_venv;

mod bare;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to determine python interpreter to use")]
    InterpreterError(#[from] uv_interpreter::Error),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error("Reserved key used for pyvenv.cfg: {0}")]
    ReservedConfigKey(String),
}

/// The value to use for the shell prompt when inside a virtual environment.
#[derive(Debug)]
pub enum Prompt {
    /// Use the current directory name as the prompt.
    CurrentDirectoryName,
    /// Use the fixed string as the prompt.
    Static(String),
    /// Default to no prompt. The prompt is then set by the activator script
    /// to the virtual environment's directory name.
    None,
}

impl Prompt {
    /// Determine the prompt value to be used from the command line arguments.
    pub fn from_args(prompt: Option<String>) -> Self {
        match prompt {
            Some(prompt) if prompt == "." => Self::CurrentDirectoryName,
            Some(prompt) => Self::Static(prompt),
            None => Self::None,
        }
    }
}

/// Create a virtualenv.
pub fn create_venv(
    location: &Path,
    interpreter: Interpreter,
    prompt: Prompt,
    extra_cfg: Vec<(String, String)>,
) -> Result<Virtualenv, Error> {
    let location: &Utf8Path = location
        .try_into()
        .map_err(|err: FromPathError| err.into_io_error())?;
    let paths = create_bare_venv(location, &interpreter, prompt, extra_cfg)?;
    Ok(Virtualenv::from_interpreter(
        interpreter,
        paths.root.as_std_path(),
    ))
}
